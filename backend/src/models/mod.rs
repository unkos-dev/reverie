//! Domain models and database queries.
//!
//! Each submodule maps one Postgres table or `ENUM` type to its Rust
//! shape: row structs, query helpers, and — for closed-set columns — a
//! typed [`sqlx::Type`] wrapper that fails closed on unknown DB variants.

/// Per-user device tokens for OPDS / mobile-client Basic-auth flows.
pub mod device_token;
/// Closed value set for the `enrichment_status` Postgres `ENUM`.
pub mod enrichment_status;
/// Per-file ingestion job rows produced by the import pipeline.
pub mod ingestion_job;
/// Closed value set for the `ingestion_status` Postgres `ENUM`.
pub mod ingestion_status;
/// Response DTOs for the JSON library API (`/api/books`, `/api/works`).
pub mod library;
/// Closed value set for the `manifestation_format` Postgres `ENUM`.
pub mod manifestation_format;
/// Per-`(user, manifestation)` reading-progress and last-read timestamp.
pub mod reading_state;
/// Closed value set for the `user_role` Postgres `ENUM`.
pub mod role;
/// Response DTOs for the series API (`/api/series/{id}`).
pub mod series;
/// Persisted operator-tunable settings (single-row `settings` table).
pub mod settings;
/// Response DTOs for the shelves CRUD API (`/api/shelves*`).
pub mod shelf;
/// Closed value set for the `theme_preference` Postgres `ENUM`.
pub mod theme_preference;
/// User accounts and OIDC-driven upsert/promotion flow.
pub mod user;
/// Work matching, stub creation, and ISBN-driven rematch.
pub mod work;
