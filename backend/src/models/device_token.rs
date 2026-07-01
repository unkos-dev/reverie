//! Per-user device tokens used by OPDS / mobile-client Basic-auth flows.
//!
//! Token verification is implemented in [`crate::auth::token`]; this
//! module owns the row shape, lifecycle queries (create/list/revoke),
//! the per-user cap, and the SQL-side debounce on `last_used_at`.

use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::scope::Scope;

/// A single device-token row. The `token_hash` field is `#[serde(skip)]`
/// so JSON responses never leak the stored hash; verification uses
/// [`crate::auth::token::verify_device_token`] with the hash kept inside
/// this struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeviceToken {
    /// Primary key.
    pub id: Uuid,
    /// Owning [`crate::models::user::User`].
    pub user_id: Uuid,
    /// User-supplied label (e.g. "My Kindle").
    pub name: String,
    /// SHA-256 of the issued token. Never the plaintext.
    #[serde(skip)]
    pub token_hash: String,
    /// `now()` of the last successful auth, written by
    /// [`update_last_used`]; `None` if the token has never been used.
    pub last_used_at: Option<OffsetDateTime>,
    /// Row insert timestamp.
    pub created_at: OffsetDateTime,
    /// `now()` of revocation; `None` while the token is active.
    /// [`list_for_user`] filters on `revoked_at IS NULL`.
    pub revoked_at: Option<OffsetDateTime>,
    /// Capabilities this token carries. Defaults to `{read}` at the DB
    /// level; a token minted after S4 carries the explicit scopes chosen at
    /// mint, bounded by the owner's role ceiling ([`Scope::grantable_by`]).
    pub scopes: Vec<Scope>,
    /// Optional expiry. `None` = never expires. [`find_by_id`] filters out
    /// expired rows.
    pub expires_at: Option<OffsetDateTime>,
}

/// Test-only token insert without the per-user cap.
///
/// Production callers must use [`create_with_limit`]. This helper exists
/// so individual lifecycle tests can pre-populate rows without paying
/// the advisory-lock and count-query cost on every insert.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `INSERT … RETURNING`.
#[cfg(test)]
pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    token_hash: &str,
) -> Result<DeviceToken, sqlx::Error> {
    sqlx::query_as!(
        DeviceToken,
        "INSERT INTO device_tokens (user_id, name, token_hash) \
         VALUES ($1, $2, $3) \
         RETURNING id, user_id, name, token_hash, last_used_at, created_at, revoked_at, \
                   scopes AS \"scopes: Vec<Scope>\", expires_at",
        user_id,
        name,
        token_hash,
    )
    .fetch_one(pool)
    .await
}

/// List active (non-revoked) tokens for a user.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<DeviceToken>, sqlx::Error> {
    sqlx::query_as!(
        DeviceToken,
        "SELECT id, user_id, name, token_hash, last_used_at, created_at, revoked_at, \
                scopes AS \"scopes: Vec<Scope>\", expires_at \
         FROM device_tokens \
         WHERE user_id = $1 AND revoked_at IS NULL \
         ORDER BY created_at DESC",
        user_id,
    )
    .fetch_all(pool)
    .await
}

/// Resolve a token by its primary key: one indexed row, no per-user scan.
/// Filters to active rows (not revoked, not expired) so a caller never needs
/// a second existence check.
///
/// Backs the unified Bearer/Basic credential resolution (both transports
/// converge on this lookup) — see `auth::middleware::verify_basic` /
/// `verify_bearer`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<DeviceToken>, sqlx::Error> {
    sqlx::query_as!(
        DeviceToken,
        "SELECT id, user_id, name, token_hash, last_used_at, created_at, revoked_at, \
                scopes AS \"scopes: Vec<Scope>\", expires_at \
         FROM device_tokens \
         WHERE id = $1 AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())",
        id,
    )
    .fetch_optional(pool)
    .await
}

/// Revoke a token. Scoped to `user_id` to prevent cross-user revocation.
///
/// Returns `true` when the row was newly revoked, `false` when no
/// matching active row existed (already revoked, wrong owner, or
/// unknown id).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn revoke(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE device_tokens SET revoked_at = now() \
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Failure modes of [`create_with_limit`].
#[derive(Debug)]
pub enum CreateError {
    /// User already holds `MAX_TOKENS_PER_USER` active (non-revoked)
    /// tokens. Caller must instruct the user to revoke an existing
    /// token before issuing a new one.
    LimitExceeded,
    /// Underlying database error during the transaction.
    Db(sqlx::Error),
}

const MAX_TOKENS_PER_USER: i64 = 10;

