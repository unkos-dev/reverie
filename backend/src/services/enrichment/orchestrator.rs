//! Per-manifestation enrichment flow.
//!
//! * Load canonical current state + enabled sources.
//! * Build a [`LookupKey`] (ISBN preferred, title+author fallback).
//! * Parallel fan-out to every enabled source, bounded by the fetch budget.
//! * Write each source's raw response to `api_cache`.
//! * Upsert one `metadata_versions` journal row per field result.
//! * Compute per-field quorum.  Call [`policy::decide`] with the lock + pending
//!   state already resolved.
//! * For any `Decision::Apply` on a scalar field: UPDATE canonical column +
//!   `*_version_id` pointer inside the transaction.  On ISBN changes call
//!   [`crate::models::work::rematch_on_isbn_change`] immediately.
//!
//! Cover downloads are deferred to Step 11 (Library Health); sources that
//! report cover URLs surface them as `cover_url` observations, but nothing
//! in this orchestrator fetches them.

use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::models::work;
use crate::services::enrichment::cache::{self, ApiCacheKind, CacheTtls};
use crate::services::enrichment::confidence;
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::http::api_client;
use crate::services::enrichment::lookup_key;
use crate::services::enrichment::policy::{self, Decision, PolicyInputRow};
use crate::services::enrichment::sources::{
    LookupCtx, LookupKey, MetadataSource, SourceError, SourceResult, google_books::GoogleBooks,
    hardcover::Hardcover, open_library::OpenLibrary,
};
use crate::services::enrichment::value_hash;

/// Outcome of a single [`run_once`] call.  Returned to the queue layer so it
/// can drive retry/skipped state transitions.
#[derive(Debug, Clone)]
#[allow(dead_code)] // consumed by queue.rs + tracing
pub struct RunOutcome {
    pub manifestation_id: Uuid,
    pub applied: usize,
    pub staged: usize,
    pub skipped_locked: usize,
    pub source_failures: Vec<SourceFailure>,
    pub duplicate_suspected: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // source_id + error are surfaced via tracing
pub struct SourceFailure {
    pub source_id: String,
    pub error: String,
    /// Populated when the source reported HTTP 429.  The queue uses this to
    /// schedule the next retry attempt.
    pub retry_after: Option<Duration>,
    /// True if the error was non-retryable (4xx other than 429).
    pub terminal: bool,
}

/// Snapshot of canonical field state + lookup key, shared between
/// [`run_once`] and [`crate::services::enrichment::dry_run`].
#[derive(Debug)]
pub struct Snapshot {
    pub manifestation_id: Uuid,
    pub work_id: Uuid,
    pub lookup_key: Option<LookupKey>,
    pub canonical: CanonicalState,
}

#[derive(Debug, Default, Clone)]
pub struct CanonicalState {
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub pub_date: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
}

impl CanonicalState {
    pub fn is_empty_for(&self, field: &str) -> bool {
        fn blank(v: &Option<String>) -> bool {
            v.as_deref().unwrap_or("").is_empty()
        }
        match field {
            "title" => blank(&self.title),
            "description" => blank(&self.description),
            "language" => blank(&self.language),
            "publisher" => blank(&self.publisher),
            "pub_date" => blank(&self.pub_date),
            "isbn_10" => blank(&self.isbn_10),
            "isbn_13" => blank(&self.isbn_13),
            _ => true,
        }
    }
}

/// Build the dynamic source set from `config`.  Hardcover disables itself
/// when no token is configured.
pub fn build_sources(config: &Config) -> Vec<Arc<dyn MetadataSource>> {
    let mut v: Vec<Arc<dyn MetadataSource>> = vec![
        Arc::new(OpenLibrary::new(config.openlibrary_base_url.clone())),
        Arc::new(GoogleBooks::new(
            config.googlebooks_base_url.clone(),
            config.googlebooks_api_key.clone(),
        )),
    ];
    let hc = Hardcover::new(
        config.hardcover_base_url.clone(),
        config.hardcover_api_token.clone(),
    );
    if hc.enabled() {
        v.push(Arc::new(hc));
    } else {
        info!("hardcover disabled: token not configured");
    }
    v
}

/// Full per-manifestation run.  Writes to `api_cache`, `metadata_versions`,
/// and canonical columns atomically.
pub async fn run_once(
    pool: &PgPool,
    config: &Config,
    manifestation_id: Uuid,
) -> anyhow::Result<RunOutcome> {
    let snapshot = load_snapshot(pool, manifestation_id).await?;
    let Some(lookup_key) = snapshot.lookup_key.clone() else {
        info!(
            %manifestation_id,
            "no lookup key (missing ISBN + title/author) — nothing to enrich"
        );
        return Ok(RunOutcome {
            manifestation_id,
            applied: 0,
            staged: 0,
            skipped_locked: 0,
            source_failures: Vec::new(),
            duplicate_suspected: false,
        });
    };

    let sources = build_sources(config);
    let ua = config.user_agent();
    let http = api_client(&ua);

    let results = fan_out(
        &sources,
        &http,
        &lookup_key,
        Duration::from_secs(config.enrichment.fetch_budget_secs),
    )
    .await;

    // Persist api_cache rows for every source (success & failure) before any
    // DB mutation — caching a miss saves future calls against dead ISBNs.
    let ttls = CacheTtls {
        hit: time::Duration::days(i64::from(config.enrichment.cache_ttl_hit_days)),
        miss: time::Duration::days(i64::from(config.enrichment.cache_ttl_miss_days)),
        error: time::Duration::minutes(i64::from(config.enrichment.cache_ttl_error_mins)),
    };
    cache_all(pool, &results, &lookup_key, &ttls).await;

    // Open the single mutating transaction: journal writes + canonical updates
    // + rematch hook all commit or roll back together.
    let mut tx = pool.begin().await?;

    let (per_field, failures) = apply_journal_batch(&mut tx, manifestation_id, &results).await?;
    let canonical = apply_canonical_batch(&mut tx, &snapshot, &per_field).await?;

    tx.commit().await?;

    Ok(RunOutcome {
        manifestation_id,
        applied: canonical.applied,
        staged: canonical.staged,
        skipped_locked: canonical.skipped_locked,
        source_failures: failures,
        duplicate_suspected: canonical.duplicate_suspected,
    })
}

#[derive(Default)]
struct CanonicalBatchOutcome {
    applied: usize,
    staged: usize,
    skipped_locked: usize,
    duplicate_suspected: bool,
}

type PerFieldRows = std::collections::HashMap<String, Vec<(String, PolicyInputRow)>>;

/// Upsert one `metadata_versions` row per source result and bucket the
/// resulting `(source_id, PolicyInputRow)` pairs by field.  Source-level
/// errors are summarised into `failures` (no journal row written).
async fn apply_journal_batch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    results: &[SourceRun],
) -> sqlx::Result<(PerFieldRows, Vec<SourceFailure>)> {
    let mut per_field: PerFieldRows = std::collections::HashMap::new();
    let mut failures = Vec::new();

    for r in results {
        match &r.outcome {
            Ok(source_results) => {
                for sr in source_results {
                    let id = upsert_journal_row(tx, manifestation_id, &r.source_id, sr).await?;
                    per_field.entry(sr.field_name.clone()).or_default().push((
                        r.source_id.clone(),
                        PolicyInputRow {
                            id,
                            value_hash: value_hash::value_hash(&sr.field_name, &sr.raw_value),
                        },
                    ));
                }
            }
            Err(err) => failures.push(summarise_failure(&r.source_id, err)),
        }
    }

    Ok((per_field, failures))
}

