//! First-party Postgres [`SessionStore`](tower_sessions::session_store::SessionStore)
//! for `tower-sessions`.
//!
//! Replaces the abandoned `tower-sessions-sqlx-store` crate (ADR
//! `2026-06-04-first-party-session-layer.md`). Targets the **unchanged**
//! `tower_sessions.session` table (`id text`, `data bytea`, `expiry_date
//! timestamptz`) — schema, grants, RLS-exemption, and the `expiry_date` index
//! are carried forward from the superseded sqlx-store ADR. The reaper lives in
//! [`crate::services::session_sweep`], driving
//! [`ExpiredDeletion`](tower_sessions::session_store::ExpiredDeletion) hourly.
//!
//! # Tier 2 — security-critical
//!
//! THREAT: session rows are the bootstrap for user identity, so this store is
//! on the authentication-critical path. Two invariants are load-bearing:
//! [`SessionStore::load`](tower_sessions::session_store::SessionStore::load)
//! filters `expiry_date > now()` so an expired cookie can never resurrect a
//! session; [`SessionStore::create`](tower_sessions::session_store::SessionStore::create)
//! inserts atomically with `ON CONFLICT DO NOTHING` and regenerates the id on a
//! collision rather than overwriting — so a concurrent creator can never clobber
//! a live session row (the previous check-then-upsert left a TOCTOU window). A
//! real collision is cryptographically improbable anyway (128-bit CSPRNG ids).
//! The `tower_sessions.session` table is intentionally RLS-exempt — session
//! load must precede user resolution, so RLS-gating it is chicken-and-egg
//! (see `backend/CLAUDE.md`); access is bounded at the role-grant layer
//! (`reverie_app` DML only).

use async_trait::async_trait;
use sqlx::PgPool;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, ExpiredDeletion, SessionStore};

/// Postgres-backed [`SessionStore`](tower_sessions::session_store::SessionStore)
/// over `tower_sessions.session`.
///
/// Serializes the whole [`Record`] (id + data map + expiry) into the `data
/// bytea` column via `MessagePack` (`rmp_serde`); the `id text` and `expiry_date
/// timestamptz` columns are stored alongside for lookup and expiry filtering.
#[derive(Clone, Debug)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Build a store over the given pool. The pool must hold `reverie_app`
    /// (or higher) credentials — the only role granted DML on
    /// `tower_sessions.session`.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a record by id (`ON CONFLICT DO UPDATE`). Used by `save`; `create`
    /// has its own atomic insert-only path.
    async fn upsert(&self, record: &Record) -> session_store::Result<()> {
        let id = record.id.to_string();
        let data =
            rmp_serde::to_vec(record).map_err(|e| session_store::Error::Encode(e.to_string()))?;
        sqlx::query!(
            "INSERT INTO tower_sessions.session (id, data, expiry_date) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (id) DO UPDATE \
               SET data = excluded.data, expiry_date = excluded.expiry_date",
            id,
            data,
            record.expiry_date,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for PostgresStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        // The default trait impl delegates to `save` (an unconditional upsert)
        // with no collision check. Override with an atomic insert: `ON CONFLICT
        // DO NOTHING RETURNING id` returns a row only when the id was free, so a
        // NULL result means a collision with a live row — regenerate the id and
        // retry. Unlike a check-then-upsert, there is no TOCTOU window in which a
        // concurrent creator could overwrite a live session.
        // (tower-sessions 0.15 routes freshly-created sessions — including those
        // minted by `cycle_id` — through `create` rather than `save`; re-verify
        // against upstream if bumping tower-sessions.)
        loop {
            let id = record.id.to_string();
            let data = rmp_serde::to_vec(record)
                .map_err(|e| session_store::Error::Encode(e.to_string()))?;
            let inserted = sqlx::query_scalar!(
                "INSERT INTO tower_sessions.session (id, data, expiry_date) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO NOTHING \
                 RETURNING id",
                id,
                data,
                record.expiry_date,
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
            if inserted.is_some() {
                return Ok(());
            }
            record.id = Id::default();
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        self.upsert(record).await
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id = session_id.to_string();
        // THREAT: the `expiry_date > now()` filter is the seam that stops an
        // expired cookie resolving to a live identity — do not relax it.
        let row = sqlx::query!(
            "SELECT data FROM tower_sessions.session WHERE id = $1 AND expiry_date > now()",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        match row {
            Some(row) => {
                let record = rmp_serde::from_slice(&row.data)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id = session_id.to_string();
        sqlx::query!("DELETE FROM tower_sessions.session WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for PostgresStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        sqlx::query!("DELETE FROM tower_sessions.session WHERE expiry_date < now()")
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}
