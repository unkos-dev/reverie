//! User accounts and OIDC-driven upsert / first-user promotion.
//!
//! Two row shapes coexist: a private `UserRow` decoded from the DB and
//! the public [`crate::models::user::User`] returned to callers.

use email_address::{EmailAddress, Options};
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
/// `session_version` is the force-logout lever: bumping `users.session_version`
/// makes every existing session's stored copy stale, which
/// [`crate::auth::middleware::CurrentUser`] rejects on the next request.
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
    /// session for this user; compared per-request by
    /// [`crate::auth::middleware::CurrentUser`].
    pub session_version: i32,
    /// Selected UI theme; see [`ThemePreference`].
    pub theme_preference: ThemePreference,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
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
        }
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

/// Whether `e` parses as an RFC-5322 *addr-spec* with display-name and
/// domain-literal forms disallowed.
///
/// THREAT: [`EmailAddress::is_valid`] uses [`Options::default`], which sets both
/// `allow_display_text` and `allow_domain_literal` to `true` — it accepts
/// display-name (`Name <a@b>`) and domain-literal (`a@[127.0.0.1]`) forms and
/// would store those angle/bracket-bearing strings raw in `users.email`. This
/// helper disables both options, so every write path to that column (OIDC upsert
/// + admin `PATCH /api/users/{id}`) rejects those two shapes (UNK-309).
///
/// This is *not* full normalisation. A quoted local-part
/// (`"john doe"@example.com`) is a valid addr-spec and is still accepted, so the
/// stored value may contain quotes and spaces inside the quoted segment. CR/LF
/// can never appear (qtext/wsp exclude control bytes), so the value is safe to
/// log and store, but downstream consumers must treat it as an opaque address —
/// not assume a bare `local@domain` free of quoting.
pub(crate) fn is_addr_spec(e: &str) -> bool {
    EmailAddress::parse_with_options(
        e,
        Options::default()
            .without_display_text()
            .without_domain_literal(),
    )
    .is_ok()
}

/// Insert or update a user from OIDC claims, then auto-promote to admin if first user.
/// Runs upsert + promotion in a single transaction to prevent race conditions where
/// concurrent first logins result in no admin.
///
/// THREAT: `email` carries a case-insensitive uniqueness constraint
/// (`idx_users_email_lower`); login identity itself rides on `sub`/`oidc_subject`
/// (this upsert keys on `ON CONFLICT (oidc_subject)`), not on `email`. The OIDC JWT
/// is signature-verified, but the `email` *string* is not format-checked upstream,
/// so a misconfigured or non-standard `IdP` could release a malformed value that
/// violates the column invariant. Both write paths to this column (here and the
/// admin `PATCH /api/users/{id}` path) guard with [`is_addr_spec`], so both uphold
/// the RFC-5322 addr-spec invariant (UNK-309). Per
/// OIDC Core §5.7 the `email` claim is optional and non-identifying, so an invalid
/// claim degrades to `NULL` rather than failing authentication — login never depends
/// on an optional claim's format. On a returning user the
/// `ON CONFLICT … DO UPDATE SET email = EXCLUDED.email` clause overwrites a
/// previously-stored valid email with `NULL` when the `IdP` later emits a malformed
/// claim (Option B: never persist a junk value in the column); identity is preserved
/// because the conflict key is `oidc_subject`. The rejected value is logged by shape
/// only (length), never verbatim (Hard Rule 7); the `had_prior_email` log field lets
/// operators distinguish that overwrite from a first-login with a malformed claim.
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
    let email = match email.map(str::trim) {
        Some("") => {
            // Claim present but whitespace-only: absence, not a value. (A truly
            // absent claim — `None` — is the normal optional-claim path per OIDC
            // Core §5.7 and is not logged, to avoid per-login debug noise for IdPs
            // that omit `email`.)
            tracing::debug!(
                oidc_subject = %subject,
                "OIDC email claim is whitespace-only; persisting NULL"
            );
            None
        }
        Some(e) if is_addr_spec(e) => Some(e),
        Some(e) => {
            // Non-empty but not a valid addr-spec: degrade to NULL. On a returning
            // user the ON CONFLICT path overwrites a previously-stored valid email,
            // so surface `had_prior_email` to distinguish an IdP misconfiguration
            // wiping a known-good value from a first-login carrying junk.
            let had_prior_email = sqlx::query_scalar!(
                "SELECT email IS NOT NULL AS \"had_email!\" FROM users WHERE oidc_subject = $1",
                subject,
            )
            .fetch_optional(pool)
            .await?
            .unwrap_or(false);
            tracing::warn!(
                oidc_subject = %subject,
                rejected_email_len = e.len(),
                had_prior_email,
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

    #[test]
    fn is_addr_spec_accepts_plain_address_rejects_display_and_literal_forms() {
        // Bare addr-spec passes.
        assert!(is_addr_spec("user@example.com"));
        // UNK-309: the default `EmailAddress::is_valid` would accept these
        // display-name and domain-literal forms and store the bracket/angle/
        // space-bearing string raw. `is_addr_spec` rejects them so the column
        // only ever holds a plain `local@domain`.
        assert!(!is_addr_spec("Bob <bob@example.com>"));
        assert!(!is_addr_spec("bob@[127.0.0.1]"));
        assert!(!is_addr_spec("not-an-email"));
    }

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

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_overwrites_valid_email_to_none_when_claim_becomes_malformed(pool: PgPool) {
        // UNK-309 conflict-path consequence: the upsert is
        // `ON CONFLICT (oidc_subject) DO UPDATE SET email = EXCLUDED.email`, so a
        // returning user whose IdP later emits a malformed claim has their
        // previously-stored valid email overwritten to NULL. Consistent with
        // Option-B (never persist junk in the column); identity is preserved
        // because matching keys on `sub`, not `email`.
        let subject = format!("email-overwrite-{}", Uuid::new_v4());
        let first =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Bob", Some("bob@example.com"))
                .await
                .expect("first upsert");
        assert_eq!(first.email.as_deref(), Some("bob@example.com"));

        let second =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Bob", Some("not-an-email"))
                .await
                .expect("second upsert");
        assert_eq!(
            second.email, None,
            "malformed claim on re-login must overwrite the previously valid email to NULL"
        );
        assert_eq!(second.id, first.id, "same row updated, not a new insert");
        assert_eq!(second.oidc_subject, subject, "identity preserved on sub");
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
