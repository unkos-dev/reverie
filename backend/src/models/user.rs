//! User accounts and OIDC-driven upsert.
//!
//! Two row shapes coexist: a private `UserRow` decoded from the DB and
//! the public [`crate::models::user::User`] returned to callers. Identity is
//! resolved through [`crate::models::user_identities`] keyed on
//! `(issuer, subject)`; `users.oidc_subject` is a vestigial nullable column,
//! no longer the identity key, and OIDC login no longer auto-promotes.

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
    oidc_subject: Option<String>,
    display_name: String,
    email: Option<String>,
    role: Role,
    is_child: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    session_version: i32,
    theme_preference: ThemePreference,
    disabled_at: Option<OffsetDateTime>,
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
    /// Vestigial nullable column. Identity is resolved through
    /// [`crate::models::user_identities`] keyed on `(issuer, subject)`; new
    /// users are provisioned with this `NULL`. Retained, not dropped, so a
    /// future read path can still observe legacy values.
    pub oidc_subject: Option<String>,
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
    /// `Some(ts)` when the account is soft-disabled (disabled at `ts`); `None`
    /// when active. Every auth-resolution path rejects a user with this set, so
    /// a disabled account cannot log in, rehydrate a session, or authenticate a
    /// device token.
    pub disabled_at: Option<OffsetDateTime>,
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
            disabled_at: row.disabled_at,
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
        "SELECT id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                role AS \"role: Role\", is_child, created_at, updated_at, \
                session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                disabled_at \
         FROM users WHERE id = $1",
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(User::from))
}

/// Whether any administrator account exists.
///
/// The bootstrap gate's cheap fast-reject check: a `false` here is
/// the common path that lets first-run setup proceed. It is NOT the race guard
/// on its own; a `SELECT EXISTS` then `INSERT` does not serialize under READ
/// COMMITTED. The authoritative zero->one transition guard is the
/// `instance_bootstrap` singleton insert in the same transaction as the admin.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
#[allow(dead_code)] // Consumed by bootstrap/setup + CLI in this PR
pub async fn admin_exists(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM users WHERE role = 'admin'::user_role) AS "exists!""#
    )
    .fetch_one(pool)
    .await
}

/// Fetch a user by email, compared case-insensitively on `lower(email)` (the
/// `idx_users_email_lower` key). Returns `Ok(None)` if no row matches. Returns
/// the row whether or not a local credential exists for it; the caller decides
/// whether a credential is required.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
#[allow(dead_code)] // Consumed by local login + recovery in this PR
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                role AS \"role: Role\", is_child, created_at, updated_at, \
                session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                disabled_at \
         FROM users WHERE lower(email) = lower($1)",
        email,
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(User::from))
}

/// Increment a user's `session_version`, invalidating all of their existing
/// sessions. This is the force-logout lever: the auth middleware rejects any
/// session whose stored version is stale.
///
/// Takes an executor so the caller can bind it to a transaction (e.g. the
/// password-reset flow bumps the version in the same transaction that writes
/// the new credential) or run it standalone against a pool.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE`.
#[allow(dead_code)] // Consumed by the password-reset route in this PR
pub async fn increment_session_version(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET session_version = session_version + 1, updated_at = now() WHERE id = $1",
        user_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Typed outcome of the first-admin bootstrap transaction.
#[derive(Debug)]
pub enum BootstrapError {
    /// The `instance_bootstrap` singleton already exists: an administrator was
    /// minted by a prior or concurrent bootstrap. Maps to HTTP 409.
    AlreadyBootstrapped,
    /// The email collides with an existing account (`idx_users_email_lower`).
    EmailTaken,
    /// Any other database failure.
    Db(sqlx::Error),
}

/// Atomically mint the first administrator: insert the `instance_bootstrap`
/// singleton marker, the admin `users` row, and its local credential in one
/// transaction.
///
/// THREAT (TOCTOU, CWE-367): the singleton insert is the DB-enforced zero->one
/// gate, not an app-layer `admin_exists` re-check, because three
/// writers can mint the first admin (HTTP setup, CLI bootstrap, env-seed) and a
/// `SELECT EXISTS ... INSERT` does not serialize under READ COMMITTED. Inserting
/// the marker FIRST means a second concurrent bootstrap collides on its primary
/// key and the whole transaction aborts, so exactly one admin can ever be the
/// first. It is a one-shot transition guard, not a permanent uniqueness rule
/// (multiple admins are allowed later).
///
/// `password_hash` is an Argon2id PHC ([`crate::auth::password`]); this never
/// sees a clear password.
///
/// # Errors
///
/// [`BootstrapError::AlreadyBootstrapped`] when the marker already exists,
/// [`BootstrapError::EmailTaken`] on an email collision, or
/// [`BootstrapError::Db`] for any other failure.
pub async fn create_first_admin(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password_hash: &str,
) -> Result<User, BootstrapError> {
    let mut tx = pool.begin().await.map_err(BootstrapError::Db)?;

    // Gate first: the singleton marker. A second concurrent insert of the single
    // `true` row collides on the PK and aborts the whole transaction.
    if let Err(e) = sqlx::query!("INSERT INTO instance_bootstrap (id) VALUES (true)")
        .execute(&mut *tx)
        .await
    {
        return Err(match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                BootstrapError::AlreadyBootstrapped
            }
            _ => BootstrapError::Db(e),
        });
    }

    let row = match sqlx::query_as!(
        UserRow,
        "INSERT INTO users (display_name, email, role) \
         VALUES ($1, $2, 'admin'::user_role) \
         RETURNING id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                   role AS \"role: Role\", is_child, created_at, updated_at, \
                   session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                   disabled_at",
        display_name,
        email,
    )
    .fetch_one(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            return Err(match &e {
                sqlx::Error::Database(db) if db.is_unique_violation() => BootstrapError::EmailTaken,
                _ => BootstrapError::Db(e),
            });
        }
    };

    sqlx::query!(
        "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, $2)",
        row.id,
        password_hash,
    )
    .execute(&mut *tx)
    .await
    .map_err(BootstrapError::Db)?;

    tx.commit().await.map_err(BootstrapError::Db)?;
    Ok(User::from(row))
}