/// For each field, compute confidence per upserted row, decide via
/// [`policy::decide`], apply canonical updates inside the same transaction,
/// and trigger ISBN rematch + writeback enqueue when applicable.
async fn apply_canonical_batch(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    per_field: &PerFieldRows,
) -> anyhow::Result<CanonicalBatchOutcome> {
    let manifestation_id = snapshot.manifestation_id;
    let mut outcome = CanonicalBatchOutcome::default();

    for (field, rows) in per_field {
        let entity = if is_work_field(field) {
            EntityType::Work
        } else {
            EntityType::Manifestation
        };
        let locked = field_lock::is_locked_tx(tx, manifestation_id, entity, field).await?;

        let canonical_empty = snapshot.canonical.is_empty_for(field);

        // Existing pending rows from prior runs (other value_hashes).
        let existing_pending = load_existing_pending(tx, manifestation_id, field).await?;

        // quorum counts distinct rows in *this* run with the same hash.
        for (source_id, incoming) in rows {
            let quorum = rows
                .iter()
                .filter(|(_, r)| r.value_hash == incoming.value_hash)
                .count() as u32;
            // Pull the authoritative match_type back from the row we just
            // upserted — it may be 'isbn', 'title_author_fuzzy', or 'title'
            // depending on the source path.
            let match_type: String =
                sqlx::query_scalar("SELECT match_type FROM metadata_versions WHERE id = $1")
                    .bind(incoming.id)
                    .fetch_one(&mut **tx)
                    .await?;
            let confidence_score = confidence::score(source_id, &match_type, quorum);
            sqlx::query("UPDATE metadata_versions SET confidence_score = $1 WHERE id = $2")
                .bind(confidence_score)
                .bind(incoming.id)
                .execute(&mut **tx)
                .await?;
            tracing::debug!(
                %field, source_id, quorum, %match_type, confidence_score,
                "confidence computed"
            );

            // Combine pending from this run with stored pending rows.
            let mut pending_set: Vec<PolicyInputRow> = existing_pending.clone();
            for (_, other) in rows.iter() {
                if other.id != incoming.id {
                    pending_set.push(other.clone());
                }
            }

            let decision = policy::decide(field, canonical_empty, incoming, locked, &pending_set);

            match decision {
                Decision::Apply(version_id) => {
                    if !apply_field(tx, snapshot, field, version_id).await? {
                        continue;
                    }
                    outcome.applied += 1;
                    info!(
                        %manifestation_id, %field, %version_id, source_id,
                        "enrichment: metadata.applied"
                    );
                    if field == "isbn_10" || field == "isbn_13" {
                        let rematch = work::rematch_on_isbn_change(tx, manifestation_id).await?;
                        match rematch {
                            work::RematchOutcome::NoOp => {}
                            work::RematchOutcome::Suspected { .. } => {
                                outcome.duplicate_suspected = true;
                                warn!(
                                    %manifestation_id,
                                    "enrichment: work.duplicate_suspected"
                                );
                            }
                            work::RematchOutcome::AutoMerged { from, to } => {
                                // warn! (not info!) — auto-merge is a
                                // destructive structural change: the `from`
                                // work is deleted and the manifestation is
                                // re-parented. Matches `Suspected`'s tier so
                                // both rematch outcomes are visible to
                                // operators filtering at warn level.
                                warn!(
                                    %manifestation_id, %from, %to,
                                    "enrichment: work.auto_merged"
                                );
                            }
                        }
                    }
                    // Enqueue writeback in the same tx so the pointer move
                    // and file-side reflection either both commit or both
                    // roll back.  Worker deduplicates if two fields flip on
                    // the same manifestation in <1s; not the emitter's job.
                    enqueue_writeback(tx, manifestation_id, field).await?;
                    // Avoid re-applying on the same run when two sources agree.
                    break;
                }
                Decision::Stage => {
                    outcome.staged += 1;
                    tracing::debug!(
                        %manifestation_id, %field, source_id,
                        "enrichment: metadata.staged"
                    );
                }
                Decision::NoOp => {
                    outcome.skipped_locked += 1;
                }
            }
        }
    }

    Ok(outcome)
}

/// Load the current canonical + lookup state for a manifestation.
pub async fn load_snapshot(pool: &PgPool, manifestation_id: Uuid) -> anyhow::Result<Snapshot> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT m.work_id, m.isbn_10, m.isbn_13, m.publisher, m.pub_date, \
                w.title, w.description, w.language, \
                (SELECT a.name FROM work_authors wa \
                 JOIN authors a ON a.id = wa.author_id \
                 WHERE wa.work_id = w.id \
                 ORDER BY wa.position \
                 LIMIT 1) AS first_author \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1",
    )
    .bind(manifestation_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| anyhow!("manifestation not found: {manifestation_id}"))?;

    let work_id: Uuid = row.try_get("work_id")?;
    let isbn_10: Option<String> = row.try_get("isbn_10")?;
    let isbn_13: Option<String> = row.try_get("isbn_13")?;
    let publisher: Option<String> = row.try_get("publisher")?;
    let pub_date: Option<time::Date> = row.try_get("pub_date")?;
    let title: Option<String> = row.try_get("title")?;
    let description: Option<String> = row.try_get("description")?;
    let language: Option<String> = row.try_get("language")?;
    let first_author: Option<String> = row.try_get("first_author")?;

    let canonical = CanonicalState {
        title: title.clone(),
        description,
        language,
        publisher,
        pub_date: pub_date.map(|d| d.to_string()),
        isbn_10: isbn_10.clone(),
        isbn_13: isbn_13.clone(),
    };

    let lookup_key = derive_lookup_key(&isbn_13, &isbn_10, &title, &first_author);

    Ok(Snapshot {
        manifestation_id,
        work_id,
        lookup_key,
        canonical,
    })
}

