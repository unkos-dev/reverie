//! Per-manifestation enrichment flow.
//!
//! * Load canonical current state + enabled sources.
//! * Build a `LookupKey` (ISBN preferred, title+author fallback).
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
use crate::models::external_identifier::{
    IdentifierLevel, get_manifestation_identifier, get_work_identifier,
    upsert_manifestation_identifier, upsert_work_identifier,
};
use crate::models::external_rating;
use crate::models::work;
use crate::services::enrichment::cache::{self, ApiCacheKind, CacheTtls};
use crate::services::enrichment::confidence;
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::http::api_client;
use crate::services::enrichment::lookup_key;
use crate::services::enrichment::policy::{self, Decision, PolicyInputRow};
use crate::services::enrichment::sources::{
    LookupCtx, LookupKey, LookupOutcome, MetadataSource, SourceError, SourceResult,
    google_books::GoogleBooks, hardcover::Hardcover, is_fetchable_scheme,
    open_library::OpenLibrary,
};
use crate::services::enrichment::value_hash;
use crate::services::metadata::external_id;

/// Outcome of a single [`run_once`] call.  Returned to the queue layer so it
/// can drive retry/skipped state transitions.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Manifestation that was enriched.
    pub manifestation_id: Uuid,
    /// Number of fields promoted to canonical in this run.
    pub applied: usize,
    /// Number of fields left as pending (awaiting review) in this run.
    pub staged: usize,
    /// Number of fields skipped because a user lock was active.
    pub skipped_locked: usize,
    /// Per-source error details for sources that did not return results.
    pub source_failures: Vec<SourceFailure>,
    /// Set to `true` when an `ISBN` change triggered a duplicate-work suspicion.
    pub duplicate_suspected: bool,
}

/// A source-level error captured during [`run_once`].
#[derive(Debug, Clone)]
pub struct SourceFailure {
    /// Source that failed (e.g. `"openlibrary"`).
    pub source_id: String,
    /// Human-readable error description.
    pub error: String,
    /// Populated when the source reported `HTTP 429`.  The queue uses this to
    /// schedule the next retry attempt.
    pub retry_after: Option<Duration>,
    /// `true` if the error was non-retryable (4xx other than 429).
    pub terminal: bool,
}

/// Snapshot of canonical field state + lookup key, shared between
/// [`run_once`] and [`crate::services::enrichment::dry_run`].
#[derive(Debug)]
pub struct Snapshot {
    /// Manifestation whose fields are being enriched.
    pub manifestation_id: Uuid,
    /// Parent work of the manifestation.
    pub work_id: Uuid,
    /// Every eligible lookup key in total priority order: ISBN first, then
    /// registry ids for the API-capable schemes (provider `base_priority`
    /// descending, fixed provider precedence on ties, manifestation-level
    /// before work-level, value-lexicographic last), then title/author.
    /// Empty when nothing usable exists.
    pub lookup_keys: Vec<LookupKey>,
    /// Current canonical field values loaded before the fan-out.
    pub canonical: CanonicalState,
}

/// Current canonical values for the fields managed by the enrichment pipeline.
///
/// Fields are `None` when the column is `NULL` and `Some("")` when the column
/// holds an empty string.  [`is_empty_for`](CanonicalState::is_empty_for) treats
/// both as "absent" to prevent auto-fill from writing over a blank placeholder.
#[derive(Debug, Default, Clone)]
pub struct CanonicalState {
    /// Work-level title (`works.title`).
    pub title: Option<String>,
    /// Work-level description (`works.description`).
    pub description: Option<String>,
    /// Work-level `BCP 47` language tag (`works.language`).
    pub language: Option<String>,
    /// Manifestation-level publisher string.
    pub publisher: Option<String>,
    /// Manifestation-level publication date (`YYYY-MM-DD` string after normalisation).
    pub pub_date: Option<String>,
    /// Manifestation-level `ISBN-10`.
    pub isbn_10: Option<String>,
    /// Manifestation-level `ISBN-13`.
    pub isbn_13: Option<String>,
    /// Work-level subtitle (`works.subtitle`).
    pub subtitle: Option<String>,
    /// Manifestation-level page count (`manifestations.pages`).
    pub pages: Option<i32>,
    /// Active external-identifier slots for the manifestation and its work,
    /// keyed by canonical field name (`identifiers.<level>.<scheme>` =>
    /// `external_id`). Loaded so `is_empty_for` reports a populated registry
    /// slot as non-empty; without it every identifier observation would
    /// `AutoFill` over an existing value instead of staging for review.
    pub identifiers: std::collections::HashMap<String, String>,
}