/// Atomically issue a new device token, refusing if the user is already
/// at `MAX_TOKENS_PER_USER` active tokens.
///
/// A per-user `pg_advisory_xact_lock` serializes concurrent calls so the
/// count-then-insert sequence cannot race past the cap; see the
/// implementation comment for why `SELECT … FOR UPDATE` on the
/// active-token rows is insufficient when the count is zero.
///
/// # Errors
///
/// - [`CreateError::LimitExceeded`] when the user is at the cap.
/// - [`CreateError::Db`] for any underlying [`sqlx::Error`] from transaction
///   begin/commit, advisory-lock acquire, count query, or insert.
pub async fn create_with_limit(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    token_hash: &str,
    scopes: &[Scope],
    expires_at: Option<OffsetDateTime>,
) -> Result<DeviceToken, CreateError> {
    let mut tx = pool.begin().await.map_err(CreateError::Db)?;

    // Serialize concurrent create_with_limit calls for this user. The earlier
    // shape (`SELECT ... FOR UPDATE` on the active-tokens result) only locks
    // existing rows; if the user has zero active tokens, the empty result set
    // means N concurrent first-token creates can all pass the count guard
    // and all insert. Per-user advisory lock closes the gap regardless of how
    // many rows already exist.
    let lock_key = user_id.to_string();
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext($1)::bigint)",
        lock_key,
    )
    .execute(&mut *tx)
    .await
    .map_err(CreateError::Db)?;

    // Expiry predicate matches find_by_id's active-row filter: an expired
    // but unrevoked token no longer counts against the cap (S1 fix — the
    // count previously filtered on revoked_at only, so an expired token
    // permanently ate a cap slot).
    let count = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM device_tokens \
         WHERE user_id = $1 AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())",
        user_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateError::Db)?;

    if count >= MAX_TOKENS_PER_USER {
        return Err(CreateError::LimitExceeded);
    }

    let dt = sqlx::query_as!(
        DeviceToken,
        "INSERT INTO device_tokens (user_id, name, token_hash, scopes, expires_at) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, user_id, name, token_hash, last_used_at, created_at, revoked_at, \
                   scopes AS \"scopes: Vec<Scope>\", expires_at",
        user_id,
        name,
        token_hash,
        scopes as &[Scope],
        expires_at,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateError::Db)?;

    tx.commit().await.map_err(CreateError::Db)?;
    Ok(dt)
}

