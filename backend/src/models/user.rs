//! User accounts and OIDC-driven upsert / first-user promotion.
//!
//! Two row shapes coexist: a private `UserRow` decoded from the DB and
//! the public [`crate::models::user::User`] returned to callers. The
//! split keeps `axum-login`-required derived state
//! (`session_version_bytes`) out of the serialised JSON shape and lets
//! `User::from` compute it once at row-load time.

use axum_login::AuthUser;
use email_address::EmailAddress;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::models::role::Role;
use crate::models::theme_preference::ThemePreference;

/// Raw row from the database. Use `User::from` to get the public type.
#[derive(Debug, Clone, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    oidc_subject: String,
    display_name: String,
    email: Option<String>,
    role: Role,
    is_child: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    session_version: i32,
    theme_preference: ThemePreference,
}

/// Public user row exposed to handlers and serialised in API responses.
///
/// The [`AuthUser`] impl returns `session_version_bytes` (a cached
/// little-endian encoding of `session_version`) from
/// `session_auth_hash`; bumping `users.session_version` therefore
/// invalidates every existing session for that user — see the comment
/// in the impl for the rationale over hashing `updated_at`.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    /// Primary key.
    pub id: Uuid,
    /// `sub` claim from the trusted OIDC issuer; the cross-`IdP`-stable
    /// identity used for upsert lookup.
    pub oidc_subject: String,
    /// User-facing display name; sourced from the OIDC `name` claim.
    pub display_name: String,
    /// User's email if the `IdP` released the `email` claim, else `None`.
    pub email: Option<String>,
    /// Authorization role; see [`Role`].
    pub role: Role,
    /// `true` if this account is a child profile subject to age-gating.
    /// Mirrors a column rather than being derived from `role` so the DB
    /// remains the single source of truth across both axes.
    pub is_child: bool,
    /// Row insert timestamp.
    pub created_at: OffsetDateTime,
    /// `now()` of the most recent change to any user-facing field.
    pub updated_at: OffsetDateTime,
    /// Monotonic counter incremented to force-invalidate every active
    /// session for this user; consumed via [`AuthUser::session_auth_hash`].
    pub session_version: i32,
    /// Selected UI theme; see [`ThemePreference`].
    pub theme_preference: ThemePreference,
    #[serde(skip)]
    session_version_bytes: Vec<u8>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        let session_version_bytes = row.session_version.to_le_bytes().to_vec();
        Self {
            id: row.id,
            oidc_subject: row.oidc_subject,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            is_child: row.is_child,
            created_at: row.created_at,
            updated_at: row.updated_at,
            session_version: row.session_version,
            theme_preference: row.theme_preference,
            session_version_bytes,
        }
    }
}

impl AuthUser for User {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        // Intentional session invalidation: incrementing session_version forces
        // logout of all sessions for this user. This is preferred over hashing
        // updated_at because it only invalidates when we explicitly want it to
        // (e.g., admin action, security event), not on every profile update.
        &self.session_version_bytes
    }
}

/// Fetch a user by primary key. Returns `Ok(None)` if no such row exists.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, oidc_subject, display_name, email, \
                role AS \"role: Role\", is_child, created_at, updated_at, \
                session_version, theme_preference AS \"theme_preference: ThemePreference\" \
         FROM users WHERE id = $1",
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(User::from))
}

/// Fetch a user by OIDC `sub` claim. Returns `Ok(None)` if no such row exists.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
#[allow(dead_code)] // Used by admin user management in future steps
pub async fn find_by_oidc_subject(
    pool: &PgPool,
    subject: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, oidc_subject, display_name, email, \
                role AS \"role: Role\", is_child, created_at, updated_at, \
                session_version, theme_preference AS \"theme_preference: ThemePreference\" \
         FROM users WHERE oidc_subject = $1",
        subject,
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(User::from))
}

