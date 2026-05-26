//! Settings persistence, validation, and live-reload via LISTEN/NOTIFY.
//!
//! See ADR `adr/2026-05-26-persisted-settings.md`.

use std::sync::Arc;

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
            updated_at
        FROM settings
        WHERE id = true"#,
    )
    .fetch_one(pool)
    .await
}

/// Apply a partial update to the settings row.
///
/// Uses dynamic SQL via `QueryBuilder` because the set of columns to
/// update is runtime-determined (only non-None fields are SET).
///
/// The UPDATE fires the `settings_changed_trigger` which issues
/// `pg_notify('settings_changed', '')`.
///
/// # Errors
/// Returns `sqlx::Error` on connection or query failure.
pub async fn save(pool: &PgPool, req: &UpdateSettings) -> Result<Settings, sqlx::Error> {
    use sqlx::{Postgres, QueryBuilder};

    let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("UPDATE settings SET ");
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
        separated.push("format_priority = ");
        separated.push_bind_unseparated(v.as_slice());
    }
    if let Some(ref v) = req.cleanup_mode {
        separated.push("cleanup_mode = ");
        separated.push_bind_unseparated(v.as_str());
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

    separated.push("updated_at = now()");

    qb.push(" WHERE id = true RETURNING enrichment_enabled, enrichment_concurrency, enrichment_poll_idle_secs, enrichment_fetch_budget_secs, cover_max_bytes, cover_download_timeout_secs, cover_min_long_edge_px, cover_redirect_limit, writeback_enabled, writeback_concurrency, writeback_poll_idle_secs, writeback_max_attempts, opds_enabled, opds_page_size, format_priority, cleanup_mode, openlibrary_base_url, googlebooks_base_url, hardcover_base_url, updated_at");

    qb.build_query_as::<Settings>().fetch_one(pool).await
}

/// Spawn the LISTEN/NOTIFY + fallback poll background task.
///
/// Listens on the `settings_changed` channel. On notification (or every
/// 60 seconds as fallback), re-loads settings from DB and updates the
/// shared `RwLock`.
///
/// Runs until the `CancellationToken` is cancelled (graceful shutdown).
pub async fn spawn_listener(
    pool: PgPool,
    settings: Arc<RwLock<Settings>>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        if let Err(e) = listen_loop(&pool, &settings, &cancel).await {
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
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), sqlx::Error> {
    let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
    listener.listen("settings_changed").await?;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(()),
            notification = listener.recv() => {
                match notification {
                    Ok(_) => {
                        refresh(pool, settings).await;
                    }
                    Err(e) => return Err(e),
                }
            }
            () = tokio::time::sleep(std::time::Duration::from_mins(1)) => {
                refresh(pool, settings).await;
            }
        }
    }
}

async fn refresh(pool: &PgPool, settings: &Arc<RwLock<Settings>>) {
    match load(pool).await {
        Ok(new_settings) => {
            let mut guard = settings.write().await;
            *guard = new_settings;
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to reload settings from database");
        }
    }
}