/// Update `last_used_at`, debounced SQL-side to at most one UPDATE per token
/// per 5 minutes. The WHERE predicate turns every call into a no-op when a
/// previous update landed within the window — single source of truth, atomic
/// under concurrent requests, no Rust-side policy to unit-test.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn update_last_used(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE device_tokens SET last_used_at = now() \
         WHERE id = $1 \
           AND (last_used_at IS NULL OR last_used_at < now() - interval '5 minutes')",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn create_list_revoke_lifecycle(pool: PgPool) {
        let display_name = format!("token-test-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let token = create(&pool, user_id, "My Kindle", "fake-hash")
            .await
            .expect("create token");
        assert_eq!(token.name, "My Kindle");
        assert!(token.revoked_at.is_none());

        let tokens = list_for_user(&pool, user_id).await.expect("list");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token.id);

        let revoked = revoke(&pool, token.id, user_id).await.expect("revoke");
        assert!(revoked);

        let tokens = list_for_user(&pool, user_id)
            .await
            .expect("list after revoke");
        assert!(tokens.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_for_user_excludes_revoked(pool: PgPool) {
        let display_name = format!("revoke-filter-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let active = create(&pool, user_id, "active", "hash-active")
            .await
            .expect("create active");
        let to_revoke = create(&pool, user_id, "to-revoke", "hash-revoked")
            .await
            .expect("create revoked");
        assert!(revoke(&pool, to_revoke.id, user_id).await.expect("revoke"),);

        let listed = list_for_user(&pool, user_id).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, active.id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_with_limit_returns_limit_exceeded_at_cap(pool: PgPool) {
        let display_name = format!("limit-cap-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let cap = usize::try_from(MAX_TOKENS_PER_USER).expect("MAX_TOKENS_PER_USER fits usize");
        for i in 0..cap {
            create_with_limit(
                &pool,
                user_id,
                &format!("t-{i}"),
                &format!("h-{i}"),
                &[Scope::Read],
                None,
            )
            .await
            .expect("create within limit");
        }

        let result = create_with_limit(
            &pool,
            user_id,
            "overflow",
            "h-overflow",
            &[Scope::Read],
            None,
        )
        .await;
        assert!(
            matches!(result, Err(CreateError::LimitExceeded)),
            "expected LimitExceeded at cap, got {result:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_with_limit_excludes_revoked_from_count(pool: PgPool) {
        let display_name = format!("limit-revoked-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        // Saturate then revoke them all — revoked tokens must not block creation.
        let cap = usize::try_from(MAX_TOKENS_PER_USER).expect("MAX_TOKENS_PER_USER fits usize");
        for i in 0..cap {
            let t = create_with_limit(
                &pool,
                user_id,
                &format!("r-{i}"),
                &format!("rh-{i}"),
                &[Scope::Read],
                None,
            )
            .await
            .expect("create within limit");
            assert!(revoke(&pool, t.id, user_id).await.expect("revoke"));
        }

        let result =
            create_with_limit(&pool, user_id, "active", "h-active", &[Scope::Read], None).await;
        assert!(
            result.is_ok(),
            "revoked tokens must not block creation: {result:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn update_last_used_debounced_within_window(pool: PgPool) {
        let display_name = format!("debounce-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");
        let token = create(&pool, user_id, "debounce", "hash-debounce")
            .await
            .expect("create token");

        update_last_used(&pool, token.id).await.expect("first");
        let first = sqlx::query_scalar!(
            "SELECT last_used_at FROM device_tokens WHERE id = $1",
            token.id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch first");
        let first = first.expect("first last_used_at not null");

        // Sleep 50ms then update again — the SQL predicate should veto the write
        // because last_used_at < now() - interval '5 minutes' is false.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        update_last_used(&pool, token.id).await.expect("second");
        let second = sqlx::query_scalar!(
            "SELECT last_used_at FROM device_tokens WHERE id = $1",
            token.id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch second")
        .expect("second last_used_at not null");
        assert_eq!(
            first, second,
            "second update within 5-minute window must be a no-op"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn new_device_token_defaults_scopes_to_read(pool: PgPool) {
        let display_name = format!("scopes-default-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let token = create(&pool, user_id, "scoped", "hash-scopes")
            .await
            .expect("create token");

        assert_eq!(token.scopes, vec![Scope::Read]);
        assert!(token.expires_at.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_with_limit_roundtrips_scopes_and_expiry(pool: PgPool) {
        let display_name = format!("mint-roundtrip-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let expires_at = OffsetDateTime::now_utc() + Duration::days(90);
        let token = create_with_limit(
            &pool,
            user_id,
            "roundtrip",
            "h-roundtrip",
            &[Scope::Read, Scope::Write],
            Some(expires_at),
        )
        .await
        .expect("create token");

        assert_eq!(token.scopes, vec![Scope::Read, Scope::Write]);
        assert_eq!(
            token.expires_at.map(OffsetDateTime::unix_timestamp),
            Some(expires_at.unix_timestamp())
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_with_limit_excludes_expired_from_count(pool: PgPool) {
        let display_name = format!("limit-expired-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        // Saturate with already-expired tokens. S1 fix regression: the cap
        // count previously filtered on revoked_at only, so an
        // expired-but-unrevoked token permanently ate a slot.
        let cap = usize::try_from(MAX_TOKENS_PER_USER).expect("MAX_TOKENS_PER_USER fits usize");
        let expired = OffsetDateTime::now_utc() - Duration::days(1);
        for i in 0..cap {
            create_with_limit(
                &pool,
                user_id,
                &format!("e-{i}"),
                &format!("eh-{i}"),
                &[Scope::Read],
                Some(expired),
            )
            .await
            .expect("create within limit");
        }

        let result =
            create_with_limit(&pool, user_id, "active", "h-active", &[Scope::Read], None).await;
        assert!(
            result.is_ok(),
            "expired tokens must not block creation: {result:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_id_elides_revoked_and_expired(pool: PgPool) {
        let display_name = format!("find-by-id-{}", Uuid::new_v4());
        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(&pool)
        .await
        .expect("create user");

        let active = create_with_limit(
            &pool,
            user_id,
            "active",
            "h-active",
            &[Scope::Read, Scope::Write],
            None,
        )
        .await
        .expect("create active");
        let found = find_by_id(&pool, active.id)
            .await
            .expect("find active")
            .expect("active token found");
        assert_eq!(found.id, active.id);
        assert_eq!(found.scopes, vec![Scope::Read, Scope::Write]);

        let revoked =
            create_with_limit(&pool, user_id, "revoked", "h-revoked", &[Scope::Read], None)
                .await
                .expect("create revoked");
        assert!(revoke(&pool, revoked.id, user_id).await.expect("revoke"));
        assert!(
            find_by_id(&pool, revoked.id)
                .await
                .expect("find revoked")
                .is_none(),
            "revoked token must not resolve"
        );

        let expired_at = OffsetDateTime::now_utc() - Duration::days(1);
        let expired = create_with_limit(
            &pool,
            user_id,
            "expired",
            "h-expired",
            &[Scope::Read],
            Some(expired_at),
        )
        .await
        .expect("create expired");
        assert!(
            find_by_id(&pool, expired.id)
                .await
                .expect("find expired")
                .is_none(),
            "expired token must not resolve"
        );

        assert!(
            find_by_id(&pool, Uuid::new_v4())
                .await
                .expect("find unknown")
                .is_none(),
            "unknown id must not resolve"
        );
    }
}
