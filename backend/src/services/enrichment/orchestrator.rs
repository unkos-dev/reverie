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
use futures::future::join_all;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use tokio::time::timeout;
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
        match field {
            "title" => self.title.is_none(),
            "description" => self.description.is_none(),
            "language" => self.language.is_none(),
            "publisher" => self.publisher.is_none(),
            "pub_date" => self.pub_date.is_none(),
            "isbn_10" => self.isbn_10.is_none(),
            "isbn_13" => self.isbn_13.is_none(),
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
    let http = api_client();

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

    let mut per_field: std::collections::HashMap<String, Vec<(String, PolicyInputRow)>> =
        std::collections::HashMap::new();
    let mut failures = Vec::new();

    for r in &results {
        match &r.outcome {
            Ok(source_results) => {
                for sr in source_results {
                    let id =
                        upsert_journal_row(&mut tx, manifestation_id, &r.source_id, sr).await?;
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

    let mut applied = 0usize;
    let mut staged = 0usize;
    let mut skipped_locked = 0usize;
    let mut duplicate_suspected = false;

    for (field, rows) in &per_field {
        let on_work = is_work_field(field);
        let entity = if on_work {
            EntityType::Work
        } else {
            EntityType::Manifestation
        };
        let locked = field_lock::is_locked_tx(&mut tx, manifestation_id, entity, field).await?;

        let canonical_empty = snapshot.canonical.is_empty_for(field);

        // Existing pending rows from prior runs (other value_hashes).
        let existing_pending = load_existing_pending(&mut tx, manifestation_id, field).await?;

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
                    .fetch_one(&mut *tx)
                    .await?;
            let confidence_score = confidence::score(source_id, &match_type, quorum);
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
                    apply_field(&mut tx, &snapshot, field, version_id).await?;
                    applied += 1;
                    info!(
                        %manifestation_id, %field, %version_id, source_id,
                        "enrichment: metadata.applied"
                    );
                    if field == "isbn_10" || field == "isbn_13" {
                        let outcome =
                            work::rematch_on_isbn_change(&mut tx, manifestation_id).await?;
                        if matches!(outcome, work::RematchOutcome::Suspected { .. }) {
                            duplicate_suspected = true;
                            warn!(
                                %manifestation_id,
                                "enrichment: work.duplicate_suspected"
                            );
                        }
                    }
                    // Avoid re-applying on the same run when two sources agree.
                    break;
                }
                Decision::Stage => {
                    staged += 1;
                    tracing::debug!(
                        %manifestation_id, %field, source_id,
                        "enrichment: metadata.staged"
                    );
                }
                Decision::NoOp => {
                    skipped_locked += 1;
                }
            }
        }
    }

    tx.commit().await?;

    Ok(RunOutcome {
        manifestation_id,
        applied,
        staged,
        skipped_locked,
        source_failures: failures,
        duplicate_suspected,
    })
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
/// starve the others: timeouts mark that provider as Timeout but results
/// from siblings still land in the journal.
pub async fn fan_out(
    sources: &[Arc<dyn MetadataSource>],
    http: &reqwest::Client,
    key: &LookupKey,
    budget: Duration,
) -> Vec<SourceRun> {
    let futs: Vec<_> = sources
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

    match timeout(budget, join_all(futs)).await {
        Ok(v) => v,
        Err(_) => {
            warn!(?budget, "enrichment fan-out exceeded fetch budget");
            Vec::new()
        }
    }
}

async fn cache_all(pool: &PgPool, runs: &[SourceRun], key: &LookupKey, ttls: &CacheTtls) {
    let cache_key = key.cache_key();
    for run in runs {
        let (payload, kind, status) = match &run.outcome {
            Ok(results) if results.is_empty() => (serde_json::json!([]), ApiCacheKind::Miss, None),
            Ok(results) => (
                serde_json::to_value(results.iter().map(|r| &r.raw_value).collect::<Vec<_>>())
                    .unwrap_or(serde_json::Value::Null),
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

/// Apply a scalar field to its canonical column + `*_version_id` pointer.
async fn apply_field(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    field: &str,
    version_id: Uuid,
) -> sqlx::Result<()> {
    // Pull canonical value from the journal row — serialised as JSON so we
    // have a single source of truth.
    let value: serde_json::Value =
        sqlx::query_scalar("SELECT new_value FROM metadata_versions WHERE id = $1")
            .bind(version_id)
            .fetch_one(&mut **tx)
            .await?;

    match field {
        "title" => {
            let v = json_as_string(&value);
            sqlx::query(
                "UPDATE works SET title = $1, sort_title = lower($1), title_version_id = $2 \
                 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.work_id)
            .execute(&mut **tx)
            .await?;
        }
        "description" => {
            let v = json_as_string(&value);
            sqlx::query(
                "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.work_id)
            .execute(&mut **tx)
            .await?;
        }
        "language" => {
            let v = json_as_string(&value);
            sqlx::query("UPDATE works SET language = $1, language_version_id = $2 WHERE id = $3")
                .bind(&v)
                .bind(version_id)
                .bind(snapshot.work_id)
                .execute(&mut **tx)
                .await?;
        }
        "publisher" => {
            let v = json_as_string(&value);
            sqlx::query(
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
        }
        "pub_date" => {
            let v = json_as_string(&value);
            if let Ok(date) = parse_iso_date(&v) {
                sqlx::query(
                    "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
                )
                .bind(date)
                .bind(version_id)
                .bind(snapshot.manifestation_id)
                .execute(&mut **tx)
                .await?;
            } else {
                tracing::debug!(value = %v, "pub_date value not ISO; skipping canonical apply");
            }
        }
        "isbn_10" => {
            let v = json_as_string(&value);
            sqlx::query(
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
        }
        "isbn_13" => {
            let v = json_as_string(&value);
            sqlx::query(
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
            )
            .bind(&v)
            .bind(version_id)
            .bind(snapshot.manifestation_id)
            .execute(&mut **tx)
            .await?;
        }
        other => {
            tracing::debug!(field = %other, "no auto-apply handler; staying staged");
        }
    }
    Ok(())
}

fn json_as_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_iso_date(s: &str) -> Result<time::Date, time::error::Parse> {
    use time::format_description::well_known::Iso8601;
    if s.len() >= 10 {
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
        error: err.to_string(),
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
    let http = api_client();
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

#[allow(dead_code)]
async fn noop(_conn: &mut PgConnection) -> sqlx::Result<()> {
    Ok(())
}