fn derive_lookup_key(
    isbn_13: &Option<String>,
    isbn_10: &Option<String>,
    title: &Option<String>,
    author: &Option<String>,
) -> Option<LookupKey> {
    if let Some(v) = isbn_13.as_deref()
        && let Some(k) = lookup_key::isbn_key(v)
    {
        return Some(LookupKey::Isbn(k));
    }
    if let Some(v) = isbn_10.as_deref()
        && let Some(k) = lookup_key::isbn_key(v)
    {
        return Some(LookupKey::Isbn(k));
    }
    if let (Some(t), Some(a)) = (title.as_deref(), author.as_deref())
        && !t.is_empty()
        && !a.is_empty()
    {
        return Some(LookupKey::TitleAuthor {
            title: t.to_string(),
            author: a.to_string(),
        });
    }
    None
}

/// One fan-out entry.
pub struct SourceRun {
    pub source_id: String,
    pub outcome: Result<Vec<SourceResult>, SourceError>,
}

/// Parallel lookup bounded by a wall-clock budget.  A slow provider cannot
/// starve the others: when the budget expires, every provider that has
/// already returned keeps its result; the rest are emitted as
/// `SourceError::Timeout` so [`finish`](super::queue) can mark the row
/// `failed` (eligible for retry) instead of silently completing it with no
/// work done.
pub async fn fan_out(
    sources: &[Arc<dyn MetadataSource>],
    http: &reqwest::Client,
    key: &LookupKey,
    budget: Duration,
) -> Vec<SourceRun> {
    let enabled_ids: Vec<String> = sources
        .iter()
        .filter(|s| s.enabled())
        .map(|s| s.id().to_string())
        .collect();

    let mut futs: FuturesUnordered<_> = sources
        .iter()
        .filter(|s| s.enabled())
        .map(|s| {
            let id = s.id().to_string();
            let s = s.clone();
            async move {
                let ctx = LookupCtx { http, cached: None };
                SourceRun {
                    source_id: id,
                    outcome: s.lookup(&ctx, key).await,
                }
            }
        })
        .collect();

    let mut done: Vec<SourceRun> = Vec::with_capacity(enabled_ids.len());
    let deadline = sleep(budget);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            biased;
            maybe_run = futs.next() => match maybe_run {
                Some(run) => done.push(run),
                None => break,
            },
            _ = &mut deadline => {
                // Budget expired: synthesise a Timeout outcome for every
                // source that hasn't reported yet so the failure surfaces
                // to `finish`.  In-flight futures are dropped (cancelled).
                for id in enabled_ids.iter() {
                    if !done.iter().any(|r| &r.source_id == id) {
                        done.push(SourceRun {
                            source_id: id.clone(),
                            outcome: Err(SourceError::Timeout),
                        });
                    }
                }
                warn!(
                    ?budget,
                    completed = done.iter().filter(|r| r.outcome.is_ok()).count(),
                    "enrichment fan-out exceeded fetch budget"
                );
                break;
            }
        }
    }

    done
}

async fn cache_all(pool: &PgPool, runs: &[SourceRun], key: &LookupKey, ttls: &CacheTtls) {
    let cache_key = key.cache_key();
    for run in runs {
        let (payload, kind, status) = match &run.outcome {
            Ok(results) if results.is_empty() => (serde_json::json!([]), ApiCacheKind::Miss, None),
            Ok(results) => (
                serde_json::to_value(results.iter().map(|r| &r.raw_value).collect::<Vec<_>>())
                    .unwrap_or_else(|e| {
                        warn!(error = %e, source = %run.source_id, "cache: failed to serialise results; writing NULL");
                        serde_json::Value::Null
                    }),
                ApiCacheKind::Hit,
                None,
            ),
            Err(SourceError::NotFound) => (serde_json::json!({}), ApiCacheKind::Miss, None),
            Err(SourceError::Http(code)) => (
                serde_json::json!({"http_status": code.as_u16()}),
                ApiCacheKind::Error,
                Some(i32::from(code.as_u16())),
            ),
            Err(SourceError::RateLimited { .. }) => (
                serde_json::json!({"status": 429}),
                ApiCacheKind::Error,
                Some(429),
            ),
            Err(SourceError::Timeout) => (
                serde_json::json!({"status": "timeout"}),
                ApiCacheKind::Error,
                None,
            ),
            Err(SourceError::Other(e)) => (
                serde_json::json!({"error": e.to_string()}),
                ApiCacheKind::Error,
                None,
            ),
        };
        if let Err(e) = cache::write(
            pool,
            &run.source_id,
            &cache_key,
            &payload,
            kind,
            status,
            ttls,
        )
        .await
        {
            warn!(error = %e, source = %run.source_id, "api_cache write failed");
        }
    }
}

async fn upsert_journal_row(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    source_id: &str,
    sr: &SourceResult,
) -> sqlx::Result<Uuid> {
    let hash = value_hash::value_hash(&sr.field_name, &sr.raw_value);
    let score = confidence::score(source_id, &sr.match_type, 1);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO metadata_versions \
             (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1 \
         RETURNING id",
    )
    .bind(manifestation_id)
    .bind(source_id)
    .bind(&sr.field_name)
    .bind(&sr.raw_value)
    .bind(&hash)
    .bind(&sr.match_type)
    .bind(score)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

async fn load_existing_pending(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: &str,
) -> sqlx::Result<Vec<PolicyInputRow>> {
    let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
        "SELECT id, value_hash FROM metadata_versions \
         WHERE manifestation_id = $1 AND field_name = $2 AND status = 'pending'",
    )
    .bind(manifestation_id)
    .bind(field)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, value_hash)| PolicyInputRow { id, value_hash })
        .collect())
}

