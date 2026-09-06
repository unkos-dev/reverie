//! Password-reset PIN records: hashed, single-use, short-lived recovery tokens.
//!
//! # Tier 2: security-critical
//!
//! The clear PIN is never stored here; only its Argon2id hash, an expiry, and a
//! consumed marker. A row is single-use (`consumed_at`) and short-lived
//! (`expires_at`). At most one row stays active per user: a new request
//! supersedes prior unconsumed rows, and the `idx_password_reset_pins_active_unique`
//! partial unique index enforces that invariant against concurrent issuance
//! (see the `rotate` function). The struct deliberately does not derive
//! `Serialize` so the hash cannot leak through an API by accident.
//!
//! [`crate::models::password_reset_pin::IssuanceLock`] additionally serializes whole issuances for one user across
//! processes, so the rotation and the publication of the clear PIN that follows
//! it stay one indivisible step; see [`crate::auth::recovery::issue_pin`].

use chrono::{DateTime, Utc};
use sqlx::pool::PoolConnection;
use sqlx::{Acquire, Connection as _, PgConnection, PgPool, Postgres};
use uuid::Uuid;

/// First key of the per-user issuance advisory lock. Advisory locks share one
/// database-wide key space, so the two-key form namespaces the recovery-PIN
/// locks away from any other per-user lock (`device_token` hashes the same user
/// id into the single-key space) that would otherwise collide.
const ISSUANCE_LOCK_NAMESPACE: i32 = 0x5245_4356;

/// Attempts and pause between them when acquiring the per-user issuance lock.
/// The section it guards is two small statements plus one local file write, so
/// this budget is generous for a queue of one and still bounded well inside a
/// request: an issuance that cannot get in hands the slot to the holder rather
/// than waiting behind it.
const ISSUANCE_LOCK_ATTEMPTS: u32 = 20;
const ISSUANCE_LOCK_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// Bound on how long a rotate statement may wait for a conflicting row or index
/// lock. Transaction-local, so it never leaks into the pooled session.
const ROTATE_LOCK_TIMEOUT: &str = "5s";

/// Retries [`rotate`] spends on the single-active race before conceding it. One
/// is enough: the retry's supersede `DELETE` sees the winner's committed row.
const ROTATE_RETRIES: u32 = 1;

/// An active (unconsumed, unexpired) password-reset PIN, as needed to verify a
/// reset attempt. Holds a SECRET (`pin_hash`); not serialisable, never logged.
///
/// `Debug` is implemented by hand to redact `pin_hash`: deriving it would emit
/// the Argon2id PHC through any `?value` tracing span (CWE-532).
#[derive(Clone, sqlx::FromRow)]
pub struct PasswordResetPin {
    /// Primary key, used to [`consume`] the row after a successful verify.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Argon2id PHC of the clear PIN.
    pub pin_hash: String,
    /// When this PIN stops being valid.
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for PasswordResetPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordResetPin")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("pin_hash", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Delete any unconsumed PIN rows for a user so at most one stays live.
///
/// Call
/// before [`insert`] on each new forgot-password request: a re-request
/// invalidates the prior PIN (codeguard #2: at most one active PIN per user).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `DELETE`.
pub async fn supersede_active(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM password_reset_pins WHERE user_id = $1 AND consumed_at IS NULL",
        user_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Insert a new PIN row (hash only) and return its id.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `INSERT`.
pub async fn insert(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO password_reset_pins (user_id, pin_hash, expires_at) \
         VALUES ($1, $2, $3) RETURNING id",
        user_id,
        pin_hash,
        expires_at,
    )
    .fetch_one(executor)
    .await
}

/// Outcome of [`rotate`]: whether this call's freshly generated PIN became the
/// single active row, or a concurrent issuance won the slot first.
///
/// The distinction is load-bearing for the caller: it hashed and generated a
/// clear PIN before calling, and it publishes that clear PIN to the operator
/// recovery channel only when this call actually persisted it. Publishing a PIN
/// that was not stored would leave a recovery code that no database hash
/// verifies, denying the account recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateOutcome {
    /// This call persisted its PIN as the sole active row (carrying its id). The
    /// caller SHOULD publish the clear PIN it generated: it is the live one.
    Issued(Uuid),
    /// A concurrent issuance holds the active slot and this call's PIN was NOT
    /// persisted. The caller MUST NOT publish its clear PIN; the winning
    /// request's PIN is the live one. This is a benign race, not an error.
    RaceLost,
}

