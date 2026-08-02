//! Domain models and database queries.
//!
//! Each submodule maps one Postgres table or `ENUM` type to its Rust
//! shape: row structs, query helpers, and — for closed-set columns — a
//! typed [`sqlx::Type`] wrapper that fails closed on unknown DB variants.

/// Closed value set for the `content_rating` Postgres `ENUM`.
pub mod content_rating;
/// Closed value set for the `author_role` Postgres `ENUM`.
pub mod contributor_role;
/// Per-user device tokens for OPDS / mobile-client Basic-auth flows.
pub mod device_token;
/// Closed value set for the `enrichment_status` Postgres `ENUM`.
pub mod enrichment_status;
/// External-source identifier registry (work + manifestation level), keyed
/// `(entity, scheme)` and journal-integrated.
pub mod external_identifier;
/// Per-source aggregate ratings cache, keyed `(manifestation, source)`.
pub mod external_rating;
/// Closed value set for the `identity_provider` Postgres `ENUM`.
pub mod identity_provider;
/// Per-file ingestion job rows produced by the import pipeline.
pub mod ingestion_job;
/// Closed value set for the `ingestion_status` Postgres `ENUM`.
pub mod ingestion_status;
/// Response DTOs for the JSON library API (`/api/v1/books`, `/api/v1/works`).
pub mod library;
/// Local password credentials (seam for local-account login).
pub mod local_credentials;
/// DB-backed per-account login throttle (escalating backoff, CLI-clearable).
pub mod login_throttle;
/// Closed value set for the `manifestation_format` Postgres `ENUM`.
pub mod manifestation_format;
/// Hashed, single-use, short-lived password-reset PIN records.
pub mod password_reset_pin;
/// Per-`(user, manifestation)` reading state: status, rating, notes,
/// progress, and reading dates.
pub mod reading_state;
/// Closed value set for the `reading_status` Postgres `ENUM`.
pub mod reading_status;
/// Closed value set for the `user_role` Postgres `ENUM`.
pub mod role;
/// Response DTOs for the series API (`/api/v1/series/{id}`).
pub mod series;
/// Persisted operator-tunable settings (single-row `settings` table).
pub mod settings;
/// Response DTOs for the shelves CRUD API (`/api/v1/shelves*`).
pub mod shelf;
/// Closed value set for the `theme_preference` Postgres `ENUM`.
pub mod theme_preference;
/// User accounts and OIDC-driven upsert flow.
pub mod user;
/// External-provider identity links keyed on `(issuer, subject)`.
pub mod user_identities;
/// Closed value set for the `validation_status` Postgres `ENUM`.
pub mod validation_status;
/// Work matching, stub creation, and ISBN-driven rematch.
pub mod work;