/// Insert or update a user from OIDC claims, then auto-promote to admin if first user.
/// Runs upsert + promotion in a single transaction to prevent race conditions where
/// concurrent first logins result in no admin.
///
/// THREAT: `email` backs the case-insensitive unique index `idx_users_email_lower`
/// and is a provider-matching key. The OIDC JWT is signature-verified, but the
/// `email` *string* is not format-checked upstream, so a misconfigured or
/// non-standard `IdP` could release a malformed value. This is the same column the
/// admin `PATCH /api/users/{id}` path guards with [`EmailAddress::is_valid`]; the
/// claim is validated here so both write paths uphold the RFC-5322 invariant
/// (UNK-309). Per OIDC Core §5.7 the `email` claim is optional and non-identifying
/// (identity rides on `sub`/`oidc_subject`), so an invalid claim degrades to `NULL`
/// rather than failing authentication — login never depends on an optional claim's
/// format. The rejected value is logged by shape only (length), never verbatim
/// (Hard Rule 7).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from any step of the transaction (advisory
/// lock, `INSERT … ON CONFLICT`, conditional promotion `UPDATE`,
/// re-fetch, or commit).
pub async fn upsert_from_oidc_and_maybe_promote(
    pool: &PgPool,
    subject: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<User, sqlx::Error> {
    // Validate the OIDC email claim before persisting (UNK-309). Trim first so a
    // whitespace-only claim reads as absence; degrade a non-empty-but-invalid
    // value to NULL with a shape-only warning.
    let email = match email.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) if EmailAddress::is_valid(e) => Some(e),
        Some(e) => {
            tracing::warn!(
                rejected_email_len = e.len(),
                "OIDC email claim is not RFC-5322 valid; persisting NULL and matching on sub (UNK-309)"
            );
            None
        }
        None => None,
    };

    let mut tx = pool.begin().await?;

    // Serialize concurrent first-user promotion attempts. Without this lock,
    // two concurrent transactions under READ COMMITTED could both see count=1
    // (their own uncommitted insert) and both promote to admin.
    sqlx::query!("SELECT pg_advisory_xact_lock(42)")
        .execute(&mut *tx)
        .await?;

    let row = sqlx::query_as!(
        UserRow,
        "INSERT INTO users (oidc_subject, display_name, email) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (oidc_subject) DO UPDATE \
           SET display_name = EXCLUDED.display_name, \
               email = EXCLUDED.email, \
               updated_at = now() \
         RETURNING id, oidc_subject, display_name, email, \
                   role AS \"role: Role\", is_child, created_at, updated_at, \
                   session_version, theme_preference AS \"theme_preference: ThemePreference\"",
        subject,
        display_name,
        email,
    )
    .fetch_one(&mut *tx)
    .await?;

    // Promote to admin if this is the only user in the table.
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role, updated_at = now() \
         WHERE id = $1 AND (SELECT count(*) FROM users) = 1",
        row.id,
    )
    .execute(&mut *tx)
    .await?;

    // Re-fetch to get potentially updated role
    let row = sqlx::query_as!(
        UserRow,
        "SELECT id, oidc_subject, display_name, email, \
                role AS \"role: Role\", is_child, created_at, updated_at, \
                session_version, theme_preference AS \"theme_preference: ThemePreference\" \
         FROM users WHERE id = $1",
        row.id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(User::from(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_creates_and_updates_user(pool: PgPool) {
        let subject = format!("test-subject-{}", Uuid::new_v4());
        let user =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Alice", Some("alice@example.com"))
                .await
                .expect("upsert");
        assert_eq!(user.display_name, "Alice");
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
        // First user in a fresh DB is auto-promoted to admin.
        assert_eq!(user.role, Role::Admin);
        assert_eq!(user.session_version, 0);
        assert_eq!(user.session_version_bytes, 0_i32.to_le_bytes());

        let updated = upsert_from_oidc_and_maybe_promote(
            &pool,
            &subject,
            "Alice B",
            Some("alice-b@example.com"),
        )
        .await
        .expect("upsert update");
        assert_eq!(updated.id, user.id);
        assert_eq!(updated.display_name, "Alice B");

        let found = find_by_id(&pool, user.id).await.expect("find").unwrap();
        assert_eq!(found.oidc_subject, subject);

        let found = find_by_oidc_subject(&pool, &subject)
            .await
            .expect("find by subject")
            .unwrap();
        assert_eq!(found.id, user.id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_drops_malformed_oidc_email_to_none(pool: PgPool) {
        // UNK-309: the OIDC email claim is signature-verified but not
        // format-checked upstream. A non-standard IdP releasing a malformed
        // value must not land it in `users.email` (the RFC-5322 invariant the
        // admin PATCH path already enforces). Per OIDC Core §5.7 email is an
        // optional, non-identifying claim, so we degrade to NULL rather than
        // failing login — identity still resolves via `sub`.
        let subject = format!("malformed-email-{}", Uuid::new_v4());
        let user =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Mallory", Some("not-an-email"))
                .await
                .expect("upsert");
        assert_eq!(
            user.email, None,
            "malformed email claim must persist as NULL"
        );
        assert_eq!(user.oidc_subject, subject, "identity still keyed on sub");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_treats_empty_oidc_email_as_none(pool: PgPool) {
        // Empty / whitespace-only claim is absence, not a value — store NULL.
        let subject = format!("empty-email-{}", Uuid::new_v4());
        let user = upsert_from_oidc_and_maybe_promote(&pool, &subject, "Eve", Some("   "))
            .await
            .expect("upsert");
        assert_eq!(user.email, None, "empty email claim must persist as NULL");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_persists_valid_oidc_email_trimmed(pool: PgPool) {
        // Valid claim with surrounding whitespace: trimmed and persisted.
        let subject = format!("valid-email-{}", Uuid::new_v4());
        let user =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Val", Some("  val@example.com "))
                .await
                .expect("upsert");
        assert_eq!(user.email.as_deref(), Some("val@example.com"));
    }

    /// Loud-failure regression for UNK-108. Simulates the failure mode
    /// where the DB `user_role` enum gains a value that has no Rust
    /// counterpart (e.g. an operator runs an out-of-band `ALTER TYPE`,
    /// or a future migration lands ahead of the matching Rust change).
    /// `sqlx::Type` must surface this as a decode error, not silently
    /// coerce — that's what makes the typed enum a real authorization
    /// boundary rather than a polite suggestion.
    #[sqlx::test(migrations = "./migrations")]
    async fn role_decode_fails_for_unknown_db_variant(pool: PgPool) {
        // CARVE-OUT (UNK-167): runtime sqlx::query is intentional. The two
        // runtime calls in this test (the ALTER TYPE below and the subsequent
        // UPDATE referencing the new 'superadmin' variant) cannot be expressed
        // as compile-time macros: ALTER TYPE is DDL, and the UPDATE references
        // a variant deliberately not in the prepare-time schema. The whole
        // point of the test is to inject an unknown variant and assert the
        // decode path rejects it.
        sqlx::query("ALTER TYPE user_role ADD VALUE 'superadmin'")
            .execute(&pool)
            .await
            .expect("alter user_role enum");

        let subject = format!("drift-test-{}", Uuid::new_v4());
        let user = upsert_from_oidc_and_maybe_promote(&pool, &subject, "Drift", None)
            .await
            .expect("upsert");

        sqlx::query("UPDATE users SET role = 'superadmin'::user_role WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("inject unknown role");

        let result = find_by_id(&pool, user.id).await;
        assert!(
            result.is_err(),
            "expected sqlx decode error for unknown DB variant, got {result:?}"
        );
    }
}