/// Outcome of a single [`rotate_once`] attempt against the single-active
/// partial unique index.
#[derive(Clone, Copy)]
enum InsertOutcome {
    /// The freshly hashed PIN was persisted as the user's sole active row.
    Inserted(Uuid),
    /// A concurrent issuance committed the active row between this attempt's
    /// supersede `DELETE` and its `INSERT`, so the `INSERT` hit the
    /// `idx_password_reset_pins_active_unique` index. Nothing was persisted (the
    /// attempt's transaction rolled back); [`next_step`] decides whether
    /// to retry or concede the race.
    LostActiveSlot,
}

/// Exclusive hold on one user's recovery-PIN issuance, backed by a
/// session-level Postgres advisory lock and carrying the connection that holds
/// it so the guarded work runs without a second pool checkout.
///
/// THREAT (a superseded issuer publishing last): the database row and the
/// operator PIN file are two stores that must agree. Committing the row proves
/// only that this issuance held the active slot at commit time, not that it
/// still holds it when the file is written, and the HTTP handler and the
/// `reset-password` CLI are separate processes, so no in-process mutex can
/// order them. Holding this lock across both steps makes the winner of the lock
/// the last writer of the file, so the published PIN is always the one the sole
/// active row hashes.
///
/// The lock is session-scoped rather than transaction-scoped on purpose: a
/// `pg_advisory_xact_lock` is released at `COMMIT`, which is before the file is
/// written, and would leave exactly the gap it is meant to close. Postgres
/// releases a session lock when the session ends, so a process that dies mid
/// section cannot lock an account out of recovery; [`IssuanceLock::release`]
/// closes the connection instead of returning it to the pool if the explicit
/// unlock does not confirm.
pub struct IssuanceLock {
    conn: PoolConnection<Postgres>,
    user_key: String,
}