impl CanonicalState {
    /// Returns `true` when the canonical slot for `field` is absent or blank.
    ///
    /// Both `None` and `Some("")` are treated as empty so that stub titles
    /// (inserted as `""` by `work::create_stub`) do not block auto-fill.
    pub fn is_empty_for(&self, field: &str) -> bool {
        fn blank(v: Option<&str>) -> bool {
            v.unwrap_or("").is_empty()
        }
        match field {
            "title" => blank(self.title.as_deref()),
            "description" => blank(self.description.as_deref()),
            "language" => blank(self.language.as_deref()),
            "publisher" => blank(self.publisher.as_deref()),
            "pub_date" => blank(self.pub_date.as_deref()),
            "isbn_10" => blank(self.isbn_10.as_deref()),
            "isbn_13" => blank(self.isbn_13.as_deref()),
            "subtitle" => blank(self.subtitle.as_deref()),
            "pages" => self.pages.is_none(),
            f if f.starts_with("identifiers.") => !self.identifiers.contains_key(f),
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
///
/// Returns early with an empty [`RunOutcome`] (zero applied/staged) when no
/// lookup key can be derived.  Individual source failures are captured in
/// [`RunOutcome::source_failures`] rather than causing an error return.
///
/// # Errors
///
/// Returns an error if the database is unreachable, the manifestation does
/// not exist, or the transaction commit fails.
pub async fn run_once(
    pool: &PgPool,
    config: &Config,
    manifestation_id: Uuid,
) -> anyhow::Result<RunOutcome> {
    let snapshot = load_snapshot(pool, manifestation_id).await?;
    if snapshot.lookup_keys.is_empty() {
        info!(
            %manifestation_id,
            "no lookup key (missing ISBN + native ids + title/author) — nothing to enrich"
        );
        return Ok(RunOutcome {
            manifestation_id,
            applied: 0,
            staged: 0,
            skipped_locked: 0,
            source_failures: Vec::new(),
            duplicate_suspected: false,
        });
    }

    let sources = build_sources(config);
    let ua = config.user_agent();
    let http = api_client(&ua);

    let ttls = CacheTtls {
        hit: time::Duration::days(i64::from(config.enrichment.cache_ttl_hit_days)),
        miss: time::Duration::days(i64::from(config.enrichment.cache_ttl_miss_days)),
        error: time::Duration::minutes(i64::from(config.enrichment.cache_ttl_error_mins)),
    };
    let results = fan_out_with_fallback(
        pool,
        &sources,
        &http,
        &snapshot.lookup_keys,
        Duration::from_secs(config.enrichment.fetch_budget_secs),
        &ttls,
    )
    .await;

    // Open the single mutating transaction: journal writes + canonical updates
    // + rematch hook all commit or roll back together.
    let mut tx = pool.begin().await?;

    let (per_field, failures) = apply_journal_batch(&mut tx, manifestation_id, &results).await?;
    let canonical = apply_canonical_batch(&mut tx, &snapshot, &per_field).await?;
    upsert_ratings(&mut tx, manifestation_id, &results).await?;

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

/// Route rating signals straight into the `manifestation_external_ratings`
/// cache, bypassing the journal and the policy engine entirely: ratings are
/// per-source refreshable values with no canonical to reconcile, never
/// journaled, locked, or written back. A reported rating refreshes the row;
/// a rating-capable record that omits its rating, or reports one the ratings
/// cache's own invariant rejects, removes the cached row so the projection
/// cannot serve an obsolete score indefinitely; a path that carries no
/// rating data leaves the cache untouched. There is no runtime range guard
/// here: `RatingObservation` cannot be constructed out of range, so an
/// unusable value already arrives as [`RatingSignal::Unusable`] rather than
/// as a [`RatingSignal::Reported`] value that this function would need to
/// re-check.
async fn upsert_ratings(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    results: &[SourceRun],
) -> anyhow::Result<()> {
    use crate::services::enrichment::sources::RatingSignal;
    for run in results {
        let Ok(outcome) = &run.outcome else { continue };
        match &outcome.rating {
            RatingSignal::Unknown => {}
            RatingSignal::Absent => {
                let removed =
                    external_rating::delete_rating(&mut **tx, manifestation_id, &run.source_id)
                        .await?;
                if removed {
                    info!(
                        %manifestation_id, source = %run.source_id,
                        "enrichment: rating removed by provider; cache row cleared"
                    );
                }
            }
            RatingSignal::Reported(observation) => {
                external_rating::upsert_rating(
                    &mut **tx,
                    manifestation_id,
                    &run.source_id,
                    observation,
                )
                .await?;
            }
            RatingSignal::Unusable(err) => {
                // A value the cache's invariant rejects is no evidence that
                // the cached score still holds, so drop it instead of
                // serving it until the provider next reports something
                // usable.
                let removed =
                    external_rating::delete_rating(&mut **tx, manifestation_id, &run.source_id)
                        .await?;
                warn!(
                    %manifestation_id, source = %run.source_id, rating = err.rating,
                    scale = err.rating_scale, count = err.review_count, cleared = removed,
                    "enrichment: unusable provider rating; cache row cleared"
                );
            }
        }
    }
    Ok(())
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
            Ok(outcome) => {
                for sr in &outcome.fields {
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

/// Compute the Apply-vs-Stage emptiness input for one field.
///
/// For identifier fields the decision must be made against the registry's
/// *current* state, not the pre-fan-out snapshot. Work-level slots are
/// shared across sibling manifestations, so the work row is locked FOR
/// UPDATE *before* the decision and the slot re-read under that lock: a
/// concurrent run on a sibling then serialises here and sees the winner's
/// value, flipping its own decision from Apply to Stage instead of
/// clobbering. A lock taken later (inside the write) could only serialise
/// the write, not change an already-made decision. Manifestation-level
/// slots race the manual PATCH path rather than sibling runs; their re-read
/// relies on the manifestation row lock [`apply_canonical_batch`] takes at
/// entry, which every manual write path also contends on. Non-identifier
/// fields keep the snapshot-based check.
async fn canonical_empty_under_lock(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    field: &str,
) -> anyhow::Result<bool> {
    match external_id::parse_canonical_field(field) {
        Ok((IdentifierLevel::Work, scheme)) => {
            sqlx::query!(
                "SELECT id FROM works WHERE id = $1 FOR UPDATE",
                snapshot.work_id,
            )
            .fetch_optional(&mut **tx)
            .await?;
            Ok(get_work_identifier(&mut **tx, snapshot.work_id, scheme)
                .await?
                .is_none())
        }
        Ok((IdentifierLevel::Manifestation, scheme)) => {
            Ok(
                get_manifestation_identifier(&mut **tx, snapshot.manifestation_id, scheme)
                    .await?
                    .is_none(),
            )
        }
        Err(_) => Ok(snapshot.canonical.is_empty_for(field)),
    }
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

    // Lock the owning manifestation row before any decision. Two jobs: the
    // manual write paths (PATCH/accept/revert) lock this row at handler
    // entry, so manifestation-level identifier emptiness is decided against
    // their committed edits instead of clobbering an operator value that
    // landed mid-run; and taking the manifestation lock before any work-row
    // lock matches the manual paths' acquisition order (manifestation, then
    // work), keeping the two sides deadlock-free regardless of per-field
    // iteration order below. A fresh journal INSERT earlier in this
    // transaction incidentally serialises with the manual paths through the
    // FK check's KEY SHARE on this row, but a repeat observation takes the
    // journal upsert's DO UPDATE arm, which locks no parent row — this
    // explicit lock is the only guarantee that holds on both paths.
    sqlx::query!(
        "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE",
        manifestation_id,
    )
    .fetch_optional(&mut **tx)
    .await?;

    for (field, rows) in per_field {
        let entity = if is_work_field(field) {
            EntityType::Work
        } else {
            EntityType::Manifestation
        };
        let locked = field_lock::is_locked_tx(tx, manifestation_id, entity, field).await?;

        let canonical_empty = canonical_empty_under_lock(tx, snapshot, field).await?;

        // Existing pending rows from prior runs (other value_hashes),
        // loaded after the work-row lock so identifier disagreement is
        // also judged against committed concurrent state.
        let existing_pending = load_existing_pending(tx, manifestation_id, field).await?;

        // quorum counts distinct rows in *this* run with the same hash.
        for (source_id, incoming) in rows {
            let quorum = u32::try_from(
                rows.iter()
                    .filter(|(_, r)| r.value_hash == incoming.value_hash)
                    .count(),
            )
            .unwrap_or(u32::MAX);
            // Pull the authoritative match_type back from the row we just
            // upserted — it may be 'isbn', 'title_author_fuzzy', or 'title'
            // depending on the source path.
            let match_type = sqlx::query_scalar!(
                "SELECT match_type FROM metadata_versions WHERE id = $1",
                incoming.id,
            )
            .fetch_one(&mut **tx)
            .await?;
            let confidence_score = confidence::score(source_id, &match_type, quorum);
            sqlx::query!(
                "UPDATE metadata_versions SET confidence_score = $1 WHERE id = $2",
                confidence_score,
                incoming.id,
            )
            .execute(&mut **tx)
            .await?;
            tracing::debug!(
                %field, source_id, quorum, %match_type, confidence_score,
                "confidence computed"
            );

            // Combine pending from this run with stored pending rows.
            let mut pending_set: Vec<PolicyInputRow> = existing_pending.clone();
            for (_, other) in rows {
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
                    // External identifiers are never written back to the
                    // source file, so their applies skip the enqueue.
                    if !field.starts_with("identifiers.") {
                        enqueue_writeback(tx, manifestation_id, field).await?;
                    }
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
///
/// # Errors
///
/// Returns an error if the manifestation does not exist or the query fails.
pub async fn load_snapshot(pool: &PgPool, manifestation_id: Uuid) -> anyhow::Result<Snapshot> {
    // `first_author` must be the display-form `a.name`, not the denormalized
    // `first_author_sort_name` column: the lookup key feeds external source
    // queries (Hardcover ilike, Google Books inauthor:) that match display
    // names, and a sort-form "Surname, Given" string never matches there.
    // The role filter keeps an editor- or translator-only work from
    // masquerading as its "first author" for lookup-key purposes.
    let row = sqlx::query!(
        "SELECT m.work_id, m.isbn_10, m.isbn_13, m.publisher, m.pub_date, m.pages, \
                w.title, w.description, w.language, w.subtitle, \
                (SELECT a.name FROM work_authors wa \
                 JOIN authors a ON a.id = wa.author_id \
                 WHERE wa.work_id = w.id AND wa.role = 'author' \
                 ORDER BY wa.position \
                 LIMIT 1) AS first_author \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1",
        manifestation_id,
    )
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| anyhow!("manifestation not found: {manifestation_id}"))?;

    // `w.title` is `TEXT NOT NULL` but `models::work::create_stub` writes an
    // empty string as a placeholder before metadata draft completes. Treat
    // empty as "absent" so canonical comparison + lookup-key derivation skip
    // the stub instead of matching against `""`.
    let title_opt = if row.title.is_empty() {
        None
    } else {
        Some(row.title)
    };

    // Active registry slots for the manifestation and its work, both for
    // `is_empty_for` (a populated slot must not AutoFill) and for deriving
    // native-id lookup keys.
    let mut identifiers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let manif_ids = sqlx::query!(
        "SELECT scheme, external_id FROM manifestation_external_identifiers \
         WHERE manifestation_id = $1",
        manifestation_id,
    )
    .fetch_all(pool)
    .await?;
    for r in manif_ids {
        identifiers.insert(
            IdentifierLevel::Manifestation.canonical_field(&r.scheme),
            r.external_id,
        );
    }
    let work_ids = sqlx::query!(
        "SELECT scheme, external_id FROM work_external_identifiers WHERE work_id = $1",
        row.work_id,
    )
    .fetch_all(pool)
    .await?;
    for r in work_ids {
        identifiers.insert(
            IdentifierLevel::Work.canonical_field(&r.scheme),
            r.external_id,
        );
    }

    // Provider base priorities for the API-capable schemes; the scheme id
    // and the metadata_sources id coincide for exactly these three.
    let priorities: std::collections::HashMap<String, i32> = sqlx::query!(
        "SELECT id, base_priority FROM metadata_sources \
         WHERE id IN ('openlibrary', 'googlebooks', 'hardcover')",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.id, r.base_priority))
    .collect();

    let lookup_keys = derive_lookup_keys(
        row.isbn_13.as_deref(),
        row.isbn_10.as_deref(),
        title_opt.as_deref(),
        row.first_author.as_deref(),
        &identifiers,
        &priorities,
    );

    let canonical = CanonicalState {
        title: title_opt,
        description: row.description,
        language: row.language,
        publisher: row.publisher,
        pub_date: row.pub_date.map(|d| d.to_string()),
        isbn_10: row.isbn_10,
        isbn_13: row.isbn_13,
        subtitle: row.subtitle,
        pages: row.pages,
        identifiers,
    };

    Ok(Snapshot {
        manifestation_id,
        work_id: row.work_id,
        lookup_keys,
        canonical,
    })
}

/// Fixed provider precedence used to break `base_priority` ties. Lower index
/// wins. `openlibrary` and `googlebooks` both seed at the same priority, so
/// this is the operative tie-break between them.
fn scheme_precedence(scheme: &str) -> usize {
    ["openlibrary", "googlebooks", "hardcover"]
        .iter()
        .position(|s| *s == scheme)
        .unwrap_or(usize::MAX)
}

/// Derive the total-ordered eligible lookup-key list.
///
/// Order: (1) the ISBN key; (2) registry ids for the API-capable schemes,
/// sorted by provider `base_priority` descending, then the fixed precedence
/// `[openlibrary, googlebooks, hardcover]`, then manifestation-level before
/// work-level (edition ids are more specific), then value-lexicographic;
/// (3) title/author last. Non-fetchable schemes (`goodreads`, `asin`, ...)
/// never become keys.
fn derive_lookup_keys(
    isbn_13: Option<&str>,
    isbn_10: Option<&str>,
    title: Option<&str>,
    author: Option<&str>,
    identifiers: &std::collections::HashMap<String, String>,
    priorities: &std::collections::HashMap<String, i32>,
) -> Vec<LookupKey> {
    let mut keys = Vec::new();

    if let Some(k) = isbn_13
        .and_then(lookup_key::isbn_key)
        .or_else(|| isbn_10.and_then(lookup_key::isbn_key))
    {
        keys.push(LookupKey::Isbn(k));
    }

    // (level_rank, scheme, value): manifestation-level ranks before work.
    let mut ids: Vec<(u8, &str, &str)> = identifiers
        .iter()
        .filter_map(|(field, value)| {
            let (level, scheme) = external_id::parse_canonical_field(field).ok()?;
            if !is_fetchable_scheme(scheme) {
                return None;
            }
            let level_rank = match level {
                IdentifierLevel::Manifestation => 0_u8,
                IdentifierLevel::Work => 1_u8,
            };
            Some((level_rank, scheme, value.as_str()))
        })
        .collect();
    ids.sort_by(|a, b| {
        let pa = priorities.get(a.1).copied().unwrap_or(0);
        let pb = priorities.get(b.1).copied().unwrap_or(0);
        pb.cmp(&pa)
            .then_with(|| scheme_precedence(a.1).cmp(&scheme_precedence(b.1)))
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.2.cmp(b.2))
    });
    keys.extend(
        ids.into_iter()
            .map(|(_, scheme, value)| LookupKey::ExternalId {
                scheme: scheme.to_string(),
                value: value.to_string(),
            }),
    );

    if let (Some(t), Some(a)) = (title, author)
        && !t.is_empty()
        && !a.is_empty()
    {
        keys.push(LookupKey::TitleAuthor {
            title: t.to_string(),
            author: a.to_string(),
        });
    }
    keys
}

/// One fan-out entry: the result of a single source lookup during [`fan_out`].
pub struct SourceRun {
    /// Source identifier (e.g. `"openlibrary"`, `"googlebooks"`).
    pub source_id: String,
    /// Field + rating results on success, or the first error on failure.
    pub outcome: Result<LookupOutcome, SourceError>,
}

/// Try each eligible key in priority order until one attempt produces data,
/// under a single wall-clock budget shared across attempts.
///
/// An [`LookupKey::ExternalId`] attempt dispatches only to the adapter whose
/// id matches the scheme; every other key fans out to all enabled sources.
/// A clean miss, an error, or a disabled/absent adapter falls through to the
/// next key. Every attempt caches its runs under that attempt's own
/// type-prefixed key, so a fallback result can never poison another key's
/// cache entry. Runs from every attempt are returned; missed attempts
/// contribute empty results and their failures.
pub async fn fan_out_with_fallback(
    pool: &PgPool,
    sources: &[Arc<dyn MetadataSource>],
    http: &reqwest::Client,
    keys: &[LookupKey],
    budget: Duration,
    ttls: &CacheTtls,
) -> Vec<SourceRun> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut all: Vec<SourceRun> = Vec::new();
    for key in keys {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let attempt: Vec<Arc<dyn MetadataSource>> = match key {
            LookupKey::ExternalId { scheme, .. } => sources
                .iter()
                .filter(|s| s.id() == scheme)
                .cloned()
                .collect(),
            LookupKey::Isbn(_) | LookupKey::TitleAuthor { .. } => sources.to_vec(),
        };
        if !attempt.iter().any(|s| s.enabled()) {
            continue;
        }
        let runs = fan_out(&attempt, http, key, remaining).await;
        cache_all(pool, &runs, key, ttls).await;
        let hit = runs
            .iter()
            .any(|r| matches!(&r.outcome, Ok(o) if !o.is_empty()));
        all.extend(runs);
        if hit {
            break;
        }
    }
    all
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
            () = &mut deadline => {
                // Budget expired: synthesise a Timeout outcome for every
                // source that hasn't reported yet so the failure surfaces
                // to `finish`.  In-flight futures are dropped (cancelled).
                for id in &enabled_ids {
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
            Ok(outcome) if outcome.is_empty() => (serde_json::json!([]), ApiCacheKind::Miss, None),
            Ok(outcome) => (
                serde_json::to_value(
                    outcome
                        .fields
                        .iter()
                        .map(|r| &r.raw_value)
                        .collect::<Vec<_>>(),
                )
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
    let id = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
             (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET new_value = EXCLUDED.new_value, \
                       last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1 \
         RETURNING id",
        manifestation_id,
        source_id,
        sr.field_name,
        sr.raw_value,
        hash,
        sr.match_type,
        score,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

async fn load_existing_pending(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: &str,
) -> sqlx::Result<Vec<PolicyInputRow>> {
    let rows = sqlx::query!(
        "SELECT id, value_hash FROM metadata_versions \
         WHERE manifestation_id = $1 AND field_name = $2 AND status = 'pending'",
        manifestation_id,
        field,
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PolicyInputRow {
            id: r.id,
            value_hash: r.value_hash,
        })
        .collect())
}

fn is_work_field(field: &str) -> bool {
    matches!(field, "title" | "description" | "language" | "subtitle")
        || field.starts_with("contributors.")
        || field.starts_with("identifiers.work.")
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
    sqlx::query!(
        "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2)",
        manifestation_id,
        reason,
    )
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
#[expect(
    clippy::too_many_lines,
    reason = "apply_field dispatches over 11 canonical axes each needing a typed UPDATE; the per-axis cases are mechanical and extracting them further would obscure the field→column mapping"
)]
async fn apply_field(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    field: &str,
    version_id: Uuid,
) -> sqlx::Result<bool> {
    // Pull canonical value from the journal row — serialised as JSON so we
    // have a single source of truth.
    let value = sqlx::query_scalar!(
        "SELECT new_value FROM metadata_versions WHERE id = $1",
        version_id,
    )
    .fetch_one(&mut **tx)
    .await?;

    match field {
        "title" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE works SET title = $1, sort_title = lower($1), title_version_id = $2 \
                 WHERE id = $3",
                v,
                version_id,
                snapshot.work_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "description" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.work_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "language" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE works SET language = $1, language_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.work_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "subtitle" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE works SET subtitle = $1, subtitle_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.work_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "pages" => {
            let Some(v) = value.as_i64().and_then(|n| i32::try_from(n).ok()) else {
                tracing::warn!(field = %field, raw = %value, "non-positive-integer canonical value; skipping apply");
                return Ok(false);
            };
            if v <= 0 {
                tracing::warn!(field = %field, raw = %value, "non-positive page count; skipping apply");
                return Ok(false);
            }
            sqlx::query!(
                "UPDATE manifestations SET pages = $1, pages_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.manifestation_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "publisher" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.manifestation_id,
            )
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
            sqlx::query!(
                "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
                date,
                version_id,
                snapshot.manifestation_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "isbn_10" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.manifestation_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        "isbn_13" => {
            let Some(v) = json_as_string(&value) else {
                tracing::warn!(field = %field, raw = %value, "non-string canonical value; skipping apply");
                return Ok(false);
            };
            sqlx::query!(
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
                v,
                version_id,
                snapshot.manifestation_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(true)
        }
        f if f.starts_with("identifiers.") => {
            let Ok((level, scheme)) = external_id::parse_canonical_field(f) else {
                tracing::warn!(field = %f, "malformed identifier field name; skipping apply");
                return Ok(false);
            };
            let Some(raw) = value.as_str() else {
                tracing::warn!(field = %f, raw = %value, "non-string identifier value; skipping apply");
                return Ok(false);
            };
            // Provider-emitted ids are untrusted; the typed parser is the
            // gate before the registry (the DB CHECK is only a backstop).
            let Ok(canonical) = external_id::parse_external_id(level, scheme, raw) else {
                tracing::warn!(field = %f, raw = %raw, "identifier fails scheme format; skipping apply");
                return Ok(false);
            };
            match level {
                IdentifierLevel::Work => {
                    upsert_work_identifier(
                        &mut **tx,
                        snapshot.work_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await?;
                }
                IdentifierLevel::Manifestation => {
                    upsert_manifestation_identifier(
                        &mut **tx,
                        snapshot.manifestation_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await?;
                }
            }
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

/// Helper used by `dry_run::preview` — same fan-out + cache but no journal
/// writes and no canonical updates.
///
/// # Errors
///
/// Returns an error if the manifestation does not exist or a database query fails.
pub async fn fan_out_for_dry_run(
    pool: &PgPool,
    config: &Config,
    manifestation_id: Uuid,
) -> anyhow::Result<(Snapshot, Vec<SourceRun>)> {
    let snapshot = load_snapshot(pool, manifestation_id).await?;
    if snapshot.lookup_keys.is_empty() {
        return Ok((snapshot, Vec::new()));
    }
    let sources = build_sources(config);
    let ua = config.user_agent();
    let http = api_client(&ua);
    let ttls = CacheTtls {
        hit: time::Duration::days(i64::from(config.enrichment.cache_ttl_hit_days)),
        miss: time::Duration::days(i64::from(config.enrichment.cache_ttl_miss_days)),
        error: time::Duration::minutes(i64::from(config.enrichment.cache_ttl_error_mins)),
    };
    let results = fan_out_with_fallback(
        pool,
        &sources,
        &http,
        &snapshot.lookup_keys,
        Duration::from_secs(config.enrichment.fetch_budget_secs),
        &ttls,
    )
    .await;
    Ok((snapshot, results))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "bare reqwest::Client::new() against wiremock on loopback is ADR-exempt (adr/2026-05-18-outbound-http-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
    )]
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
        ) -> Result<LookupOutcome, SourceError> {
            Ok(LookupOutcome::default())
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
        ) -> Result<LookupOutcome, SourceError> {
            tokio::time::sleep(Duration::from_mins(1)).await;
            Ok(LookupOutcome::default())
        }
    }

    /// A hung provider must NOT discard
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

    /// Build a known-valid observation for seeding a cached rating; the
    /// constructor's own validation is covered in `models::external_rating`.
    fn seed_rating(
        rating: f32,
        rating_scale: f32,
        review_count: i32,
    ) -> external_rating::RatingObservation {
        external_rating::RatingObservation::new(rating, rating_scale, review_count)
            .expect("valid test rating")
    }

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
            local_auth_enabled: true,
            resource_server_issuer: String::new(),
            resource_server_audience: String::new(),
            resource_server_jwks_url: String::new(),
            resource_server_require_at_jwt: false,
            login_rate_per_min: 10,
            login_throttle_base_secs: 2,
            login_throttle_cap_secs: 900,
            password_min_length: 8,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            password_breach_check_url: "https://api.pwnedpasswords.com/range".into(),
            self_registration_enabled: false,
            recovery_pin_ttl_secs: 900,
            recovery_pin_dir: "./reverie-recovery".into(),
            trusted_client_ip_header: None,
            migration_database_url: None,
            auto_migrate: false,
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
            hardcover_api_token: hc_token.map(std::convert::Into::into),
            operator_contact: None,
            ingestion_dsn_defaulted: false,
        }
    }

    /// Insert (work + manifestation) with the given ISBN-13 and return both IDs.
    /// Canonical fields start empty so `AutoFill` is exercised.
    async fn insert_enrich_fixture(pool: &PgPool, isbn_13: &str, marker: &str) -> (Uuid, Uuid) {
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let path = format!("/tmp/orch-{marker}.epub");
        let hash = format!("orch-hash-{marker}");
        let manifestation_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, isbn_13, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, $2, 'epub'::manifestation_format, $3, $4, $4, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            isbn_13,
            path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, manifestation_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn load_snapshot_lookup_key_uses_author_display_name(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let title = format!("Snapshot Vocab {marker}");
        let sort_form = format!("{marker}, Alpha Writer");
        let display_form = format!("Alpha Writer {marker}");
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title, first_author_sort_name) \
             VALUES ($1, $1, $2) RETURNING id",
            title,
            sort_form,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let path = format!("/tmp/snap-{marker}.epub");
        let hash = format!("snap-hash-{marker}");
        let m_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            path,
            hash,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let editor_name = format!("Edit Person {marker}");
        let editor_sort = format!("{marker}, Edit Person");
        let editor_id = sqlx::query_scalar!(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $2) RETURNING id",
            editor_name,
            editor_sort,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let author_id = sqlx::query_scalar!(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $2) RETURNING id",
            display_form,
            sort_form,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // Editor sits at position 0: the role filter must skip it and pick
        // the position-1 author row.
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, 'editor', 0), ($1, $3, 'author', 1)",
            work_id,
            editor_id,
            author_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let snapshot = load_snapshot(&pool, m_id).await.unwrap();
        match snapshot.lookup_keys.last().cloned() {
            Some(LookupKey::TitleAuthor { title: t, author }) => {
                assert_eq!(t, title);
                assert_eq!(
                    author, display_form,
                    "lookup key must carry the display-form name external \
                     sources match on, not first_author_sort_name"
                );
            }
            other => panic!("expected TitleAuthor lookup key, got {other:?}"),
        }
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
        let writeback_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            writeback_rows, 1,
            "expected exactly one writeback row, got {writeback_rows}"
        );

        let canon = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            canon, canon_title,
            "canonical title should match agreement value"
        );

        // Three sources agreed on `title`; quorum=3 boost (1.20×) must be
        // persisted on the journal rows.  The maximum quorum-1 score for any
        // ISBN-matched source is `hardcover` at 0.85; with the boost,
        // `openlibrary` reaches 0.96 — anything ≥ 0.90 proves the boost
        // landed in the row, not just the log.
        let max_score = sqlx::query_scalar!(
            "SELECT MAX(confidence_score) AS \"max_score!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
            m_id,
        )
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
        let title_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_rows >= 3,
            "expected ≥3 title journal rows across sources, got {title_rows}"
        );

        let canon_title = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            canon_title.is_empty(),
            "canonical title should remain empty after disagreement, got '{canon_title}'"
        );

        let title_version_id = sqlx::query_scalar!(
            "SELECT w.title_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_version_id.is_none(),
            "no Apply should have run, title_version_id should be NULL"
        );
    }

    /// One source returns `publisher` (`AutoFill` by default) on an empty
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

        let row = sqlx::query!(
            "SELECT publisher, publisher_version_id FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.publisher.as_deref(),
            Some(publisher_name.as_str()),
            "AutoFill on empty canonical should apply publisher"
        );
        assert!(
            row.publisher_version_id.is_some(),
            "publisher_version_id must be wired"
        );

        // Apply path must emit a writeback_jobs row in the same tx.
        let job_count = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
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
    /// `title_version_id` are NOT updated.
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
        sqlx::query!(
            "INSERT INTO field_locks (manifestation_id, entity_type, field_name) \
             VALUES ($1, 'work', 'title')",
            m_id,
        )
        .execute(&app_pool)
        .await
        .unwrap();

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        // Journal row for the proposed title WAS written.
        let title_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_rows >= 1,
            "journal row must be written even when locked, got {title_rows}"
        );

        // Canonical title_version_id stays NULL.
        let title_ptr = sqlx::query_scalar!(
            "SELECT w.title_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title_ptr.is_none(),
            "locked field must NOT set canonical pointer"
        );
        let canon_title = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(canon_title.is_empty(), "canonical title must stay empty");
    }

    /// Empty canonical `works.subtitle` + a source-provided subtitle → applied.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_subtitle_autofill_applies_when_canonical_empty(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780553213119";
        let marker = Uuid::new_v4().simple().to_string();
        let subtitle = format!("A Subtitle {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({})).await;
        mock_googlebooks_isbn(
            &gb,
            json!({"items": [{"volumeInfo": {"subtitle": subtitle}}]}),
        )
        .await;
        mock_hardcover(&hc, json!({"data": {"books": []}})).await;

        let (_work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let (canon_subtitle, subtitle_ptr) = sqlx::query!(
            "SELECT w.subtitle, w.subtitle_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.subtitle, r.subtitle_version_id))
        .unwrap();
        assert_eq!(canon_subtitle.as_deref(), Some(subtitle.as_str()));
        assert!(subtitle_ptr.is_some());
    }

    /// A non-empty canonical `works.subtitle` must Stage (not overwrite) a
    /// differing source-provided subtitle — `AutoFill` only fills empty slots.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_subtitle_autofill_stages_when_canonical_already_set(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let isbn = "9780345391803";
        let marker = Uuid::new_v4().simple().to_string();
        let existing_subtitle = format!("Existing Subtitle {marker}");
        let proposed_subtitle = format!("Proposed Subtitle {marker}");

        mock_openlibrary_isbn(&ol, isbn, json!({})).await;
        mock_googlebooks_isbn(
            &gb,
            json!({"items": [{"volumeInfo": {"subtitle": proposed_subtitle}}]}),
        )
        .await;
        mock_hardcover(&hc, json!({"data": {"books": []}})).await;

        let (work_id, m_id) = insert_enrich_fixture(&pool, isbn, &marker).await;
        sqlx::query!(
            "UPDATE works SET subtitle = $1 WHERE id = $2",
            existing_subtitle,
            work_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let (canon_subtitle, subtitle_ptr) = sqlx::query!(
            "SELECT w.subtitle, w.subtitle_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.subtitle, r.subtitle_version_id))
        .unwrap();
        assert_eq!(
            canon_subtitle.as_deref(),
            Some(existing_subtitle.as_str()),
            "non-empty canonical subtitle must not be clobbered by AutoFill"
        );
        assert!(
            subtitle_ptr.is_none(),
            "staged (not applied) proposals must not set the pointer"
        );
    }

    /// A `contributors.editor` lock (entity `work`) isolates that role: the
    /// locked role is skipped while `contributors.author` in the same batch
    /// still stages. Enrichment never applies contributors (no apply arm), so
    /// this exercises `apply_canonical_batch` directly against synthetic
    /// per-field rows — no live source ever emits an editor observation.
    #[sqlx::test(migrations = "./migrations")]
    async fn orchestrator_per_role_contributor_lock_isolates_role(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = format!("978{}", &marker[..10]);
        let (_work_id, m_id) = insert_enrich_fixture(&pool, &isbn, &marker).await;

        sqlx::query!(
            "INSERT INTO field_locks (manifestation_id, entity_type, field_name) \
             VALUES ($1, 'work', 'contributors.editor')",
            m_id,
        )
        .execute(&app_pool)
        .await
        .unwrap();

        let author_value = json!(["Role Isolation Author"]);
        let editor_value = json!(["Role Isolation Editor"]);
        let author_row_id =
            insert_pending_journal_row(&pool, m_id, "contributors.author", &author_value).await;
        let editor_row_id =
            insert_pending_journal_row(&pool, m_id, "contributors.editor", &editor_value).await;

        let snapshot = load_snapshot(&pool, m_id).await.unwrap();
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "contributors.author".into(),
            vec![(
                "opf".into(),
                PolicyInputRow {
                    id: author_row_id,
                    value_hash: value_hash::value_hash("contributors.author", &author_value),
                },
            )],
        );
        per_field.insert(
            "contributors.editor".into(),
            vec![(
                "opf".into(),
                PolicyInputRow {
                    id: editor_row_id,
                    value_hash: value_hash::value_hash("contributors.editor", &editor_value),
                },
            )],
        );

        let mut tx = pool.begin().await.unwrap();
        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.skipped_locked, 1,
            "locked contributors.editor must be skipped"
        );
        assert_eq!(
            outcome.staged, 1,
            "unlocked contributors.author must still stage"
        );
        assert_eq!(outcome.applied, 0, "contributors.* never auto-applies");
    }

    /// Insert a `pending` journal row directly (bypassing the fan-out) with a
    /// real `value_hash`, for use as an `apply_canonical_batch` input.
    async fn insert_pending_journal_row(
        pool: &PgPool,
        manifestation_id: Uuid,
        field_name: &str,
        raw_value: &serde_json::Value,
    ) -> Uuid {
        let hash = value_hash::value_hash(field_name, raw_value);
        sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                 (manifestation_id, source, field_name, new_value, value_hash, \
                  match_type, confidence_score) \
             VALUES ($1, 'opf', $2, $3, $4, 'title', 0.5) \
             RETURNING id",
            manifestation_id,
            field_name,
            raw_value,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap()
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
        let mv_before = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let isbn_lookup = format!("isbn:{isbn}");
        let cache_before = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM api_cache WHERE lookup_key = $1",
            isbn_lookup,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let diff = dry_run::preview(&pool, &cfg, m_id).await.unwrap();
        assert!(
            !diff.would_apply.is_empty() || !diff.would_stage.is_empty(),
            "dry_run should surface at least one proposed change"
        );

        let mv_after = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cache_after = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM api_cache WHERE lookup_key = $1",
            isbn_lookup,
        )
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

    // ── Phase-direct tests ────────────────────────────
    //
    // The phase decomposition makes it cheap to exercise tail-of-distribution
    // scenarios that would otherwise need three configured wiremock servers
    // and a full `run_once` integration call.

    /// Every source returned an error → no journal rows, all failures
    /// summarised with correct `retry_after` / terminal flags. The
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
                    retry_after: Some(Duration::from_mins(1)),
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
            Some(Duration::from_mins(1)),
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
            lookup_keys: Vec::new(),
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

        let writeback_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            writeback_rows, 1,
            "exactly one writeback row expected; got {writeback_rows}"
        );
    }

    /// A pending row from a prior run with a different `value_hash` must
    /// downgrade `AutoFill` to Propose — even when canonical is empty and the
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
            lookup_keys: Vec::new(),
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

        let canon_publisher =
            sqlx::query_scalar!("SELECT publisher FROM manifestations WHERE id = $1", m_id,)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            canon_publisher.is_none(),
            "canonical publisher must remain empty"
        );

        let writeback_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
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
            lookup_keys: Vec::new(),
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

    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_applies_pages(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, "9781857231380", &marker).await;

        let new = SourceResult {
            field_name: "pages".into(),
            raw_value: serde_json::json!(353),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_new = upsert_journal_row(&mut tx, m_id, "googlebooks", &new)
            .await
            .unwrap();
        let new_hash = value_hash::value_hash(&new.field_name, &new.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "pages".into(),
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
            lookup_keys: Vec::new(),
            canonical: CanonicalState::default(),
        };
        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 1,
            "positive pages on empty canonical must Apply"
        );
        let row = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.pages, Some(353));
        assert_eq!(row.pages_version_id, Some(id_new));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn apply_canonical_batch_skips_non_positive_pages(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_enrich_fixture(&pool, "9781857231397", &marker).await;

        let new = SourceResult {
            field_name: "pages".into(),
            raw_value: serde_json::json!(0),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_new = upsert_journal_row(&mut tx, m_id, "googlebooks", &new)
            .await
            .unwrap();
        let new_hash = value_hash::value_hash(&new.field_name, &new.raw_value);
        let mut per_field: PerFieldRows = std::collections::HashMap::new();
        per_field.insert(
            "pages".into(),
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
            lookup_keys: Vec::new(),
            canonical: CanonicalState::default(),
        };
        let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome.applied, 0,
            "non-positive pages must skip the canonical write"
        );
        let row = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.pages, None);
        assert_eq!(row.pages_version_id, None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_journal_row_refreshes_new_value_on_reorder(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_enrich_fixture(&pool, "9781857231403", &marker).await;

        let first = SourceResult {
            field_name: "contributors.author".into(),
            raw_value: serde_json::json!([format!("Ada {marker}"), format!("Grace {marker}")]),
            match_type: "isbn".into(),
        };
        let reordered = SourceResult {
            field_name: "contributors.author".into(),
            raw_value: serde_json::json!([format!("Grace {marker}"), format!("Ada {marker}")]),
            match_type: "isbn".into(),
        };

        let mut tx = pool.begin().await.unwrap();
        let id_first = upsert_journal_row(&mut tx, m_id, "openlibrary", &first)
            .await
            .unwrap();
        let id_second = upsert_journal_row(&mut tx, m_id, "openlibrary", &reordered)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            id_first, id_second,
            "order-insensitive hash must collide on the same journal row"
        );

        let row = sqlx::query!(
            "SELECT new_value, observation_count FROM metadata_versions WHERE id = $1",
            id_first,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.new_value, reordered.raw_value,
            "colliding upsert must refresh new_value to the latest representation"
        );
        assert_eq!(row.observation_count, 2);
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
        let bad_value = serde_json::json!(["Bad Publisher"]);
        let id_bad = sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id",
            m_id,
            "openlibrary",
            "publisher",
            bad_value,
            shared_hash,
            "isbn",
            0.85_f32,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        // Good row: string → apply_field returns Ok(true).
        let good_json = serde_json::json!(good_value.clone());
        let id_good = sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id",
            m_id,
            "googlebooks",
            "publisher",
            good_json,
            shared_hash,
            "isbn",
            0.85_f32,
        )
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
            lookup_keys: Vec::new(),
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

        let canon =
            sqlx::query_scalar!("SELECT publisher FROM manifestations WHERE id = $1", m_id,)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            canon.as_deref(),
            Some(good_value.as_str()),
            "canonical publisher must come from the good source"
        );

        let writeback_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
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
            lookup_keys: Vec::new(),
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

        let pub_date =
            sqlx::query_scalar!("SELECT pub_date FROM manifestations WHERE id = $1", m_id,)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(pub_date.is_none(), "canonical pub_date must remain unset");

        let writeback_rows = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            writeback_rows, 0,
            "no writeback should be enqueued for a skipped apply"
        );
    }

    // ── external identifiers + ratings (native-id enrichment) ────────────

    /// ISBN-less fixture: no isbn column and an empty stub title, so lookup
    /// keys can only come from the identifier registry.
    async fn insert_isbnless_fixture(pool: &PgPool, marker: &str) -> (Uuid, Uuid) {
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let path = format!("/tmp/orch-extid-{marker}.epub");
        let hash = format!("orch-extid-hash-{marker}");
        let manifestation_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, manifestation_id)
    }

    async fn mock_ol_edition(server: &MockServer, olid: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(format!("/books/{olid}.json")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    #[test]
    fn derive_lookup_keys_order_is_pinned() {
        let mut identifiers = std::collections::HashMap::new();
        identifiers.insert(
            "identifiers.manifestation.openlibrary".to_string(),
            "OL7353617M".to_string(),
        );
        identifiers.insert(
            "identifiers.work.openlibrary".to_string(),
            "OL45804W".to_string(),
        );
        identifiers.insert(
            "identifiers.manifestation.googlebooks".to_string(),
            "zyTZAAAAYAAJ".to_string(),
        );
        identifiers.insert("identifiers.work.hardcover".to_string(), "dune".to_string());
        // Never-fetched schemes must not become keys.
        identifiers.insert(
            "identifiers.manifestation.asin".to_string(),
            "B004GXAX8C".to_string(),
        );
        identifiers.insert("identifiers.work.goodreads".to_string(), "5907".to_string());

        let priorities: std::collections::HashMap<String, i32> = [
            ("openlibrary".to_string(), 100),
            ("googlebooks".to_string(), 100),
            ("hardcover".to_string(), 90),
        ]
        .into_iter()
        .collect();

        let keys = derive_lookup_keys(
            Some("9780441172719"),
            None,
            Some("Dune"),
            Some("Frank Herbert"),
            &identifiers,
            &priorities,
        );

        let rendered: Vec<String> = keys
            .iter()
            .map(|k| match k {
                LookupKey::Isbn(v) => format!("isbn|{v}"),
                LookupKey::ExternalId { scheme, value } => format!("ext|{scheme}|{value}"),
                LookupKey::TitleAuthor { .. } => "ta".to_string(),
            })
            .collect();
        // Pinned total order: ISBN; then priority desc with the fixed
        // [openlibrary, googlebooks, hardcover] tie-break, manifestation
        // before work within a scheme; title/author last. asin/goodreads
        // never appear.
        assert_eq!(
            rendered,
            vec![
                "isbn|isbn:9780441172719".to_string(),
                "ext|openlibrary|OL7353617M".to_string(),
                "ext|openlibrary|OL45804W".to_string(),
                "ext|googlebooks|zyTZAAAAYAAJ".to_string(),
                "ext|hardcover|dune".to_string(),
                "ta".to_string(),
            ],
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn load_snapshot_never_fetched_schemes_yield_no_keys(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "asin", "B004GXAX8C", None)
            .await
            .unwrap();

        let snapshot = load_snapshot(&pool, m_id).await.unwrap();
        assert!(
            snapshot.lookup_keys.is_empty(),
            "an asin-only manifestation has nothing fetchable, got {:?}",
            snapshot.lookup_keys
        );
    }

    /// An ISBN-less manifestation with a stored OL edition id resolves by
    /// native id, and the edition's parent-work link `AutoFill`s the empty
    /// work-level slot in the registry — with a journal pointer, no
    /// writeback, and no calls to the other adapters.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_external_id_autofills_empty_registry_slot(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "openlibrary", "OL7353617M", None)
            .await
            .unwrap();

        mock_ol_edition(
            &ol,
            "OL7353617M",
            json!({"works": [{"key": "/works/OL45804W"}]}),
        )
        .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let outcome = run_once(&pool, &cfg, m_id).await.unwrap();
        assert_eq!(outcome.applied, 1, "work-level id should AutoFill");

        let row = sqlx::query!(
            "SELECT external_id, source_version_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.external_id, "OL45804W");
        let version_id = row.source_version_id.expect("journal pointer wired");
        let journaled: serde_json::Value = sqlx::query_scalar!(
            "SELECT new_value AS \"new_value!\" FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(journaled, json!("OL45804W"));

        let writebacks: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(writebacks, 0, "identifier applies never enqueue writeback");

        // Dispatch is per-scheme: the other adapters must not be called.
        assert!(
            gb.received_requests().await.unwrap_or_default().is_empty(),
            "googlebooks must not be queried for an openlibrary id"
        );
        assert!(
            hc.received_requests().await.unwrap_or_default().is_empty(),
            "hardcover must not be queried for an openlibrary id"
        );

        // The attempt cached under its own type-prefixed key.
        let kind: String = sqlx::query_scalar!(
            "SELECT response_kind::text AS \"kind!\" FROM api_cache \
             WHERE source = 'openlibrary' AND lookup_key = 'external:openlibrary:OL7353617M'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(kind, "hit");
    }

    /// A populated work-level slot with a different value must Stage, not be
    /// overwritten: single-slot replacement stays an explicit human accept.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_stages_when_registry_slot_populated_with_different_value(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "openlibrary", "OL7353617M", None)
            .await
            .unwrap();
        upsert_work_identifier(&pool, work_id, "openlibrary", "OL999W", None)
            .await
            .unwrap();

        mock_ol_edition(
            &ol,
            "OL7353617M",
            json!({"works": [{"key": "/works/OL45804W"}]}),
        )
        .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let outcome = run_once(&pool, &cfg, m_id).await.unwrap();
        assert_eq!(outcome.applied, 0);
        assert_eq!(outcome.staged, 1, "disagreeing observation must Stage");

        let resident: String = sqlx::query_scalar!(
            "SELECT external_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(resident, "OL999W", "populated slot must not be overwritten");

        let pending: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 \
               AND field_name = 'identifiers.work.openlibrary' \
               AND status = 'pending'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1, "the observation stays pending for review");
    }

    /// A prior pending row with a different value downgrades `AutoFill` to
    /// Stage even though the slot itself is empty (cross-source or
    /// cross-run disagreement).
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_identifier_disagreement_with_prior_pending_stages(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "openlibrary", "OL7353617M", None)
            .await
            .unwrap();
        // A disagreeing observation from an earlier run, still pending.
        let hash = value_hash::value_hash("identifiers.work.openlibrary", &json!("OL777W"));
        sqlx::query!(
            "INSERT INTO metadata_versions \
                 (manifestation_id, source, field_name, new_value, value_hash, \
                  match_type, confidence_score) \
             VALUES ($1, 'hardcover', 'identifiers.work.openlibrary', $2, $3, \
                     'external_id', 0.85)",
            m_id,
            json!("OL777W"),
            hash,
        )
        .execute(&pool)
        .await
        .unwrap();

        mock_ol_edition(
            &ol,
            "OL7353617M",
            json!({"works": [{"key": "/works/OL45804W"}]}),
        )
        .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let outcome = run_once(&pool, &cfg, m_id).await.unwrap();
        assert_eq!(outcome.applied, 0, "disagreement must downgrade AutoFill");
        assert!(outcome.staged >= 1);

        let rows: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_external_identifiers WHERE work_id = $1",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rows, 0, "no registry write on disagreement");
    }

    /// Ratings land in the cache keyed (manifestation, source), are updated
    /// in place on re-run, and are never journaled.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_upserts_ratings_in_place_and_never_journals_them(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "googlebooks", "zyTZAAAAYAAJ", None)
            .await
            .unwrap();

        let volume = |rating: f64, count: i64| {
            json!({
                "id": "zyTZAAAAYAAJ",
                "volumeInfo": {"averageRating": rating, "ratingsCount": count}
            })
        };
        Mock::given(method("GET"))
            .and(path("/volumes/zyTZAAAAYAAJ"))
            .respond_with(ResponseTemplate::new(200).set_body_json(volume(4.5, 100)))
            .mount(&gb)
            .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let row = sqlx::query!(
            "SELECT rating, rating_scale, review_count FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!((row.rating - 4.5).abs() < 1e-6);
        assert!((row.rating_scale - 5.0).abs() < 1e-6);
        assert_eq!(row.review_count, 100);

        // Ratings never enter the journal.
        let journaled: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name ILIKE '%rating%'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(journaled, 0, "rating observations must never be journaled");

        // Re-run with a changed score: same row, updated in place. Reset the
        // enrichment cache row first so the rerun actually refetches.
        gb.reset().await;
        Mock::given(method("GET"))
            .and(path("/volumes/zyTZAAAAYAAJ"))
            .respond_with(ResponseTemplate::new(200).set_body_json(volume(3.9, 250)))
            .mount(&gb)
            .await;
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let rows = sqlx::query!(
            "SELECT rating, review_count FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "re-run must update in place, not duplicate");
        assert!((rows[0].rating - 3.9).abs() < 1e-6);
        assert_eq!(rows[0].review_count, 250);
    }

    /// Two editions of one work keep independent per-source ratings: the
    /// manifestation-level cache cannot cross-clobber.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_ratings_are_independent_per_edition(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m1) = insert_isbnless_fixture(&pool, &marker).await;
        let path2 = format!("/tmp/orch-extid-b-{marker}.epub");
        let hash2 = format!("orch-extid-b-hash-{marker}");
        let m2: Uuid = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            path2,
            hash2,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        upsert_manifestation_identifier(&pool, m1, "googlebooks", "volAAA111", None)
            .await
            .unwrap();
        upsert_manifestation_identifier(&pool, m2, "googlebooks", "volBBB222", None)
            .await
            .unwrap();

        for (vol, rating) in [("volAAA111", 4.0), ("volBBB222", 2.0)] {
            Mock::given(method("GET"))
                .and(path(format!("/volumes/{vol}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "id": vol,
                    "volumeInfo": {"averageRating": rating, "ratingsCount": 10}
                })))
                .mount(&gb)
                .await;
        }

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m1).await.unwrap();
        let _ = run_once(&pool, &cfg, m2).await.unwrap();

        let r1 = sqlx::query_scalar!(
            "SELECT rating FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m1,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let r2 = sqlx::query_scalar!(
            "SELECT rating FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m2,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!((r1 - 4.0).abs() < 1e-6);
        assert!((r2 - 2.0).abs() < 1e-6, "editions keep independent ratings");
    }

    /// A missed native id falls through to the next key in order, and each
    /// attempt caches under its own type-prefixed key so the miss cannot
    /// poison the winning key's cache entry.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_fallback_tries_next_key_and_caches_per_key(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        // openlibrary outranks googlebooks (same priority, fixed precedence),
        // so the OL id is attempted first and misses.
        upsert_manifestation_identifier(&pool, m_id, "openlibrary", "OL404404M", None)
            .await
            .unwrap();
        upsert_manifestation_identifier(&pool, m_id, "googlebooks", "volFALLBACK", None)
            .await
            .unwrap();

        Mock::given(method("GET"))
            .and(path("/books/OL404404M.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&ol)
            .await;
        let title = format!("Fallback Title {marker}");
        Mock::given(method("GET"))
            .and(path("/volumes/volFALLBACK"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "volFALLBACK",
                "volumeInfo": {"title": title}
            })))
            .mount(&gb)
            .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let outcome = run_once(&pool, &cfg, m_id).await.unwrap();
        assert!(outcome.applied >= 1, "fallback key should produce applies");

        let canon_title: String = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(canon_title, title, "title applied from the fallback key");

        let miss_kind: String = sqlx::query_scalar!(
            "SELECT response_kind::text AS \"kind!\" FROM api_cache \
             WHERE source = 'openlibrary' AND lookup_key = 'external:openlibrary:OL404404M'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(miss_kind, "miss", "the missed attempt caches as a miss");

        let hit_kind: String = sqlx::query_scalar!(
            "SELECT response_kind::text AS \"kind!\" FROM api_cache \
             WHERE source = 'googlebooks' AND lookup_key = 'external:googlebooks:volFALLBACK'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            hit_kind, "hit",
            "the winning attempt caches under its own key"
        );
    }

    /// A rating-capable record that stops reporting a rating clears the
    /// cached row; a path that never carries rating data leaves it alone.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_clears_rating_when_provider_omits_it(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "googlebooks", "volDROPPED", None)
            .await
            .unwrap();
        // A rating cached by an earlier run.
        crate::models::external_rating::upsert_rating(
            &pool,
            m_id,
            "googlebooks",
            &seed_rating(4.5, 5.0, 100),
        )
        .await
        .unwrap();

        // The re-fetched Volume record no longer reports a rating.
        Mock::given(method("GET"))
            .and(path("/volumes/volDROPPED"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "volDROPPED",
                "volumeInfo": {"title": "Unrated"}
            })))
            .mount(&gb)
            .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remaining, 0,
            "a rating the provider no longer reports must be cleared"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_clears_rating_when_provider_reports_unusable_value(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        // Two editions cover both reachable halves of the range guard. The
        // scale half is unreachable from the adapters, which pin 5.0.
        let over_scale = Uuid::new_v4().simple().to_string();
        let (_ow, over_id) = insert_isbnless_fixture(&pool, &over_scale).await;
        upsert_manifestation_identifier(&pool, over_id, "googlebooks", "volOVER", None)
            .await
            .unwrap();
        let negative_count = Uuid::new_v4().simple().to_string();
        let (_nw, negative_id) = insert_isbnless_fixture(&pool, &negative_count).await;
        upsert_manifestation_identifier(&pool, negative_id, "googlebooks", "volNEG", None)
            .await
            .unwrap();
        for m in [over_id, negative_id] {
            crate::models::external_rating::upsert_rating(
                &pool,
                m,
                "googlebooks",
                &seed_rating(4.5, 5.0, 100),
            )
            .await
            .unwrap();
        }

        // Both payloads carry values the table's CHECK constraints reject.
        Mock::given(method("GET"))
            .and(path("/volumes/volOVER"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "volOVER",
                "volumeInfo": {"title": "Over Scale", "averageRating": 6.0, "ratingsCount": 12}
            })))
            .mount(&gb)
            .await;
        Mock::given(method("GET"))
            .and(path("/volumes/volNEG"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "volNEG",
                "volumeInfo": {"title": "Negative Count", "averageRating": 4.0, "ratingsCount": -1}
            })))
            .mount(&gb)
            .await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        for m in [over_id, negative_id] {
            let _ = run_once(&pool, &cfg, m).await.unwrap();
        }

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestation_external_ratings \
             WHERE manifestation_id = ANY($1::uuid[]) AND source = 'googlebooks'",
            &[over_id, negative_id][..],
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remaining, 0,
            "a rating the schema would reject must not survive as the cached score"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_retains_rating_on_non_rating_capable_path(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_isbnless_fixture(&pool, &marker).await;
        upsert_manifestation_identifier(&pool, m_id, "openlibrary", "OL7353617M", None)
            .await
            .unwrap();
        // An openlibrary rating cached from an earlier search-path run.
        crate::models::external_rating::upsert_rating(
            &pool,
            m_id,
            "openlibrary",
            &seed_rating(4.2, 5.0, 55),
        )
        .await
        .unwrap();

        // The native edition record carries no rating data either way.
        mock_ol_edition(&ol, "OL7353617M", json!({"title": "Dune"})).await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let _ = run_once(&pool, &cfg, m_id).await.unwrap();

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestation_external_ratings \
             WHERE manifestation_id = $1 AND source = 'openlibrary'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remaining, 1,
            "a path with no rating data must not clear the cached rating"
        );
    }

    /// Two sibling manifestations concurrently observing disagreeing
    /// work-level ids must not clobber: the work row is locked FOR UPDATE
    /// before the Apply-vs-Stage decision and the slot re-read under the
    /// lock, so the loser Stages against the winner's committed value.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_concurrent_work_identifier_disagreement_stages(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ol, gb, hc) = (
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        );
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m1) = insert_isbnless_fixture(&pool, &marker).await;
        let path2 = format!("/tmp/orch-conc-{marker}.epub");
        let hash2 = format!("orch-conc-hash-{marker}");
        let m2: Uuid = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            path2,
            hash2,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        upsert_manifestation_identifier(&pool, m1, "openlibrary", "OL111M", None)
            .await
            .unwrap();
        upsert_manifestation_identifier(&pool, m2, "openlibrary", "OL222M", None)
            .await
            .unwrap();

        mock_ol_edition(&ol, "OL111M", json!({"works": [{"key": "/works/OL111W"}]})).await;
        mock_ol_edition(&ol, "OL222M", json!({"works": [{"key": "/works/OL222W"}]})).await;

        let cfg = config_with_mock_sources(&ol.uri(), &gb.uri(), &hc.uri(), Some("test-token"));
        let (a, b) = tokio::join!(run_once(&pool, &cfg, m1), run_once(&pool, &cfg, m2));
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(
            a.applied + b.applied,
            1,
            "exactly one run wins the empty slot"
        );
        assert_eq!(
            a.staged + b.staged,
            1,
            "the other run stages against the committed value"
        );

        let rows = sqlx::query!(
            "SELECT external_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "single slot survives the race");
        let resident = rows[0].external_id.clone();
        assert!(["OL111W", "OL222W"].contains(&resident.as_str()));

        let loser = if resident == "OL111W" {
            "OL222W"
        } else {
            "OL111W"
        };
        let pending: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE field_name = 'identifiers.work.openlibrary' \
               AND new_value = to_jsonb($1::text) \
               AND status = 'pending' \
               AND manifestation_id IN ($2, $3)",
            loser,
            m1,
            m2,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1, "the losing value stays pending for review");
    }

    /// A manifestation-level identifier entered manually while enrichment is
    /// mid-run must not be clobbered. The editor transaction holds the
    /// manifestation row FOR UPDATE (as the PATCH handler does at entry) and
    /// has written the slot but not yet committed when the enrichment batch
    /// starts; the batch must serialise behind the editor at its own
    /// manifestation-row lock, re-read the slot after the editor commits,
    /// and stage its disagreeing observation instead of overwriting the
    /// operator's value.
    ///
    /// The journal row is committed up front, modelling a repeat observation:
    /// on that path the journal upsert takes its DO UPDATE arm, which locks
    /// no parent row, so the batch-entry lock is the only thing standing
    /// between the emptiness decision and the concurrent edit (a fresh
    /// observation's INSERT would incidentally serialise through the FK
    /// check's KEY SHARE on the manifestation row and mask a missing lock).
    /// The test releases the editor only once the batch is provably blocked
    /// (an ungranted lock appears in `pg_locks`), so the decision window
    /// genuinely overlaps the edit.
    #[sqlx::test(migrations = "./migrations")]
    async fn manual_manifestation_identifier_edit_mid_run_is_not_clobbered(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = format!("978{}", &marker[..10]);
        let (_work_id, m_id) = insert_enrich_fixture(&ing, &isbn, &marker).await;

        let field = "identifiers.manifestation.googlebooks";
        let provider_value = json!("volPROVIDER2");
        let provider_hash = value_hash::value_hash(field, &provider_value);
        let row_id = sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                 (manifestation_id, source, field_name, new_value, value_hash, \
                  match_type, confidence_score) \
             VALUES ($1, 'googlebooks', $2, $3, $4, 'isbn', 0.9) \
             RETURNING id",
            m_id,
            field,
            provider_value,
            provider_hash,
        )
        .fetch_one(&ing)
        .await
        .unwrap();

        // Editor: manifestation row locked, slot written, commit withheld.
        let mut editor_tx = ing.begin().await.unwrap();
        sqlx::query!(
            "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE",
            m_id,
        )
        .fetch_optional(&mut *editor_tx)
        .await
        .unwrap();
        upsert_manifestation_identifier(&mut *editor_tx, m_id, "googlebooks", "volOPERATOR1", None)
            .await
            .unwrap();

        // Enrichment: canonical batch over the pre-journaled observation, in
        // its own transaction.
        let run = {
            let ing = ing.clone();
            async move {
                let snapshot = load_snapshot(&ing, m_id).await.unwrap();
                let mut tx = ing.begin().await.unwrap();
                let mut per_field: PerFieldRows = std::collections::HashMap::new();
                per_field.insert(
                    field.into(),
                    vec![(
                        "googlebooks".into(),
                        PolicyInputRow {
                            id: row_id,
                            value_hash: provider_hash,
                        },
                    )],
                );
                let outcome = apply_canonical_batch(&mut tx, &snapshot, &per_field)
                    .await
                    .unwrap();
                tx.commit().await.unwrap();
                outcome
            }
        };
        let batch = tokio::spawn(run);

        // Release the editor only once the batch is provably waiting on it.
        let mut waited_ms = 0;
        loop {
            let blocked: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM pg_locks WHERE NOT granted",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if blocked > 0 {
                break;
            }
            assert!(
                waited_ms < 5000,
                "enrichment batch never blocked behind the editor transaction"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            waited_ms += 10;
        }
        editor_tx.commit().await.unwrap();

        let outcome = batch.await.unwrap();
        assert_eq!(
            outcome.applied, 0,
            "the operator's mid-run edit owns the slot; enrichment must not apply"
        );
        assert_eq!(
            outcome.staged, 1,
            "the provider observation stages as a disagreement"
        );

        let resident: String = sqlx::query_scalar!(
            "SELECT external_id FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'googlebooks'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            resident, "volOPERATOR1",
            "the operator value survives the race"
        );

        let pending: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE field_name = 'identifiers.manifestation.googlebooks' \
               AND new_value = to_jsonb('volPROVIDER2'::text) \
               AND status = 'pending' \
               AND manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1, "the provider value stays pending for review");
    }
}