fn is_work_field(field: &str) -> bool {
    matches!(field, "title" | "description" | "language")
}

fn is_cover_field(f: &str) -> bool {
    f == "cover" || f == "cover_url"
}

/// Enqueue a writeback job in the caller's transaction.  The pointer move
/// plus the job INSERT commit or roll back together, so the worker never
/// sees a pointer change that has no corresponding job.
async fn enqueue_writeback(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: &str,
) -> anyhow::Result<()> {
    let reason = if is_cover_field(field) {
        "cover"
    } else {
        "metadata"
    };
    sqlx::query("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2)")
        .bind(manifestation_id)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Apply a scalar field to its canonical column + `*_version_id` pointer.
///
/// Returns `true` when the apply should count toward the run's `applied`
/// tally and trigger a writeback enqueue. Returns `false` when the journal
/// value is unusable (non-string JSON, malformed `pub_date`) so the caller
/// can try the next source instead of inflating counters and enqueuing a
/// writeback for a change that did not happen.
async fn apply_field(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    field: &str,
    version_id: Uuid,
) -> sqlx::Result<bool> {
    // Pull canonical value from the journal row — serialised as JSON so we
    // have a single source of truth.
    let value: serde_json::Value =
        sqlx::query_scalar("SELECT new_value FROM metadata_versions WHERE id = $1")
            .bind(version_id)
            .fetch_one(&mut **tx)
            .await?;

    match field {
        "title" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE works SET title = $1, sort_title = lower($1), title_version_id = $2 \
                 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.work_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "description" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.work_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "language" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query("UPDATE works SET language = $1, language_version_id = $2 WHERE id = $3")
                .bind(&v)
                .bind(version_id)
                .bind(snapshot.work_id)
                .execute(&mut **tx)
                .await?;
            Ok(true)
        }
        "publisher" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "pub_date" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            // Intentional divergence from routes::metadata::apply_version,
            // which returns AppError::Validation. In the pipeline a bad
            // pub_date comes from an external source; we keep the journal
            // row so a reviewer can still accept/reject it and skip the
            // canonical write.
            let Ok(date) = parse_iso_date(&v) else {
                tracing::debug!(value = %v, "pub_date value not ISO; skipping canonical apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
            )
            .bind(date)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "isbn_10" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "isbn_13" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query(
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        // Cover fields and any other recognised non-canonical fields rely on
        // the writeback worker for the actual change (Step 11), so the
        // caller still enqueues a writeback and counts the apply.
        other if is_cover_field(other) => Ok(true),
        other => {
            tracing::debug!(field = %other, "no auto-apply handler; staying staged");
            Ok(false)
        }
    }
}

/// Coerce a JSON journal value into the scalar string that canonical
/// columns expect. `Array` and `Object` are rejected — stringifying them
/// produces raw JSON (e.g. `["Dune"]`) which would corrupt text columns
/// like `title`. `Null` is also rejected.
fn json_as_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            None
        }
    }
}

fn parse_iso_date(s: &str) -> Result<time::Date, time::error::Parse> {
    use time::format_description::well_known::Iso8601;
    // `s.len()` is in bytes; provider strings are adversarial and may contain
    // multi-byte UTF-8 codepoints. `is_char_boundary` keeps the slice valid.
    if s.len() >= 10 && s.is_char_boundary(10) {
        time::Date::parse(&s[..10], &Iso8601::DATE)
    } else {
        // Fall back to `YYYY` or `YYYY-MM` by padding.
        let padded = match s.len() {
            4 => format!("{s}-01-01"),
            7 => format!("{s}-01"),
            _ => s.to_string(),
        };
        time::Date::parse(&padded, &Iso8601::DATE)
    }
}

fn summarise_failure(source_id: &str, err: &SourceError) -> SourceFailure {
    let (retry_after, terminal) = match err {
        SourceError::RateLimited { retry_after } => (*retry_after, false),
        SourceError::Http(status) => {
            let code = status.as_u16();
            let is_4xx = (400..500).contains(&code);
            (None, is_4xx && code != 429)
        }
        _ => (None, false),
    };
    SourceFailure {
        source_id: source_id.to_string(),
        // {err:#} activates anyhow's chain formatter on
        // SourceError::Other (transparent over anyhow::Error), preserving
        // the full context chain. Other variants are simple `#[error("…")]`
        // strings — same output as `{err}`.
        error: format!("{err:#}"),
        retry_after,
        terminal,
    }
}