impl IssuanceLock {
    /// Try to take the issuance lock for `user_id`, retrying briefly while
    /// another issuance holds it. `Ok(None)` means the lock stayed taken for the
    /// whole budget: another issuance is publishing its own PIN, and this caller
    /// MUST NOT rotate or publish, because it would be racing that holder.
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::Error`] if a connection cannot be checked out or the lock
    /// query fails.
    pub async fn try_acquire(pool: &PgPool, user_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let user_key = user_id.to_string();
        let mut conn = pool.acquire().await?;
        for attempt in 0..ISSUANCE_LOCK_ATTEMPTS {
            let acquired = sqlx::query_scalar!(
                r#"SELECT pg_try_advisory_lock($1, hashtext($2)::int) AS "acquired!""#,
                ISSUANCE_LOCK_NAMESPACE,
                user_key,
            )
            .fetch_one(&mut *conn)
            .await?;
            if acquired {
                return Ok(Some(Self { conn, user_key }));
            }
            if attempt + 1 < ISSUANCE_LOCK_ATTEMPTS {
                tokio::time::sleep(ISSUANCE_LOCK_RETRY_INTERVAL).await;
            }
        }
        tracing::debug!("recovery-pin issuance lock stayed held for the whole wait budget");
        Ok(None)
    }

    /// The locked connection, for running the guarded work without checking out
    /// a second connection (the `reset-password` CLI runs a pool of one).
    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.conn
    }

    /// Release the lock. Prefers the explicit unlock so the connection goes back
    /// to the pool reusable; if that does not confirm, the connection is closed
    /// instead, because Postgres drops session advisory locks with the session
    /// and a still-locked connection in the pool would deny that user recovery
    /// for the lifetime of the process.
    pub async fn release(mut self) {
        match sqlx::query_scalar!(
            r#"SELECT pg_advisory_unlock($1, hashtext($2)::int) AS "released!""#,
            ISSUANCE_LOCK_NAMESPACE,
            self.user_key,
        )
        .fetch_one(&mut *self.conn)
        .await
        {
            Ok(true) => return,
            Ok(false) => {
                tracing::error!("recovery-pin issuance lock was not held at release");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to release the recovery-pin issuance lock");
            }
        }
        if let Err(e) = self.conn.detach().close().await {
            tracing::warn!(error = %e, "failed to close the recovery-pin issuance connection");
        }
    }
}

/// Atomically supersede a user's prior active PINs and persist a fresh one,
/// reporting whether this call won the single active slot.
///
/// The
/// [`supersede_active`] delete and the [`insert`] share one transaction, so a
/// failure between them cannot leave the user with no active PIN (codeguard #2:
/// at most one active PIN per user). Hash the PIN before calling so the
/// CPU-bound work stays outside the transaction.
///
/// THREAT (concurrent issuance under READ COMMITTED): the single-active
/// invariant is enforced by the `idx_password_reset_pins_active_unique` partial
/// unique index, not by this transaction alone. Two concurrent forgot-password
/// requests for one account can each run the supersede `DELETE` without seeing
/// the other's uncommitted `INSERT`; without the index both would commit and
/// leave two live PINs. The losing issuer's `INSERT` instead fails with a
/// unique violation once the winner commits. That is a benign race, never a
/// client-visible failure: this function retries the whole rotate once (the
/// retry's `DELETE` now sees and supersedes the winner's committed row).
///
/// THREAT (publishing an unpersisted PIN): if a further concurrent issuer still
/// holds the slot after the retry, this call's PIN was never stored. It returns
/// [`RotateOutcome::RaceLost`] rather than another row's id, so the caller does
/// not publish a clear PIN that no stored hash would verify. The caller returns
/// the same generic success and never a 500 for this path.
///
/// Callers reach this through [`crate::auth::recovery::issue_pin`], which holds
/// the user's [`IssuanceLock`] across both the rotation and the publication of
/// the clear PIN. Under that lock the index race is a backstop rather than the
/// primary defence, and it still resolves issuers that predate the lock.
///
/// # Errors
///
/// Returns [`sqlx::Error`] if the transaction cannot begin, either statement
/// fails for a reason other than the single-active unique violation, or the
/// commit fails. On any failure the transaction rolls back, so the prior active
/// PIN is preserved.
pub async fn rotate(
    conn: &mut PgConnection,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<RotateOutcome, sqlx::Error> {
    let mut retries_left = ROTATE_RETRIES;
    loop {
        let outcome = rotate_once(&mut *conn, user_id, pin_hash, expires_at).await?;
        match next_step(outcome, retries_left) {
            NextStep::Done(outcome) => return Ok(outcome),
            NextStep::Retry => {
                tracing::debug!("recovery-pin rotate lost the single-active race; retrying once");
                retries_left -= 1;
            }
        }
    }
}

/// What [`rotate`] does after one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextStep {
    /// Stop with this outcome.
    Done(RotateOutcome),
    /// Run the whole supersede-then-insert again.
    Retry,
}

/// rotate's bounded single-active-race retry policy, as a pure decision so it is
/// exercised deterministically rather than through a live multi-way race no test
/// can schedule reliably.
///
/// A lost race is benign: retry the whole supersede-then-insert so the retry's
/// `DELETE` supersedes the winner's now-committed row. If a further concurrent
/// issuer still holds the slot once the retries are spent, this call's PIN was
/// never persisted, so the outcome is [`RotateOutcome::RaceLost`]. The caller
/// then withholds its unstorable clear PIN rather than reporting another
/// issuer's row as this call's success.
const fn next_step(outcome: InsertOutcome, retries_left: u32) -> NextStep {
    match outcome {
        InsertOutcome::Inserted(id) => NextStep::Done(RotateOutcome::Issued(id)),
        InsertOutcome::LostActiveSlot if retries_left > 0 => NextStep::Retry,
        InsertOutcome::LostActiveSlot => NextStep::Done(RotateOutcome::RaceLost),
    }
}

/// One transactional supersede-then-insert attempt for [`rotate`].
///
/// THREAT (concurrent issuance under READ COMMITTED): the supersede `DELETE`
/// cannot see a concurrent issuer's uncommitted `INSERT`, so once that issuer
/// commits, this attempt's `INSERT` hits the single-active unique index. That is
/// a benign race: the transaction rolls back (dropped uncommitted) and the
/// attempt reports [`InsertOutcome::LostActiveSlot`] instead of surfacing a
/// client-visible error. Any other database error propagates and rolls back,
/// preserving the prior active PIN.
async fn rotate_once(
    conn: &mut PgConnection,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<InsertOutcome, sqlx::Error> {
    let mut tx = Acquire::begin(&mut *conn).await?;
    // Bound the wait on a conflicting row or index lock so a rotation cannot pin
    // a connection, and the issuance lock it runs under, behind an unrelated
    // transaction indefinitely. Transaction-local, so the pooled session keeps
    // the server default.
    sqlx::query_scalar!(
        r#"SELECT set_config('lock_timeout', $1, true) AS "applied!""#,
        ROTATE_LOCK_TIMEOUT,
    )
    .fetch_one(&mut *tx)
    .await?;
    supersede_active(&mut *tx, user_id).await?;
    match insert(&mut *tx, user_id, pin_hash, expires_at).await {
        Ok(id) => {
            tx.commit().await?;
            Ok(InsertOutcome::Inserted(id))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Ok(InsertOutcome::LostActiveSlot)
        }
        Err(e) => Err(e),
    }
}

/// Fetch the single active (unconsumed, unexpired) PIN for a user, newest first
/// if more than one somehow exists. Returns `Ok(None)` when none is active.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `SELECT`.
pub async fn find_active_by_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<PasswordResetPin>, sqlx::Error> {
    sqlx::query_as!(
        PasswordResetPin,
        "SELECT id, user_id, pin_hash, expires_at FROM password_reset_pins \
         WHERE user_id = $1 AND consumed_at IS NULL AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1",
        user_id,
    )
    .fetch_optional(pool)
    .await
}

/// Mark a PIN consumed, but only if it is still unconsumed and unexpired.
/// Returns `true` iff this call performed the consumption.
///
/// THREAT (single-use under concurrency): the guarded `WHERE consumed_at IS
/// NULL` makes consumption atomic at the row level, so two concurrent resets
/// presenting the same PIN cannot both succeed. The caller MUST treat `false`
/// as a failed reset (the PIN was already used or has expired).
///
/// THREAT (expired-PIN reuse): `expires_at > now()` is re-checked inside the
/// atomic `UPDATE` so a PIN that expires between `find_active_by_user` and
/// `consume` cannot be consumed after expiry.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE`.
pub async fn consume(executor: impl sqlx::PgExecutor<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE password_reset_pins SET consumed_at = now() \
         WHERE id = $1 AND consumed_at IS NULL AND expires_at > now()",
        id,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    async fn insert_user(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!("INSERT INTO users (display_name) VALUES ('PIN Test') RETURNING id")
            .fetch_one(pool)
            .await
            .expect("insert user")
    }

    async fn wait_for_blocked_backend(pool: &PgPool) {
        for _ in 0..200 {
            let blocked = sqlx::query_scalar!(
                r#"SELECT count(*) AS "count!" FROM pg_locks WHERE NOT granted"#
            )
            .fetch_one(pool)
            .await
            .expect("query pg_locks");
            if blocked > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("rotate did not block on the single-active slot within the timeout");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn active_then_consumed_is_inactive(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);
        let id = insert(&pool, user_id, "$argon2id$hash", expires)
            .await
            .expect("insert pin");

        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_some(),
            "freshly inserted PIN is active"
        );
        assert!(
            consume(&pool, id).await.expect("consume"),
            "first consume succeeds"
        );
        assert!(
            !consume(&pool, id).await.expect("second consume"),
            "second consume is a no-op (single-use)"
        );
        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_none(),
            "consumed PIN is no longer active"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn expired_pin_is_not_active(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expired = Utc::now() - TimeDelta::minutes(1);
        insert(&pool, user_id, "$argon2id$hash", expired)
            .await
            .expect("insert pin");
        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_none(),
            "an expired PIN is not active"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn supersede_leaves_at_most_one_active(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);
        insert(&pool, user_id, "$argon2id$first", expires)
            .await
            .expect("first");

        supersede_active(&pool, user_id).await.expect("supersede");
        let second = insert(&pool, user_id, "$argon2id$second", expires)
            .await
            .expect("second");

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("one active remains");
        assert_eq!(
            active.id, second,
            "only the newest PIN is active after supersede"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rotate_replaces_the_active_pin(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);
        let first = insert(&pool, user_id, "$argon2id$first", expires)
            .await
            .expect("seed first pin");

        let mut conn = pool.acquire().await.expect("acquire");
        let second = match rotate(&mut conn, user_id, "$argon2id$second", expires)
            .await
            .expect("rotate")
        {
            RotateOutcome::Issued(id) => id,
            RotateOutcome::RaceLost => panic!("uncontended rotate must issue"),
        };

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("one active remains");
        assert_eq!(active.id, second, "rotate leaves the new PIN active");
        assert_ne!(active.id, first, "the prior PIN is superseded");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn partial_unique_index_rejects_a_second_active_pin(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);
        insert(&pool, user_id, "$argon2id$first", expires)
            .await
            .expect("first active pin");

        let err = insert(&pool, user_id, "$argon2id$second", expires)
            .await
            .expect_err("a second active pin must violate the partial unique index");
        assert!(
            matches!(&err, sqlx::Error::Database(db) if db.is_unique_violation()),
            "expected a unique violation from the single-active index, got {err:?}"
        );

        // A consumed row vacates the active slot (the index is partial on
        // consumed_at IS NULL), so a fresh active PIN is admissible again.
        sqlx::query!(
            "UPDATE password_reset_pins SET consumed_at = now() \
             WHERE user_id = $1 AND consumed_at IS NULL",
            user_id,
        )
        .execute(&pool)
        .await
        .expect("consume the active pin");
        insert(&pool, user_id, "$argon2id$third", expires)
            .await
            .expect("a new active pin is admissible once the prior is consumed");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_rotate_leaves_exactly_one_active(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);

        // Seed a prior active PIN so both concurrent rotates run the supersede
        // DELETE and then race on the INSERT, the READ COMMITTED interleaving
        // the partial unique index must resolve into a single winner.
        insert(&pool, user_id, "$argon2id$seed", expires)
            .await
            .expect("seed prior pin");

        let pool_a = pool.clone();
        let pool_b = pool.clone();
        let a = tokio::spawn(async move {
            let mut conn = pool_a.acquire().await.expect("acquire a");
            rotate(&mut conn, user_id, "$argon2id$a", expires).await
        });
        let b = tokio::spawn(async move {
            let mut conn = pool_b.acquire().await.expect("acquire b");
            rotate(&mut conn, user_id, "$argon2id$b", expires).await
        });

        let (a, b) = tokio::join!(a, b);
        let a = a.expect("rotate task a joined");
        let b = b.expect("rotate task b joined");
        // A two-way race resolves to two Issued outcomes: the loser's INSERT
        // fails once, then its retry supersedes the winner and succeeds. Neither
        // call errors, so neither caller ever sees a 500.
        assert!(
            matches!(a, Ok(RotateOutcome::Issued(_))),
            "concurrent rotate a must issue, got {a:?}"
        );
        assert!(
            matches!(b, Ok(RotateOutcome::Issued(_))),
            "concurrent rotate b must issue, got {b:?}"
        );

        let active_count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM password_reset_pins
               WHERE user_id = $1 AND consumed_at IS NULL AND expires_at > now()"#,
            user_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count active pins");
        assert_eq!(
            active_count, 1,
            "exactly one active recovery PIN must remain after concurrent rotate"
        );
        // The sole survivor must be a genuinely persisted PIN, not a stale or
        // phantom row: its hash is one of the two the concurrent calls stored,
        // so whatever a caller publishes for it verifies against the DB.
        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("the single surviving PIN is active");
        assert!(
            ["$argon2id$a", "$argon2id$b"].contains(&active.pin_hash.as_str()),
            "surviving PIN hash must be one an issuer persisted, got {:?}",
            active.pin_hash
        );
    }

    #[test]
    fn an_inserted_attempt_stops_immediately() {
        let id = Uuid::new_v4();
        assert_eq!(
            next_step(InsertOutcome::Inserted(id), ROTATE_RETRIES),
            NextStep::Done(RotateOutcome::Issued(id)),
            "an uncontended attempt needs no retry"
        );
    }

    #[test]
    fn a_lost_slot_retries_while_retries_remain() {
        assert_eq!(
            next_step(InsertOutcome::LostActiveSlot, ROTATE_RETRIES),
            NextStep::Retry,
            "a lost race is retried, not surfaced"
        );
    }

    #[test]
    fn a_lost_slot_concedes_once_retries_are_spent() {
        assert_eq!(
            next_step(InsertOutcome::LostActiveSlot, 0),
            NextStep::Done(RotateOutcome::RaceLost),
            "a still-taken slot must report the lost race, not another row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rotate_retries_after_a_real_unique_violation(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);

        // Hold an uncommitted active row for the user so rotate's INSERT parks on
        // the single-active slot deterministically instead of racing on the
        // thread scheduler. The supersede DELETE cannot see this uncommitted row,
        // so the INSERT is what blocks.
        let mut blocker = pool.begin().await.expect("begin blocker tx");
        insert(&mut *blocker, user_id, "$argon2id$blocker", expires)
            .await
            .expect("blocker insert");

        let pool_rotate = pool.clone();
        let rotate_task = tokio::spawn(async move {
            let mut conn = pool_rotate.acquire().await.expect("acquire");
            rotate(&mut conn, user_id, "$argon2id$winner", expires).await
        });

        // Once rotate's INSERT is parked on the blocker's slot, committing the
        // blocker forces the first attempt's INSERT to fail with the unique
        // violation; the retry's DELETE then supersedes the committed row and
        // succeeds. This drives the real 23505 retry path with no timing luck.
        wait_for_blocked_backend(&pool).await;
        blocker.commit().await.expect("commit blocker");

        let outcome = rotate_task
            .await
            .expect("join rotate task")
            .expect("rotate succeeds after retrying");
        assert!(
            matches!(outcome, RotateOutcome::Issued(_)),
            "the retry must issue this call's PIN, got {outcome:?}"
        );

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("one active row remains");
        assert_eq!(
            active.pin_hash, "$argon2id$winner",
            "the retry's PIN is the sole active row, having superseded the blocker"
        );
        let active_count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM password_reset_pins
               WHERE user_id = $1 AND consumed_at IS NULL"#,
            user_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count active pins");
        assert_eq!(active_count, 1, "exactly one active row after the retry");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn migration_reconciles_duplicate_active_rows_before_indexing(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);

        // A database that ran the pre-index code could carry two unconsumed PINs
        // for one user. The unique index does not exist in that state, so drop
        // it to recreate the dirty data the migration must reconcile before it
        // can build the index.
        sqlx::query("DROP INDEX idx_password_reset_pins_active_unique")
            .execute(&pool)
            .await
            .expect("drop the single-active index to seed the pre-index state");

        let mut newest = Uuid::nil();
        for (offset, hash) in [
            (10, "$argon2id$old"),
            (5, "$argon2id$mid"),
            (1, "$argon2id$new"),
        ] {
            let created = Utc::now() - TimeDelta::minutes(offset);
            newest = sqlx::query_scalar!(
                "INSERT INTO password_reset_pins (user_id, pin_hash, expires_at, created_at) \
                 VALUES ($1, $2, $3, $4) RETURNING id",
                user_id,
                hash,
                expires,
                created,
            )
            .fetch_one(&pool)
            .await
            .expect("seed a duplicate unconsumed pin");
        }

        // The migration's reconciliation: keep the newest unconsumed row per
        // user, delete the older duplicates.
        sqlx::query!(
            "DELETE FROM password_reset_pins p \
             USING ( \
                 SELECT id, row_number() OVER ( \
                     PARTITION BY user_id ORDER BY created_at DESC, id DESC) AS rn \
                 FROM password_reset_pins WHERE consumed_at IS NULL) ranked \
             WHERE p.id = ranked.id AND ranked.rn > 1",
        )
        .execute(&pool)
        .await
        .expect("reconcile duplicate active rows");

        // Recreating the index must now succeed, proving the migration applies
        // cleanly against reconciled data.
        sqlx::query("CREATE UNIQUE INDEX idx_password_reset_pins_active_unique ON password_reset_pins (user_id) WHERE consumed_at IS NULL")
            .execute(&pool)
            .await
            .expect("recreate the index after reconciliation");

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("one active row remains");
        assert_eq!(
            active.id, newest,
            "reconciliation keeps the newest unconsumed row"
        );
        let remaining = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM password_reset_pins
               WHERE user_id = $1 AND consumed_at IS NULL"#,
            user_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count unconsumed rows");
        assert_eq!(remaining, 1, "exactly one unconsumed row survives");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn consume_rejects_expired_pin(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        // Insert a PIN that expired 1 second ago (unconsumed).
        let expired = Utc::now() - TimeDelta::seconds(1);
        let id = insert(&pool, user_id, "$argon2id$hash", expired)
            .await
            .expect("insert expired pin");

        // consume must refuse an expired PIN even when consumed_at is still NULL.
        assert!(
            !consume(&pool, id).await.expect("consume"),
            "an expired PIN cannot be consumed"
        );

        // The row must not be marked consumed so a fresh PIN can be issued.
        let consumed_at = sqlx::query_scalar!(
            "SELECT consumed_at FROM password_reset_pins WHERE id = $1",
            id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch");
        assert!(consumed_at.is_none(), "expired PIN row was not consumed");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rotate_rolls_back_when_insert_fails(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = Utc::now() + TimeDelta::minutes(15);
        let original = insert(&pool, user_id, "$argon2id$original", expires)
            .await
            .expect("seed original pin");

        // A pre-0001 expiry trips the timestamptz decode-range CHECK, so the
        // INSERT inside rotate fails after the supersede DELETE has run. An
        // atomic rotate must roll that DELETE back and leave the original PIN
        // active; a non-atomic supersede would destroy it and lock the user out
        // of recovery.
        let out_of_range = chrono::NaiveDate::from_ymd_opt(0, 1, 1)
            .expect("year-0 date")
            .and_time(chrono::NaiveTime::MIN)
            .and_utc();
        let mut conn = pool.acquire().await.expect("acquire");
        let err = rotate(&mut conn, user_id, "$argon2id$replacement", out_of_range)
            .await
            .expect_err("insert must violate the expires_at CHECK constraint");
        assert!(
            matches!(err, sqlx::Error::Database(_)),
            "expected a database constraint violation, got {err:?}"
        );

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("the original PIN must survive the rolled-back rotate");
        assert_eq!(
            active.id, original,
            "rotate must preserve the prior PIN when the insert fails"
        );
    }
}
