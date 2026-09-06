//! Settings persistence, validation, and live-reload via LISTEN/NOTIFY.
//!
//! See ADR `docs/adr/0012-persist-operator-tunable-settings-to-database-with-live-reload.md`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::models::settings::{Settings, UpdateSettings};

/// Load the singleton settings row from the database.
///
/// # Errors
/// Returns `sqlx::Error` on connection or query failure.
pub async fn load(pool: &PgPool) -> Result<Settings, sqlx::Error> {
    sqlx::query_as!(
        Settings,
        r#"SELECT
            enrichment_enabled,
            enrichment_concurrency,
            enrichment_poll_idle_secs,
            enrichment_fetch_budget_secs,
            cover_max_bytes,
            cover_download_timeout_secs,
            cover_min_long_edge_px,
            cover_redirect_limit,
            writeback_enabled,
            writeback_concurrency,
            writeback_poll_idle_secs,
            writeback_max_attempts,
            opds_enabled,
            opds_page_size,
            format_priority,
            cleanup_mode,
            openlibrary_base_url,
            googlebooks_base_url,
            hardcover_base_url,
            provider_visibility,
            revision,
            updated_at
        FROM settings
        WHERE id = true"#,
    )
    .fetch_one(pool)
    .await
}

/// Install `candidate` into the cache slot only when its `revision` is newer
/// than the resident one.
///
/// Every cache writer (the PUT handler's immediate
/// swap, the NOTIFY reload, and the fallback poll) goes through this guard,
/// so the resident snapshot is a monotonic replica of the DB row: a writer
/// that read an older row version can never overwrite a newer one,
/// regardless of the order the writers reach the lock. Returns whether the
/// candidate was installed.
pub fn apply_if_newer(resident: &mut Settings, candidate: Settings) -> bool {
    if candidate.revision > resident.revision {
        *resident = candidate;
        true
    } else {
        false
    }
}