/// Helper used by [`dry_run::preview`] — same fan-out + cache but no journal
/// writes and no canonical updates.
pub async fn fan_out_for_dry_run(
    pool: &PgPool,
    config: &Config,
    manifestation_id: Uuid,
) -> anyhow::Result<(Snapshot, Vec<SourceRun>)> {
    let snapshot = load_snapshot(pool, manifestation_id).await?;
    let Some(key) = snapshot.lookup_key.clone() else {
        return Ok((snapshot, Vec::new()));
    };
    let sources = build_sources(config);
    let ua = config.user_agent();
    let http = api_client(&ua);
    let results = fan_out(
        &sources,
        &http,
        &key,
        Duration::from_secs(config.enrichment.fetch_budget_secs),
    )
    .await;

    let ttls = CacheTtls {
        hit: time::Duration::days(i64::from(config.enrichment.cache_ttl_hit_days)),
        miss: time::Duration::days(i64::from(config.enrichment.cache_ttl_miss_days)),
        error: time::Duration::minutes(i64::from(config.enrichment.cache_ttl_error_mins)),
    };
    cache_all(pool, &results, &key, &ttls).await;
    Ok((snapshot, results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig};
    use crate::models::manifestation_format::ManifestationFormat;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // ── fan_out budget behaviour ──────────────────────────────────────────

    struct FastSource;
    #[async_trait::async_trait]
    impl MetadataSource for FastSource {
        fn id(&self) -> &'static str {
            "fast"
        }
        fn enabled(&self) -> bool {
            true
        }
        async fn lookup(
            &self,
            _ctx: &LookupCtx<'_>,
            _key: &LookupKey,
        ) -> Result<Vec<SourceResult>, SourceError> {
            Ok(Vec::new())
        }
    }

    struct SlowSource;
    #[async_trait::async_trait]
    impl MetadataSource for SlowSource {
        fn id(&self) -> &'static str {
            "slow"
        }
        fn enabled(&self) -> bool {
            true
        }
        async fn lookup(
            &self,
            _ctx: &LookupCtx<'_>,
            _key: &LookupKey,
        ) -> Result<Vec<SourceResult>, SourceError> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(Vec::new())
        }
    }

    /// Adversarial-review D2 + C5: a hung provider must NOT discard
    /// completed siblings, and every unfinished provider must be reported
    /// as `SourceError::Timeout` so the queue marks the row `failed`
    /// (eligible for retry) instead of silently `complete`.
    #[tokio::test]
    async fn fan_out_preserves_partial_results_and_emits_timeouts() {
        let sources: Vec<Arc<dyn MetadataSource>> =
            vec![Arc::new(FastSource), Arc::new(SlowSource)];
        let http = reqwest::Client::new();
        let key = LookupKey::Isbn("9780000000000".into());

        let runs = fan_out(&sources, &http, &key, Duration::from_millis(50)).await;

        assert_eq!(
            runs.len(),
            2,
            "every enabled source must produce a SourceRun"
        );
        let fast = runs.iter().find(|r| r.source_id == "fast").unwrap();
        let slow = runs.iter().find(|r| r.source_id == "slow").unwrap();
        assert!(
            fast.outcome.is_ok(),
            "fast source result was discarded by timeout"
        );
        assert!(
            matches!(slow.outcome, Err(SourceError::Timeout)),
            "slow source should surface as Timeout, got {:?}",
            slow.outcome
        );
    }

    // Tests run against `reverie_ingestion`: that role holds the
    // `manifestations_ingestion_full_access` RLS policy which lets the
    // test fixture INSERT manifestations with `RETURNING id` without
    // setting up an `app.current_user_id` session variable. The companion
    // migration `20260417000002_grant_field_locks_select_ingestion` adds
    // the missing `SELECT` grant so the orchestrator's
    // `field_lock::is_locked_tx` call succeeds under this role.
    use crate::test_support::db::{app_pool_for, ingestion_pool_for};

    fn config_with_mock_sources(
        ol_uri: &str,
        gb_uri: &str,
        hc_uri: &str,
        hc_token: Option<&str>,
    ) -> Config {
        Config {
            port: 3000,
            database_url: String::new(),
            library_path: String::new(),
            ingestion_path: String::new(),
            quarantine_path: String::new(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            ingestion_database_url: String::new(),
            format_priority: vec![ManifestationFormat::Epub],
            cleanup_mode: CleanupMode::None,
            enrichment: EnrichmentConfig {
                enabled: true,
                concurrency: 1,
                poll_idle_secs: 30,
                fetch_budget_secs: 30,
                http_timeout_secs: 10,
                max_attempts: 3,
                cache_ttl_hit_days: 1,
                cache_ttl_miss_days: 1,
                cache_ttl_error_mins: 1,
            },
            cover: CoverConfig {
                max_bytes: 10_485_760,
                download_timeout_secs: 30,
                min_long_edge_px: 1000,
                redirect_limit: 3,
            },
            writeback: crate::config::WritebackConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 5,
                max_attempts: 3,
            },
            opds: crate::config::OpdsConfig {
                enabled: false,
                page_size: 50,
                realm: "Reverie OPDS".into(),
                public_url: None,
            },
            security: crate::config::SecurityConfig {
                behind_https: false,
                hsts_include_subdomains: false,
                hsts_preload: false,
                csp_report_endpoint: None,
                frontend_dist_path: None,
                csp_html_header: None,
                csp_api_header: None,
            },
            openlibrary_base_url: ol_uri.into(),
            googlebooks_base_url: gb_uri.into(),
            googlebooks_api_key: None,
            hardcover_base_url: hc_uri.into(),
            hardcover_api_token: hc_token.map(|s| s.into()),
            operator_contact: None,
        }
    }

    /// Insert (work + manifestation) with the given ISBN-13 and return both IDs.
    /// Canonical fields start empty so AutoFill is exercised.
    async fn insert_enrich_fixture(pool: &PgPool, isbn_13: &str, marker: &str) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let manifestation_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
               (work_id, isbn_13, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, $2, 'epub'::manifestation_format, $3, $4, $4, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(isbn_13)
        .bind(format!("/tmp/orch-{marker}.epub"))
        .bind(format!("orch-hash-{marker}"))
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, manifestation_id)
    }

    /// Build an `/api/books?bibkeys=ISBN:X&jscmd=data` mock response.
    ///
    /// Existing callers still pass the old `{title, publishers: [...]}`
    /// shape — wrap it under the `ISBN:{isbn}` bibkey, lift string
    /// publishers into `{name}` objects, and surface authors inline.  This
    /// keeps the per-test bodies compact while matching the humanised
    /// response shape the adapter now consumes.
    async fn mock_openlibrary_isbn(server: &MockServer, isbn: &str, body: serde_json::Value) {
        let entry = normalise_api_books_entry(body);
        let wrapped = serde_json::json!({ format!("ISBN:{isbn}"): entry });
        Mock::given(method("GET"))
            .and(path("/api/books"))
            .respond_with(ResponseTemplate::new(200).set_body_json(wrapped))
            .mount(server)
            .await;
    }

    /// Translate the legacy `/isbn/{isbn}.json` body shape into the
    /// `/api/books?jscmd=data` entry shape the adapter now expects.
    fn normalise_api_books_entry(mut body: serde_json::Value) -> serde_json::Value {
        if let Some(obj) = body.as_object_mut()
            && let Some(pubs) = obj.get("publishers").cloned()
            && let Some(arr) = pubs.as_array()
        {
            let lifted: Vec<serde_json::Value> = arr
                .iter()
                .map(|p| match p {
                    serde_json::Value::String(s) => serde_json::json!({"name": s}),
                    other => other.clone(),
                })
                .collect();
            obj.insert("publishers".into(), serde_json::Value::Array(lifted));
        }
        body
    }

    async fn mock_googlebooks_isbn(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/volumes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mock_hardcover(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    /// Three sources return the same title → Apply fires AND the applied
    /// row's confidence reflects the quorum=3 boost.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_multi_source_agreement_applies_with_quorum_boost(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        // Pick an ISBN that does NOT collide with the one baked into
        // `make_metadata_epub()` (9780306406157) — on test panic, lingering
        // rows would otherwise pollute the ingest-invariant tests that run
        // later in the alphabetical order.
        let isbn = "9780451524935";
        let marker = Uuid::new_v4().simple().to_string();
        let canon_title = format!("Agreement Canon {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({"title": canon_title})).await;
        mock_googlebooks_isbn(
            &gb,
            json!({"items":[{"volumeInfo":{"title": canon_title}}]}),
        )
        .await;
        mock_hardcover(&hc, json!({"data":{"books":[{"title": canon_title}]}})).await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));

        let outcome = run_once(&pool, &cfg, m_id).await.unwrap();
        // The break-after-Apply guard inside apply_canonical_batch must
        // prevent agreeing siblings from re-applying — exactly one Apply,
        // exactly one writeback row.
        assert_eq!(
            outcome.applied, 1,
            "agreement should Apply once, not once per agreeing source"
        );
        let writeback_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            writeback_rows, 1,
            "expected exactly one writeback row, got {writeback_rows}"
        );

        let canon: Option<String> = sqlx::query_scalar(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            canon.as_deref(),
            Some(canon_title.as_str()),
            "canonical title should match agreement value"
        );

        // Three sources agreed on `title`; quorum=3 boost (1.20×) must be
        // persisted on the journal rows.  The maximum quorum-1 score for any
        // ISBN-matched source is `hardcover` at 0.85; with the boost,
        // `openlibrary` reaches 0.96 — anything ≥ 0.90 proves the boost
        // landed in the row, not just the log.
        let max_score: f32 = sqlx::query_scalar(
            "SELECT MAX(confidence_score) FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            max_score >= 0.90,
            "expected quorum-boosted confidence_score >= 0.90 on title, got {max_score}"
        );
    }

    /// Three sources disagree on title → Propose downgrade — all rows stage,
    /// canonical title remains empty.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_disagreement_stages_all_candidates(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780441172719";
        let marker = Uuid::new_v4().simple().to_string();

        mock_openlibrary_isbn(&ol, isbn, json!({"title": format!("OL Title {marker}")})).await;
        mock_googlebooks_isbn(
            &gb,
            json!({"items":[{"volumeInfo":{"title": format!("GB Title {marker}")}}]}),
        )
        .await;
        mock_hardcover(
            &hc,
            json!({"data":{"books":[{"title": format!("HC Title {marker}")}]}}),
        )
        .await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));

        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        // Title journal rows written (all pending), but canonical empty.
        let title_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_rows >= 3,
            "expected ≥3 title journal rows across sources, got {title_rows}"
        );

        let canon_title: String = sqlx::query_scalar(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            canon_title.is_empty(),
            "canonical title should remain empty after disagreement, got '{canon_title}'"
        );

        let title_version_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT w.title_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_version_id.is_none(),
            "no Apply should have run, title_version_id should be NULL"
        );
    }

    /// One source returns `publisher` (AutoFill by default) on an empty
    /// canonical → Apply fires and `publisher` is written to the
    /// manifestation.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_autofill_applies_when_canonical_empty(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780061120084";
        let marker = Uuid::new_v4().simple().to_string();
        let publisher_name = format!("HarperCollins {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({"publishers": [publisher_name.clone()]})).await;
        // GoogleBooks + Hardcover return 'miss' (no items / empty books)
        mock_googlebooks_isbn(&gb, json!({"items": []})).await;
        mock_hardcover(&hc, json!({"data": {"books": []}})).await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));

        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let (publisher, publisher_ptr): (Option<String>, Option<Uuid>) = sqlx::query_as(
            "SELECT publisher, publisher_version_id FROM manifestations WHERE id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            publisher.as_deref(),
            Some(publisher_name.as_str()),
            "AutoFill on empty canonical should apply publisher"
        );
        assert!(
            publisher_ptr.is_some(),
            "publisher_version_id must be wired"
        );

        // Apply path must emit a writeback_jobs row in the same tx.
        let job_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            job_count, 1,
            "enrichment Apply must emit exactly one writeback_jobs row; got {job_count}"
        );
    }

    /// When the `title` field is locked, the journal row is still written
    /// (so admins can see what the source proposed) but canonical and
    /// title_version_id are NOT updated.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_locked_field_writes_journal_but_not_canonical(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780547928227";
        let marker = Uuid::new_v4().simple().to_string();
        let proposed_title = format!("Proposed New Title {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({"title": proposed_title})).await;
        mock_googlebooks_isbn(&gb, json!({"items": []})).await;
        mock_hardcover(&hc, json!({"data": {"books": []}})).await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        // Lock the title field on the work side.  field_locks writes require
        // reverie_app (reverie_ingestion has SELECT only) — use a separate pool.
        sqlx::query(
            "INSERT INTO field_locks (manifestation_id, entity_type, field_name) \
             VALUES ($1, 'work', 'title')",
        )
        .bind(m_id)
        .execute(&app_pool)
        .await
        .unwrap();

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        // Journal row for the proposed title WAS written.
        let title_rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_rows >= 1,
            "journal row must be written even when locked, got {title_rows}"
        );

        // Canonical title_version_id stays NULL.
        let title_ptr: Option<Uuid> = sqlx::query_scalar(
            "SELECT w.title_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_ptr.is_none(),
            "locked field must NOT set canonical pointer"
        );
        let canon_title: String = sqlx::query_scalar(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(canon_title.is_empty(), "canonical title must stay empty");
    }

    /// `dry_run::preview` fans out + fills `api_cache` but never writes to
    /// `metadata_versions`.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_dry_run_leaves_journal_unchanged_writes_api_cache(pool: PgPool) {
        use crate::services::enrichment::dry_run;

        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780553283686";
        let marker = Uuid::new_v4().simple().to_string();
        let canon_title = format!("Dry Run Title {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({"title": canon_title})).await;
        mock_googlebooks_isbn(
            &gb,
            json!({"items":[{"volumeInfo":{"title": canon_title}}]}),
        )
        .await;
        mock_hardcover(&hc, json!({"data":{"books":[{"title": canon_title}]}})).await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));

        // Baseline counts — scoped by manifestation / lookup_key so other
        // tests' rows don't pollute.
        let mv_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions WHERE manifestation_id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM api_cache WHERE lookup_key = $1")
                .bind(format!("isbn:{isbn}"))
                .fetch_one(&pool)
                .await
                .unwrap();

        let diff = dry_run::preview(&pool, &cfg, m_id).await.unwrap();
        assert!(
            !diff.would_apply.is_empty() || !diff.would_stage.is_empty(),
            "dry_run should surface at least one proposed change"
        );

        let mv_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions WHERE manifestation_id = $1",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM api_cache WHERE lookup_key = $1")
                .bind(format!("isbn:{isbn}"))
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            mv_after,
            mv_before,
            "dry_run must NOT write to metadata_versions (delta {})",
            mv_after - mv_before
        );
        assert!(
            cache_after > cache_before,
            "dry_run must populate api_cache (before={cache_before}, after={cache_after})"
        );
    }

    // ── Phase-direct tests (UNK-96 follow-up) ────────────────────────────
    //
    // The phase decomposition makes it cheap to exercise tail-of-distribution
    // scenarios that would otherwise need three configured wiremock servers
    // and a full `run_once` integration call.

    /// Every source returned an error → no journal rows, all failures
    /// summarised with correct retry_after / terminal flags. The
    /// `SourceError::Other` case also verifies that
    /// `summarise_failure`'s `{err:#}` formatting preserves the full
    /// anyhow `.chain()` of context.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_journal_batch_collects_all_source_failures(pool: PgPool) {
        use reqwest::StatusCode;

        let pool = ingestion_pool_for(&pool).await;
        // No DB row needed — apply_journal_batch only touches the DB on the
        // Ok arm; the manifestation_id is bound only inside upsert_journal_row.
        // If a future change adds a failure-side write (e.g. a
        // `source_failures` table), this test will need a real fixture row
        // to avoid an FK violation.
        let m_id = Uuid::new_v4();

        let chained = anyhow::anyhow!("leaf parse error")
            .context("decoding response body")
            .context("during google_books fetch");

        let results = vec![
            SourceRun {
                source_id: "openlibrary".into(),
                outcome: Err(SourceError::Timeout),
            },
            SourceRun {
                source_id: "googlebooks".into(),
                outcome: Err(SourceError::Http(StatusCode::NOT_FOUND)),
            },
            SourceRun {
                source_id: "hardcover".into(),
                outcome: Err(SourceError::RateLimited {
                    retry_after: Some(Duration::from_secs(60)),
                }),
            },
            SourceRun {
                source_id: "chained".into(),
                outcome: Err(SourceError::Other(chained)),
            },
        ];

        let mut tx = pool.begin().await.unwrap();
        let (per_field, failures) = apply_journal_batch(&mut tx, m_id, &results).await.unwrap();
        tx.commit().await.unwrap();

        assert!(
            per_field.is_empty(),
            "no journal rows should be written when every source errored"
        );
        assert_eq!(failures.len(), 4);

        let ol = failures
            .iter()
            .find(|f| f.source_id == "openlibrary")
            .unwrap();
        assert!(ol.retry_after.is_none(), "Timeout has no retry_after");
        assert!(!ol.terminal, "Timeout is retryable");

        let gb = failures
            .iter()
            .find(|f| f.source_id == "googlebooks")
            .unwrap();
        assert!(gb.retry_after.is_none(), "Http(404) has no retry_after");
        assert!(gb.terminal, "non-429 4xx must be terminal");

        let hc = failures
            .iter()
            .find(|f| f.source_id == "hardcover")
            .unwrap();
        assert_eq!(
            hc.retry_after,
            Some(Duration::from_secs(60)),
            "RateLimited retry_after must round-trip"
        );
        assert!(!hc.terminal, "RateLimited is not terminal");

        // anyhow chain preservation — `err.to_string()` would have only
        // surfaced the leaf; `{err:#}` walks `.chain()` and joins the
        // outer context. Each layer must appear in the stored error.
        let chained = failures.iter().find(|f| f.source_id == "chained").unwrap();
        assert!(
            chained.error.contains("leaf parse error"),
            "leaf must survive: {}",
            chained.error
        );
        assert!(
            chained.error.contains("decoding response body"),
            "middle context must survive: {}",
            chained.error
        );
        assert!(
            chained.error.contains("during google_books fetch"),
            "outer context must survive: {}",
            chained.error
        );
    }

    /// Two sources agree on a hash → first Apply fires, the inner loop
    /// `break`s, no second Apply, exactly one writeback enqueue.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_breaks_after_first_apply_on_agreement(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let isbn = "9780553283686";
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;

        let agreed = SourceResult {
            field_name: "publisher".into(),
            raw_value: serde_json::json!(format!("Agreed Publisher {marker}")),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_ol = upsert_journal_row(&mut tx, m_id, "openlibrary", &agreed)
            .await
            .unwrap();
        let id_gb = upsert_journal_row(&mut tx, m_id, "googlebooks", &agreed)
            .await
            .unwrap();
        let hash = value_hash::value_hash(&agreed.field_name, &agreed.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "publisher".into(),
            vec![
                (
                    "openlibrary".into(),
                    PolicyInputRow {
                        id: id_ol,
                        value_hash: hash.clone(),
                    },
                ),
                (
                    "googlebooks".into(),
                    PolicyInputRow {
                        id: id_gb,
                        value_hash: hash.clone(),
                    },
                ),
            ],
        );

        let snapshot = Snapshot {
            manifestation_id: m_id,
            work_id,
            lookup_key: None,
            canonical: CanonicalState::default(),
        };

        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 1,
            "agreement should Apply once; break must prevent the second source from re-applying"
        );
        assert_eq!(outcome.staged, 0);
        assert_eq!(outcome.skipped_locked, 0);

        let writeback_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            writeback_rows, 1,
            "exactly one writeback row expected; got {writeback_rows}"
        );
    }

    /// A pending row from a prior run with a different value_hash must
    /// downgrade AutoFill to Propose — even when canonical is empty and the
    /// new run has only one row.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_merges_prior_pending_into_decision(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let isbn = "9780747532699";
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;

        let prior = SourceResult {
            field_name: "publisher".into(),
            raw_value: serde_json::json!(format!("Prior Publisher {marker}")),
            match_type: "isbn".into(),
        };
        let new = SourceResult {
            field_name: "publisher".into(),
            raw_value: serde_json::json!(format!("New Publisher {marker}")),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        // Simulate the prior run's pending row.
        upsert_journal_row(&mut tx, m_id, "openlibrary", &prior)
            .await
            .unwrap();
        // The current run's row.
        let id_new = upsert_journal_row(&mut tx, m_id, "googlebooks", &new)
            .await
            .unwrap();
        let new_hash = value_hash::value_hash(&new.field_name, &new.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "publisher".into(),
            vec![(
                "googlebooks".into(),
                PolicyInputRow {
                    id: id_new,
                    value_hash: new_hash,
                },
            )],
        );

        let snapshot = Snapshot {
            manifestation_id: m_id,
            work_id,
            lookup_key: None,
            canonical: CanonicalState::default(),
        };

        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 0,
            "disagreement with stored pending must prevent AutoFill"
        );
        assert_eq!(
            outcome.staged, 1,
            "the new run's row should land in Stage when prior pending disagrees"
        );

        let canon_publisher: Option<String> =
            sqlx::query_scalar("SELECT publisher FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            canon_publisher.is_none(),
            "canonical publisher must remain empty"
        );

        let writeback_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            writeback_rows, 0,
            "Stage decision must not enqueue a writeback"
        );
    }

    /// Positive control for `apply_canonical_batch_merges_prior_pending_into_decision`:
    /// the same shape (single source, empty canonical) MUST Apply when no
    /// disagreeing prior pending row exists. If this test fails, the
    /// prior-pending Stage assertion above is passing vacuously.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_applies_when_no_prior_pending(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let isbn = "9780747538103";
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;

        let new = SourceResult {
            field_name: "publisher".into(),
            raw_value: serde_json::json!(format!("Solo Publisher {marker}")),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_new = upsert_journal_row(&mut tx, m_id, "googlebooks", &new)
            .await
            .unwrap();
        let new_hash = value_hash::value_hash(&new.field_name, &new.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "publisher".into(),
            vec![(
                "googlebooks".into(),
                PolicyInputRow {
                    id: id_new,
                    value_hash: new_hash,
                },
            )],
        );

        let snapshot = Snapshot {
            manifestation_id: m_id,
            work_id,
            lookup_key: None,
            canonical: CanonicalState::default(),
        };

        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 1,
            "single source + empty canonical + no prior pending MUST Apply"
        );
        assert_eq!(outcome.staged, 0);
    }

    /// `apply_field` returning `Ok(false)` for a non-string JSON value
    /// must skip without inflating counters or enqueuing writebacks; the
    /// inner loop must `continue` to the next source for the same field.
    ///
    /// Both rows are inserted directly with a forged shared `value_hash`
    /// to defeat `policy::decide`'s disagreement check —
    /// `upsert_journal_row` would compute distinct hashes from the
    /// distinct raw values, and `load_existing_pending` re-reads those
    /// real hashes. The forge is necessary to land both sources in the
    /// `Decision::Apply` branch so the continue-on-Ok(false) branch is
    /// actually exercised.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_skips_non_string_value_and_continues(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let isbn = "9780743273565";
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;

        let good_value = format!("Good Publisher {marker}");
        let shared_hash: Vec<u8> = vec![0u8; 32];

        // Bad row: array (non-string) → apply_field returns Ok(false).
        let id_bad: Uuid = sqlx::query_scalar(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id",
        )
        .bind(m_id)
        .bind("openlibrary")
        .bind("publisher")
        .bind(serde_json::json!(["Bad Publisher"]))
        .bind(&shared_hash)
        .bind("isbn")
        .bind(0.85_f32)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Good row: string → apply_field returns Ok(true).
        let id_good: Uuid = sqlx::query_scalar(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id",
        )
        .bind(m_id)
        .bind("googlebooks")
        .bind("publisher")
        .bind(serde_json::json!(good_value.clone()))
        .bind(&shared_hash)
        .bind("isbn")
        .bind(0.85_f32)
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "publisher".into(),
            vec![
                (
                    "openlibrary".into(),
                    PolicyInputRow {
                        id: id_bad,
                        value_hash: shared_hash.clone(),
                    },
                ),
                (
                    "googlebooks".into(),
                    PolicyInputRow {
                        id: id_good,
                        value_hash: shared_hash,
                    },
                ),
            ],
        );

        let snapshot = Snapshot {
            manifestation_id: m_id,
            work_id,
            lookup_key: None,
            canonical: CanonicalState::default(),
        };

        let mut tx = pool.begin().await.unwrap();
        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 1,
            "bad-value source must be skipped; good source must apply"
        );

        let canon: Option<String> =
            sqlx::query_scalar("SELECT publisher FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            canon.as_deref(),
            Some(good_value.as_str()),
            "canonical publisher must come from the good source"
        );

        let writeback_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            writeback_rows, 1,
            "exactly one writeback row for the successful apply"
        );
    }

    /// `apply_field` returning `Ok(false)` for a malformed `pub_date`
    /// (the `parse_iso_date` branch — distinct internal control flow
    /// from the non-string skip) must also leave canonical unchanged
    /// with no counter bump and no writeback.
    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_skips_malformed_pub_date(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let isbn = "9780812550702";
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;

        let bad = SourceResult {
            field_name: "pub_date".into(),
            // String, but parse_iso_date will reject it.
            raw_value: serde_json::json!("not-an-iso-date"),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_bad = upsert_journal_row(&mut tx, m_id, "openlibrary", &bad)
            .await
            .unwrap();
        let hash = value_hash::value_hash(&bad.field_name, &bad.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "pub_date".into(),
            vec![(
                "openlibrary".into(),
                PolicyInputRow {
                    id: id_bad,
                    value_hash: hash,
                },
            )],
        );

        let snapshot = Snapshot {
            manifestation_id: m_id,
            work_id,
            lookup_key: None,
            canonical: CanonicalState::default(),
        };

        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 0,
            "malformed pub_date must not count as applied"
        );
        assert_eq!(outcome.staged, 0);
        assert_eq!(outcome.skipped_locked, 0);

        let pub_date: Option<time::Date> =
            sqlx::query_scalar("SELECT pub_date FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(pub_date.is_none(), "canonical pub_date must remain unset");

        let writeback_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM writeback_jobs WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            writeback_rows, 0,
            "no writeback should be enqueued for a skipped apply"
        );
    }
}