/// Typed outcome of a local-account create.
#[derive(Debug)]
pub enum CreateUserError {
    /// The email collides with an existing account (`idx_users_email_lower`).
    /// Maps to HTTP 409.
    EmailExists,
    /// Any other database failure.
    Db(sqlx::Error),
}

/// Create a local (non-bootstrap) account, optionally with a password.
///
/// Mirrors [`create_first_admin`] without the `instance_bootstrap` singleton
/// marker: this path is for accounts minted after the instance is already
/// bootstrapped (admin create/invite and self-registration). When
/// `password_hash` is `Some`, the `local_credentials` row is inserted in the
/// same transaction so an account and its credential are atomic. `is_child` is
/// derived from `role` to satisfy the `chk_child_role_sync` constraint.
///
/// `password_hash`, when present, is an Argon2id PHC ([`crate::auth::password`]);
/// this never sees a clear password.
///
/// # Errors
///
/// [`CreateUserError::EmailExists`] on an email collision, or
/// [`CreateUserError::Db`] for any other failure.
#[allow(dead_code)] // Consumed by the admin-create + register routes in this PR
pub async fn create_local(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    role: Role,
    password_hash: Option<&str>,
) -> Result<User, CreateUserError> {
    let is_child = role == Role::Child;
    let mut tx = pool.begin().await.map_err(CreateUserError::Db)?;

    let row = match sqlx::query_as!(
        UserRow,
        "INSERT INTO users (display_name, email, role, is_child) \
         VALUES ($1, $2, ($3::text)::user_role, $4) \
         RETURNING id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                   role AS \"role: Role\", is_child, created_at, updated_at, \
                   session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                   disabled_at",
        display_name,
        email,
        role.as_str(),
        is_child,
    )
    .fetch_one(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            return Err(match &e {
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    CreateUserError::EmailExists
                }
                _ => CreateUserError::Db(e),
            });
        }
    };

    if let Some(phc) = password_hash {
        sqlx::query!(
            "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, $2)",
            row.id,
            phc,
        )
        .execute(&mut *tx)
        .await
        .map_err(CreateUserError::Db)?;
    }

    tx.commit().await.map_err(CreateUserError::Db)?;
    Ok(User::from(row))
}