/// Failure modes of [`validate_provider_keys`], so the handler can map an
/// unknown key to a validation response and a query failure to an internal
/// error.
#[derive(Debug, thiserror::Error)]
pub enum ProviderKeyError {
    /// A `provider_visibility` key outside the known provider vocabulary.
    #[error("unknown provider_visibility key '{0}'")]
    UnknownKey(String),
    /// The vocabulary lookup itself failed.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Validate that every `provider_visibility` key names a known provider: a
/// member of the `identifier_schemes` and `rating_sources` union.
///
/// The vocabulary lives in reference tables (readable by every role), so the
/// closed set the projections filter on is DB-defined, not hardcoded.
///
/// # Errors
/// [`ProviderKeyError::UnknownKey`] for the first unknown key,
/// [`ProviderKeyError::Db`] on query failure.
pub async fn validate_provider_keys(
    pool: &PgPool,
    req: &UpdateSettings,
) -> Result<(), ProviderKeyError> {
    let Some(ref visibility) = req.provider_visibility else {
        return Ok(());
    };
    if visibility.is_empty() {
        return Ok(());
    }
    let known: Vec<String> = sqlx::query_scalar!(
        r#"SELECT id AS "id!" FROM identifier_schemes
           UNION
           SELECT id AS "id!" FROM rating_sources"#,
    )
    .fetch_all(pool)
    .await?;
    let known: std::collections::HashSet<&str> = known.iter().map(String::as_str).collect();
    if let Some(unknown) = visibility.keys().find(|k| !known.contains(k.as_str())) {
        return Err(ProviderKeyError::UnknownKey(unknown.clone()));
    }
    Ok(())
}

/// Apply a partial update to the settings row.
///
/// Uses dynamic SQL via `QueryBuilder` because the set of columns to
/// update is runtime-determined (only non-None fields are SET).
///
/// The UPDATE fires the `settings_changed_trigger` which issues
/// `pg_notify('settings_changed', '')`.
///
/// # Co-maintenance note
///
/// The RETURNING column list must match `Settings` field order exactly.
/// `build_query_as::<Settings>()` will fail at test time if columns
/// diverge (sqlx offline check catches it too). When adding a column
/// to `Settings`, update the RETURNING clause here AND the SELECT in
/// [`load`].
///
/// # Errors
/// Returns `sqlx::Error` on connection or query failure.
pub async fn save(pool: &PgPool, req: &UpdateSettings) -> Result<Settings, sqlx::Error> {
    use sqlx::{Postgres, QueryBuilder};

    debug_assert!(!req.is_empty(), "save() called with empty UpdateSettings");
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE settings SET ");
    let mut separated = qb.separated(", ");

    if let Some(v) = req.enrichment_enabled {
        separated.push("enrichment_enabled = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.enrichment_concurrency {
        separated.push("enrichment_concurrency = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.enrichment_poll_idle_secs {
        separated.push("enrichment_poll_idle_secs = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.enrichment_fetch_budget_secs {
        separated.push("enrichment_fetch_budget_secs = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.cover_max_bytes {
        separated.push("cover_max_bytes = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.cover_download_timeout_secs {
        separated.push("cover_download_timeout_secs = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.cover_min_long_edge_px {
        separated.push("cover_min_long_edge_px = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.cover_redirect_limit {
        separated.push("cover_redirect_limit = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.writeback_enabled {
        separated.push("writeback_enabled = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.writeback_concurrency {
        separated.push("writeback_concurrency = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.writeback_poll_idle_secs {
        separated.push("writeback_poll_idle_secs = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.writeback_max_attempts {
        separated.push("writeback_max_attempts = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.opds_enabled {
        separated.push("opds_enabled = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(v) = req.opds_page_size {
        separated.push("opds_page_size = ");
        separated.push_bind_unseparated(v);
    }
    if let Some(ref v) = req.format_priority {
        let strings: Vec<String> = v.iter().map(ToString::to_string).collect();
        separated.push("format_priority = ");
        separated.push_bind_unseparated(strings);
    }
    if let Some(ref v) = req.cleanup_mode {
        separated.push("cleanup_mode = ");
        separated.push_bind_unseparated(v.as_str().to_owned());
    }
    if let Some(ref v) = req.openlibrary_base_url {
        separated.push("openlibrary_base_url = ");
        separated.push_bind_unseparated(v.as_str());
    }
    if let Some(ref v) = req.googlebooks_base_url {
        separated.push("googlebooks_base_url = ");
        separated.push_bind_unseparated(v.as_str());
    }
    if let Some(ref v) = req.hardcover_base_url {
        separated.push("hardcover_base_url = ");
        separated.push_bind_unseparated(v.as_str());
    }
    if let Some(ref v) = req.provider_visibility {
        let obj: serde_json::Map<String, serde_json::Value> = v
            .iter()
            .map(|(k, &b)| (k.clone(), serde_json::Value::Bool(b)))
            .collect();
        separated.push("provider_visibility = ");
        separated.push_bind_unseparated(serde_json::Value::Object(obj));
    }

    // The row lock serialises concurrent updates, so the last committer
    // always returns the highest revision; cache writers gate their swap on
    // it (see `apply_if_newer`).
    separated.push("revision = revision + 1");
    separated.push("updated_at = now()");

    qb.push(" WHERE id = true RETURNING enrichment_enabled, enrichment_concurrency, enrichment_poll_idle_secs, enrichment_fetch_budget_secs, cover_max_bytes, cover_download_timeout_secs, cover_min_long_edge_px, cover_redirect_limit, writeback_enabled, writeback_concurrency, writeback_poll_idle_secs, writeback_max_attempts, opds_enabled, opds_page_size, format_priority, cleanup_mode, openlibrary_base_url, googlebooks_base_url, hardcover_base_url, provider_visibility, revision, updated_at");

    qb.build_query_as::<Settings>().fetch_one(pool).await
}

/// Spawn the LISTEN/NOTIFY + fallback poll background task.
///
/// Listens on the `settings_changed` channel. On notification (or every
/// 60 seconds as fallback), re-loads settings from DB and updates the
/// shared `RwLock`.
///
/// Automatically reconnects on connection failure with a 5-second backoff.
/// Runs until the `CancellationToken` is cancelled (graceful shutdown).
pub async fn spawn_listener(
    pool: PgPool,
    settings: Arc<RwLock<Settings>>,
    last_reload: Arc<RwLock<Option<DateTime<Utc>>>>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        if let Err(e) = listen_loop(&pool, &settings, &last_reload, &cancel).await {
            tracing::warn!(error = %e, "settings listener disconnected, reconnecting in 5s");
            tokio::select! {
                biased;
                () = cancel.cancelled() => return,
                () = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            }
        }
    }
}

async fn listen_loop(
    pool: &PgPool,
    settings: &Arc<RwLock<Settings>>,
    last_reload: &Arc<RwLock<Option<DateTime<Utc>>>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), sqlx::Error> {
    let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
    listener.listen("settings_changed").await?;
    tracing::info!("settings listener connected");

    let mut poll_interval = tokio::time::interval(std::time::Duration::from_mins(1));
    poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    poll_interval.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(()),
            notification = listener.recv() => {
                match notification {
                    Ok(_) => {
                        refresh(pool, settings, last_reload).await;
                    }
                    Err(e) => return Err(e),
                }
            }
            _ = poll_interval.tick() => {
                refresh(pool, settings, last_reload).await;
            }
        }
    }
}

async fn refresh(
    pool: &PgPool,
    settings: &Arc<RwLock<Settings>>,
    last_reload: &Arc<RwLock<Option<DateTime<Utc>>>>,
) {
    match load(pool).await {
        Ok(new_settings) => {
            // Monotonic swap: a reload that raced a concurrent writer and
            // read an older row version must not roll the cache back.
            let mut guard = settings.write().await;
            apply_if_newer(&mut guard, new_settings);
            drop(guard);
            let mut ts = last_reload.write().await;
            *ts = Some(Utc::now());
        }
        Err(e) => {
            let last = *last_reload.read().await;
            let cache_age_secs = last.map_or(-1, |ts| Utc::now().timestamp() - ts.timestamp());
            tracing::error!(
                error = %e,
                cache_age_secs,
                "failed to reload settings; serving stale cache"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::UpdateSettings;

    fn empty_update() -> UpdateSettings {
        UpdateSettings {
            enrichment_enabled: None,
            enrichment_concurrency: None,
            enrichment_poll_idle_secs: None,
            enrichment_fetch_budget_secs: None,
            cover_max_bytes: None,
            cover_download_timeout_secs: None,
            cover_min_long_edge_px: None,
            cover_redirect_limit: None,
            writeback_enabled: None,
            writeback_concurrency: None,
            writeback_poll_idle_secs: None,
            writeback_max_attempts: None,
            opds_enabled: None,
            opds_page_size: None,
            format_priority: None,
            cleanup_mode: None,
            openlibrary_base_url: None,
            googlebooks_base_url: None,
            hardcover_base_url: None,
            provider_visibility: None,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn save_increments_revision_monotonically(pool: PgPool) {
        let before = load(&pool).await.expect("load");
        let mut req = empty_update();
        req.opds_page_size = Some(60);
        let first = save(&pool, &req).await.expect("first save");
        req.opds_page_size = Some(70);
        let second = save(&pool, &req).await.expect("second save");
        assert!(first.revision > before.revision);
        assert!(
            second.revision > first.revision,
            "every save must return a strictly higher revision"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stale_snapshot_cannot_overwrite_newer_cache(pool: PgPool) {
        // Deterministic form of the concurrent-PUT race: writer A commits
        // first (lower revision) but reaches the cache lock after writer B.
        // The guard must keep B resident no matter the arrival order.
        let mut req = empty_update();
        req.opds_page_size = Some(61);
        let older = save(&pool, &req).await.expect("save A");
        req.opds_page_size = Some(71);
        let newer = save(&pool, &req).await.expect("save B");

        let mut resident = older.clone();
        assert!(
            apply_if_newer(&mut resident, newer.clone()),
            "newer snapshot must install"
        );
        assert!(
            !apply_if_newer(&mut resident, older),
            "stale snapshot must be rejected once a newer one is resident"
        );
        assert_eq!(resident.revision, newer.revision);
        assert_eq!(resident.opds_page_size, 71);

        // Same-revision reload (NOTIFY / poll re-reading the row it already
        // has) is a no-op, not a rollback.
        assert!(!apply_if_newer(&mut resident, newer));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn provider_visibility_round_trips_through_save_and_load(pool: PgPool) {
        let mut req = empty_update();
        req.provider_visibility = Some(
            [
                ("googlebooks".to_string(), false),
                ("asin".to_string(), true),
            ]
            .into_iter()
            .collect(),
        );
        let saved = save(&pool, &req).await.expect("save");
        assert_eq!(
            saved.provider_visibility,
            serde_json::json!({"asin": true, "googlebooks": false})
        );
        let loaded = load(&pool).await.expect("load");
        assert_eq!(loaded.provider_visibility, saved.provider_visibility);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn provider_keys_accept_union_and_reject_outsiders(pool: PgPool) {
        // `asin` is only in identifier_schemes, `amazon` only in
        // rating_sources: both sides of the union must be accepted.
        let mut req = empty_update();
        req.provider_visibility = Some(
            [("asin".to_string(), false), ("amazon".to_string(), false)]
                .into_iter()
                .collect(),
        );
        validate_provider_keys(&pool, &req)
            .await
            .expect("union members accepted");

        // `manual` is a metadata_sources observer but neither a scheme nor a
        // rating source, so it is not a valid visibility key.
        req.provider_visibility = Some(std::collections::BTreeMap::from([(
            "manual".to_string(),
            false,
        )]));
        let err = validate_provider_keys(&pool, &req)
            .await
            .expect_err("non-union key rejected");
        assert!(matches!(err, ProviderKeyError::UnknownKey(k) if k == "manual"));
    }
}