/// Soft-disable an account.
///
/// Stamps `disabled_at = now()` AND bumps `session_version` in one statement so
/// every live session for the target is invalidated immediately (the
/// force-logout lever).
///
/// Takes an executor so the caller binds it to the same transaction as its
/// last-enabled-admin guard (the account-status handler).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE`.
#[allow(dead_code)] // Consumed by the account-status route in this PR
pub async fn disable_account(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users \
         SET disabled_at = now(), session_version = session_version + 1, updated_at = now() \
         WHERE id = $1",
        user_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Re-enable a soft-disabled account.
///
/// Clears `disabled_at`; it does not bump `session_version`, since a disabled
/// account holds no live sessions to preserve.
///
/// Takes an executor so the caller binds it to the same transaction as its
/// last-enabled-admin guard (the account-status handler).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE`.
#[allow(dead_code)] // Consumed by the account-status route in this PR
pub async fn enable_account(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET disabled_at = NULL, updated_at = now() WHERE id = $1",
        user_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Fetch a user by OIDC identity `(issuer, subject)`, resolved through
/// [`crate::models::user_identities`]. Returns `Ok(None)` if no link exists.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the identity lookup or the user `SELECT`.
pub async fn find_by_oidc_identity(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
) -> Result<Option<User>, sqlx::Error> {
    let Some(user_id) =
        crate::models::user_identities::find_user_id_by_oidc(pool, issuer, subject).await?
    else {
        return Ok(None);
    };
    find_by_id(pool, user_id).await
}

/// Whether `e` parses as an RFC-5322 *addr-spec* with display-name and
/// domain-literal forms disallowed.
///
/// THREAT: [`EmailAddress::is_valid`] uses [`Options::default`], which sets both
/// `allow_display_text` and `allow_domain_literal` to `true` — it accepts
/// display-name (`Name <a@b>`) and domain-literal (`a@[127.0.0.1]`) forms and
/// would store those angle/bracket-bearing strings raw in `users.email`. This
/// helper disables both options, so every write path to that column (OIDC upsert
/// + admin `PATCH /api/v1/users/{id}`) rejects those two shapes.
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

/// Insert or update a user from verified OIDC claims, resolving identity
/// through [`crate::models::user_identities`] keyed on `(issuer, subject)`.
/// Does NOT promote: a fresh instance's first login is a non-administrator
/// (the first administrator is granted only through bootstrap). Identity
/// resolution and the two-table create run in a single transaction so
/// concurrent same-identity logins cannot orphan a `users` row.
///
/// THREAT: `email` carries a case-insensitive uniqueness constraint
/// (`idx_users_email_lower`); login identity rides on the verified
/// `(issuer, subject)` resolved via `user_identities`, never on `email`. The
/// OIDC JWT is signature-verified, but the `email` *string* is not
/// format-checked upstream, so a misconfigured or non-standard `IdP` could
/// release a malformed value that violates the column invariant. Both write
/// paths to this column (here and the admin `PATCH /api/v1/users/{id}` path)
/// guard with [`is_addr_spec`], so both uphold the RFC-5322 addr-spec
/// invariant. Per OIDC Core §5.7 the `email` claim is optional and
/// non-identifying, so an invalid claim degrades to `NULL` rather than failing
/// authentication; login never depends on an optional claim's format. On a
/// returning user the `UPDATE … SET email = $email` overwrites a
/// previously-stored valid email with `NULL` when the `IdP` later emits a
/// malformed claim (Option B: never persist a junk value in the column);
/// identity is preserved because resolution keys on `(issuer, subject)`. The
/// rejected value is logged by shape only (length), never verbatim (n 7);
/// the `had_prior_email` log field lets operators distinguish that
/// overwrite from a first-login with a malformed claim.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from any step of the transaction (advisory lock,
/// identity resolution, `INSERT`/`UPDATE`, identity-link insert, re-fetch, or
/// commit). A concurrent first-login losing the race to the
/// `UNIQUE (issuer, subject)` backstop is serialised by the advisory lock and
/// resolves to the same row rather than erroring.
pub async fn upsert_from_oidc(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<User, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Serialize concurrent provisioning of the same identity. Replacing the
    // atomic single-table `ON CONFLICT (oidc_subject)` upsert with a two-table
    // create (users + user_identities) reopens a race: two concurrent first
    // logins for one identity could both miss the resolve and both insert. The
    // per-identity advisory lock makes the second wait for the first to commit;
    // `UNIQUE (issuer, subject)` on user_identities is the backstop. Keyed on
    // the identity, not the fixed `42` the retired promotion count used.
    //
    // `hashtext` is a 32-bit hash widened to bigint, so the lock keyspace is
    // ~2^32: two distinct identities can collide and serialize against each
    // other. That is benign (a false-positive wait, never a wrong row) because
    // correctness rests on the UNIQUE backstop, not the lock. The post-lock
    // resolve below also assumes READ COMMITTED (the pool default) so it sees a
    // concurrent committed insert; under a stricter isolation level it would
    // miss it and fall through to the UNIQUE violation instead.
    let lock_key = format!("{issuer}|{subject}");
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext($1)::bigint)",
        lock_key
    )
    .execute(&mut *tx)
    .await?;

    // Validate the OIDC email claim before persisting. Trim first so a
    // whitespace-only claim reads as absence; degrade a non-empty-but-invalid
    // value to NULL with a shape-only warning. Runs inside the transaction and
    // under the advisory lock so the `had_prior_email` diagnostic reflects the
    // same serialized state the upsert below acts on.
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
            // user the UPDATE path overwrites a previously-stored valid email, so
            // surface `had_prior_email` to distinguish an IdP misconfiguration
            // wiping a known-good value from a first-login carrying junk. Resolve
            // the prior-email check through `user_identities` (the identity key),
            // since `users.oidc_subject` is no longer populated for new users.
            let had_prior_email = sqlx::query_scalar!(
                "SELECT u.email IS NOT NULL AS \"had_email!\" \
                 FROM users u \
                 JOIN user_identities ui ON ui.user_id = u.id \
                 WHERE ui.issuer = $1 AND ui.subject = $2",
                issuer,
                subject,
            )
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(false);
            tracing::warn!(
                oidc_subject = %subject,
                rejected_email_len = e.len(),
                had_prior_email,
                "OIDC email claim is not RFC-5322 valid; persisting NULL and matching on sub"
            );
            None
        }
        None => None,
    };

    let row = if let Some(user_id) =
        crate::models::user_identities::find_user_id_by_oidc(&mut *tx, issuer, subject).await?
    {
        // Returning identity: refresh the canonical user's mutable fields.
        sqlx::query_as!(
            UserRow,
            "UPDATE users \
             SET display_name = $2, email = $3, updated_at = now() \
             WHERE id = $1 \
             RETURNING id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                       role AS \"role: Role\", is_child, created_at, updated_at, \
                       session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                       disabled_at",
            user_id,
            display_name,
            email,
        )
        .fetch_one(&mut *tx)
        .await?
    } else {
        // First login for this identity: create the canonical user (no
        // oidc_subject — identity lives in user_identities) and the link.
        let user_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO users (display_name, email) VALUES ($1, $2) RETURNING id",
            display_name,
            email,
        )
        .fetch_one(&mut *tx)
        .await?;

        // Per-identity verification state is added with its write path in a
        // later slice; the link carries no verification column yet.
        crate::models::user_identities::insert_oidc(&mut *tx, user_id, issuer, subject).await?;

        sqlx::query_as!(
            UserRow,
            "SELECT id, oidc_subject AS \"oidc_subject?\", display_name, email, \
                    role AS \"role: Role\", is_child, created_at, updated_at, \
                    session_version, theme_preference AS \"theme_preference: ThemePreference\", \
                    disabled_at \
             FROM users WHERE id = $1",
            user_id,
        )
        .fetch_one(&mut *tx)
        .await?
    };

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
        // The default `EmailAddress::is_valid` would accept these
        // display-name and domain-literal forms and store the bracket/angle/
        // space-bearing string raw. `is_addr_spec` rejects them so the column
        // only ever holds a plain `local@domain`.
        assert!(!is_addr_spec("Bob <bob@example.com>"));
        assert!(!is_addr_spec("bob@[127.0.0.1]"));
        assert!(!is_addr_spec("not-an-email"));
    }

    const TEST_ISSUER: &str = "https://test-issuer.example.com";

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_creates_and_updates_user(pool: PgPool) {
        let subject = format!("test-subject-{}", Uuid::new_v4());
        let user = upsert_from_oidc(
            &pool,
            TEST_ISSUER,
            &subject,
            "Alice",
            Some("alice@example.com"),
        )
        .await
        .expect("upsert");
        assert_eq!(user.display_name, "Alice");
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
        // A first login on a fresh instance is NOT auto-promoted; the user
        // takes the default non-administrator role.
        assert_ne!(user.role, Role::Admin);
        assert_eq!(user.role, Role::Adult);
        assert_eq!(user.session_version, 0);
        // The canonical user carries no oidc_subject; identity lives in
        // user_identities.
        assert_eq!(user.oidc_subject, None);

        let updated = upsert_from_oidc(
            &pool,
            TEST_ISSUER,
            &subject,
            "Alice B",
            Some("alice-b@example.com"),
        )
        .await
        .expect("upsert update");
        // Same (issuer, subject) resolves to the same row, fields updated.
        assert_eq!(updated.id, user.id);
        assert_eq!(updated.display_name, "Alice B");

        let found = find_by_id(&pool, user.id).await.expect("find").unwrap();
        assert_eq!(found.oidc_subject, None);

        // Identity resolves through user_identities, not the vestigial column.
        let found = find_by_oidc_identity(&pool, TEST_ISSUER, &subject)
            .await
            .expect("find by identity")
            .unwrap();
        assert_eq!(found.id, user.id);

        // Exactly one identity link and one users row for this identity.
        let identity_count = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM user_identities WHERE issuer = $1 AND subject = $2",
            TEST_ISSUER,
            subject,
        )
        .fetch_one(&pool)
        .await
        .expect("count identities");
        assert_eq!(identity_count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn first_oidc_login_is_not_admin(pool: PgPool) {
        // Explicit: neither the first login nor any subsequent login is
        // auto-promoted. Admin is granted only via bootstrap.
        let first = upsert_from_oidc(&pool, TEST_ISSUER, "subject-one", "First", None)
            .await
            .expect("first login");
        assert_ne!(
            first.role,
            Role::Admin,
            "first OIDC login must not be admin"
        );

        let second = upsert_from_oidc(&pool, TEST_ISSUER, "subject-two", "Second", None)
            .await
            .expect("second login");
        assert_ne!(second.role, Role::Admin);
        assert_ne!(first.id, second.id, "distinct subjects are distinct users");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn same_subject_distinct_issuers_are_distinct_users(pool: PgPool) {
        // The identity key is (issuer, subject): one subject string asserted by
        // two issuers is two independent users, never collapsed. upsert_from_oidc
        // must honour that, not only the user_identities insert path.
        let subject = format!("multi-issuer-{}", Uuid::new_v4());
        let issuer_a = "https://issuer-a.example.com";
        let issuer_b = "https://issuer-b.example.com";

        let a = upsert_from_oidc(&pool, issuer_a, &subject, "A", None)
            .await
            .expect("upsert under issuer a");
        let b = upsert_from_oidc(&pool, issuer_b, &subject, "B", None)
            .await
            .expect("upsert under issuer b");
        assert_ne!(
            a.id, b.id,
            "same subject under distinct issuers must be distinct users"
        );

        // Each identity link resolves independently to its own user.
        let resolved_a = find_by_oidc_identity(&pool, issuer_a, &subject)
            .await
            .expect("resolve a")
            .expect("a present");
        let resolved_b = find_by_oidc_identity(&pool, issuer_b, &subject)
            .await
            .expect("resolve b")
            .expect("b present");
        assert_eq!(resolved_a.id, a.id);
        assert_eq!(resolved_b.id, b.id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_oidc_identity_absent_resolves_to_none(pool: PgPool) {
        // The public resolver returns Ok(None) for an identity that was never
        // provisioned, distinct from surfacing an error.
        let resolved = find_by_oidc_identity(&pool, TEST_ISSUER, "never-provisioned")
            .await
            .expect("no db error");
        assert!(resolved.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn first_login_with_duplicate_email_collides(pool: PgPool) {
        // The migration documents that on an existing pre-release DB a first
        // login whose IdP releases an email already held by another user
        // collides on idx_users_email_lower and fails. Reproduce that shape: two
        // distinct identities carrying the same email; the second is a first
        // login whose users INSERT violates the case-insensitive email index.
        let email = "dupe@example.com";
        upsert_from_oidc(&pool, TEST_ISSUER, "collide-a", "A", Some(email))
            .await
            .expect("first identity provisions");

        let err = upsert_from_oidc(&pool, TEST_ISSUER, "collide-b", "B", Some(email))
            .await
            .expect_err("duplicate email on a first login must fail");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("idx_users_email_lower"),
            "collision must be the case-insensitive email index"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_same_identity_resolves_to_one_user(pool: PgPool) {
        // The two-table create lost the atomic single-table upsert; the
        // per-identity advisory lock plus the UNIQUE (issuer, subject) backstop
        // must keep concurrent same-identity provisioning to a single users row.
        let subject = format!("race-{}", Uuid::new_v4());
        let (r1, r2) = tokio::join!(
            upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Race A", None),
            upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Race B", None),
        );
        let u1 = r1.expect("first concurrent upsert");
        let u2 = r2.expect("second concurrent upsert");
        assert_eq!(
            u1.id, u2.id,
            "concurrent same-identity logins resolve to one user"
        );

        let identity_count = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM user_identities WHERE issuer = $1 AND subject = $2",
            TEST_ISSUER,
            subject,
        )
        .fetch_one(&pool)
        .await
        .expect("count identities");
        assert_eq!(identity_count, 1, "no orphaned identity link");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn nullable_subject_with_local_credential_decodes(pool: PgPool) {
        // A credential-only user (NULL oidc_subject + a local_credentials row)
        // is representable and decodes through the Option<String> column.
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ('Credential Only') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert user");
        sqlx::query!(
            "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, $2)",
            user_id,
            "$argon2id$v=19$m=19456,t=2,p=1$fake$fakehash",
        )
        .execute(&pool)
        .await
        .expect("insert credential");

        let user = find_by_id(&pool, user_id)
            .await
            .expect("find")
            .expect("user present");
        assert_eq!(user.oidc_subject, None, "NULL oidc_subject decodes as None");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn child_role_sync_constraint_still_enforced(pool: PgPool) {
        // The migration is column-only on device_tokens and leaves the users
        // CHECK intact: setting role=child without is_child=true is rejected.
        let subject = format!("child-sync-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Sync", None)
            .await
            .expect("upsert");
        let err = sqlx::query!(
            "UPDATE users SET role = 'child'::user_role WHERE id = $1",
            user.id,
        )
        .execute(&pool)
        .await
        .expect_err("role=child without is_child must violate chk_child_role_sync");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("chk_child_role_sync"),
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_drops_malformed_oidc_email_to_none(pool: PgPool) {
        // The OIDC email claim is signature-verified but not
        // format-checked upstream. A non-standard IdP releasing a malformed
        // value must not land it in `users.email` (the RFC-5322 invariant the
        // admin PATCH path already enforces). Per OIDC Core §5.7 email is an
        // optional, non-identifying claim, so we degrade to NULL rather than
        // failing login — identity still resolves via `sub`.
        let subject = format!("malformed-email-{}", Uuid::new_v4());
        let user = upsert_from_oidc(
            &pool,
            TEST_ISSUER,
            &subject,
            "Mallory",
            Some("not-an-email"),
        )
        .await
        .expect("upsert");
        assert_eq!(
            user.email, None,
            "malformed email claim must persist as NULL"
        );
        // Identity resolves through user_identities; the column stays NULL.
        assert_eq!(user.oidc_subject, None);
        let resolved = find_by_oidc_identity(&pool, TEST_ISSUER, &subject)
            .await
            .expect("resolve")
            .expect("user present");
        assert_eq!(
            resolved.id, user.id,
            "identity still resolves via (issuer, subject)"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_treats_empty_oidc_email_as_none(pool: PgPool) {
        // Empty / whitespace-only claim is absence, not a value — store NULL.
        let subject = format!("empty-email-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Eve", Some("   "))
            .await
            .expect("upsert");
        assert_eq!(user.email, None, "empty email claim must persist as NULL");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_persists_valid_oidc_email_trimmed(pool: PgPool) {
        // Valid claim with surrounding whitespace: trimmed and persisted.
        let subject = format!("valid-email-{}", Uuid::new_v4());
        let user = upsert_from_oidc(
            &pool,
            TEST_ISSUER,
            &subject,
            "Val",
            Some("  val@example.com "),
        )
        .await
        .expect("upsert");
        assert_eq!(user.email.as_deref(), Some("val@example.com"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_overwrites_valid_email_to_none_when_claim_becomes_malformed(pool: PgPool) {
        // Returning-user consequence: the upsert updates the resolved row's
        // email, so a returning user whose IdP later emits a malformed claim
        // has their previously-stored valid email overwritten to NULL.
        // Consistent with Option-B (never persist junk in the column); identity
        // is preserved because resolution keys on `(issuer, subject)`, not email.
        let subject = format!("email-overwrite-{}", Uuid::new_v4());
        let first = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Bob", Some("bob@example.com"))
            .await
            .expect("first upsert");
        assert_eq!(first.email.as_deref(), Some("bob@example.com"));

        let second = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Bob", Some("not-an-email"))
            .await
            .expect("second upsert");
        assert_eq!(
            second.email, None,
            "malformed claim on re-login must overwrite the previously valid email to NULL"
        );
        assert_eq!(second.id, first.id, "same row updated, not a new insert");
        assert_eq!(
            second.oidc_subject, None,
            "oidc_subject stays vestigial NULL"
        );
    }

    /// Loud-failure regression for role-enum drift. Simulates the failure mode
    /// where the DB `user_role` enum gains a value that has no Rust
    /// counterpart (e.g. an operator runs an out-of-band `ALTER TYPE`,
    /// or a future migration lands ahead of the matching Rust change).
    /// `sqlx::Type` must surface this as a decode error, not silently
    /// coerce — that's what makes the typed enum a real authorization
    /// boundary rather than a polite suggestion.
    #[sqlx::test(migrations = "./migrations")]
    async fn role_decode_fails_for_unknown_db_variant(pool: PgPool) {
        // CARVE-OUT: runtime sqlx::query is intentional. The two
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
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Drift", None)
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

    // Migration 20260604120000 adds CHECK (session_version >= 0). A negative
    // value must be rejected at the schema layer so the force-logout counter
    // can never be reset below an already-issued version (which would revive
    // sessions force-logout had invalidated).
    #[sqlx::test(migrations = "./migrations")]
    async fn session_version_check_rejects_negative(pool: PgPool) {
        let subject = format!("check-subject-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "Check", None)
            .await
            .expect("create user");

        let err = sqlx::query!(
            "UPDATE users SET session_version = -1 WHERE id = $1",
            user.id
        )
        .execute(&pool)
        .await
        .expect_err("negative session_version must violate the CHECK constraint");

        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("users_session_version_nonneg"),
            "violation must be the session_version CHECK: {err}"
        );
    }

    // Migration 20260610165400 adds decode-range CHECK constraints on TIMESTAMPTZ
    // columns: `time` without `large-dates` only decodes years -9999..=9999,
    // so a year-10000+ row (out-of-band mutation only) would panic at
    // `row.get::<OffsetDateTime>` on every read path. The write must be
    // rejected at the schema boundary instead.
    #[sqlx::test(migrations = "./migrations")]
    async fn timestamptz_check_rejects_beyond_decode_upper_bound(pool: PgPool) {
        let subject = format!("ts-upper-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "TsUpper", None)
            .await
            .expect("create user");

        let err = sqlx::query(
            "UPDATE users SET created_at = TIMESTAMPTZ '10000-01-01 00:00:00+00' WHERE id = $1",
        )
        .bind(user.id)
        .execute(&pool)
        .await
        .expect_err("year 10000 must violate the decode-range CHECK");

        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("users_created_at_ts_decode_range"),
            "violation must be the created_at decode-range CHECK: {err}"
        );
    }

    // `infinity` is undecodable like any year-10000+ value; the finite
    // upper bound rejects the special value too.
    #[sqlx::test(migrations = "./migrations")]
    async fn timestamptz_check_rejects_positive_infinity(pool: PgPool) {
        let subject = format!("ts-posinf-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "TsPosInf", None)
            .await
            .expect("create user");

        let err = sqlx::query("UPDATE users SET created_at = TIMESTAMPTZ 'infinity' WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .expect_err("infinity must violate the decode-range CHECK");

        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("users_created_at_ts_decode_range"),
            "violation must be the created_at decode-range CHECK: {err}"
        );
    }

    // `-infinity` (and any pre-CE date) is equally undecodable; the finite
    // lower bound rejects it at write time.
    #[sqlx::test(migrations = "./migrations")]
    async fn timestamptz_check_rejects_negative_infinity(pool: PgPool) {
        let subject = format!("ts-lower-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "TsLower", None)
            .await
            .expect("create user");

        let err =
            sqlx::query("UPDATE users SET created_at = TIMESTAMPTZ '-infinity' WHERE id = $1")
                .bind(user.id)
                .execute(&pool)
                .await
                .expect_err("-infinity must violate the decode-range CHECK");

        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("users_created_at_ts_decode_range"),
            "violation must be the created_at decode-range CHECK: {err}"
        );
    }

    // The full decodable range stays writable: year 9999 passes the CHECK
    // and round-trips through OffsetDateTime decode.
    #[sqlx::test(migrations = "./migrations")]
    async fn timestamptz_check_accepts_max_decodable_year(pool: PgPool) {
        let subject = format!("ts-max-{}", Uuid::new_v4());
        let user = upsert_from_oidc(&pool, TEST_ISSUER, &subject, "TsMax", None)
            .await
            .expect("create user");

        sqlx::query(
            "UPDATE users SET created_at = TIMESTAMPTZ '9999-12-31 23:59:59+00' WHERE id = $1",
        )
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("max decodable year must pass the CHECK");

        let reloaded = find_by_id(&pool, user.id)
            .await
            .expect("decode of max in-range created_at must succeed")
            .expect("user still present");
        assert_eq!(reloaded.created_at.year(), 9999);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn first_admin_created_then_second_is_rejected(pool: PgPool) {
        let phc = crate::auth::password::hash_password(b"a strong password").expect("hash");
        let admin = create_first_admin(&pool, "admin@example.com", "Admin", &phc)
            .await
            .expect("first admin");
        assert!(matches!(admin.role, Role::Admin), "first user is an admin");

        let second = create_first_admin(&pool, "other@example.com", "Other", &phc).await;
        assert!(
            matches!(second, Err(BootstrapError::AlreadyBootstrapped)),
            "a second bootstrap is rejected once the marker exists"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_first_admin_yields_exactly_one(pool: PgPool) {
        // Single-admin invariant: the instance_bootstrap singleton serializes
        // the zero->one transition. Two overlapping bootstraps must yield exactly
        // one administrator, not two.
        let phc = crate::auth::password::hash_password(b"a strong password").expect("hash");
        let (p1, p2, h1, h2) = (pool.clone(), pool.clone(), phc.clone(), phc);
        let (r1, r2) = tokio::join!(
            async move { create_first_admin(&p1, "a@example.com", "Admin A", &h1).await },
            async move { create_first_admin(&p2, "b@example.com", "Admin B", &h2).await },
        );

        let wins = [r1.is_ok(), r2.is_ok()].into_iter().filter(|&b| b).count();
        assert_eq!(wins, 1, "exactly one concurrent bootstrap succeeds");
        assert!(
            matches!(
                (&r1, &r2),
                (Ok(_), Err(BootstrapError::AlreadyBootstrapped))
                    | (Err(BootstrapError::AlreadyBootstrapped), Ok(_))
            ),
            "the losing bootstrap is rejected as AlreadyBootstrapped"
        );

        let admin_count: Option<i64> =
            sqlx::query_scalar!(r#"SELECT COUNT(*) FROM users WHERE role = 'admin'::user_role"#)
                .fetch_one(&pool)
                .await
                .expect("count admins");
        assert_eq!(admin_count, Some(1), "exactly one administrator exists");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_local_creates_adult_with_credential(pool: PgPool) {
        let phc = crate::auth::password::hash_password(b"a strong password").expect("hash");
        let user = create_local(&pool, "adult@example.com", "Adult", Role::Adult, Some(&phc))
            .await
            .expect("create adult");
        assert_eq!(user.role, Role::Adult);
        assert!(!user.is_child);
        assert!(user.disabled_at.is_none());
        let cred = crate::models::local_credentials::find_by_user_id(&pool, user.id)
            .await
            .expect("query credential");
        assert!(
            cred.is_some(),
            "a password create writes a local credential"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_local_child_sets_is_child(pool: PgPool) {
        let user = create_local(&pool, "child@example.com", "Child", Role::Child, None)
            .await
            .expect("create child");
        assert_eq!(user.role, Role::Child);
        assert!(
            user.is_child,
            "child role sets is_child to satisfy chk_child_role_sync"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_local_without_password_writes_no_credential(pool: PgPool) {
        let user = create_local(&pool, "nopass@example.com", "NoPass", Role::Adult, None)
            .await
            .expect("create");
        let cred = crate::models::local_credentials::find_by_user_id(&pool, user.id)
            .await
            .expect("query credential");
        assert!(cred.is_none(), "no password means no credential row");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_local_duplicate_email_is_email_exists(pool: PgPool) {
        create_local(&pool, "dup@example.com", "First", Role::Adult, None)
            .await
            .expect("first create");
        // Case-insensitive collision via idx_users_email_lower.
        let err = create_local(&pool, "DUP@example.com", "Second", Role::Adult, None)
            .await
            .expect_err("duplicate email rejected");
        assert!(matches!(err, CreateUserError::EmailExists));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disable_account_stamps_timestamp_and_bumps_session_version(pool: PgPool) {
        let user = create_local(&pool, "disable@example.com", "Dis", Role::Adult, None)
            .await
            .expect("create");
        assert!(user.disabled_at.is_none());
        let before = user.session_version;

        disable_account(&pool, user.id).await.expect("disable");

        let reloaded = find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("exists");
        assert!(reloaded.disabled_at.is_some(), "disable stamps disabled_at");
        assert_eq!(
            reloaded.session_version,
            before + 1,
            "disable bumps session_version to kill live sessions"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn enable_account_clears_timestamp(pool: PgPool) {
        let user = create_local(&pool, "reenable@example.com", "Re", Role::Adult, None)
            .await
            .expect("create");
        disable_account(&pool, user.id).await.expect("disable");
        enable_account(&pool, user.id).await.expect("re-enable");

        let reloaded = find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("exists");
        assert!(
            reloaded.disabled_at.is_none(),
            "re-enable clears disabled_at"
        );
    }
}
