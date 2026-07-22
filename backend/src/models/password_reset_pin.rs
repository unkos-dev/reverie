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

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

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
    pub expires_at: OffsetDateTime,
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

/// Delete any unconsumed PIN rows for a user so at most one stays live. Call
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
    expires_at: OffsetDateTime,
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
enum InsertOutcome {
    /// The freshly hashed PIN was persisted as the user's sole active row.
    Inserted(Uuid),
    /// A concurrent issuance committed the active row between this attempt's
    /// supersede `DELETE` and its `INSERT`, so the `INSERT` hit the
    /// `idx_password_reset_pins_active_unique` index. Nothing was persisted (the
    /// attempt's transaction rolled back); [`rotate_with_retry`] decides whether
    /// to retry or concede the race.
    LostActiveSlot,
}

/// Atomically supersede a user's prior active PINs and persist a fresh one,
/// reporting whether this call won the single active slot. The
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
/// # Errors
///
/// Returns [`sqlx::Error`] if the transaction cannot begin, either statement
/// fails for a reason other than the single-active unique violation, or the
/// commit fails. On any failure the transaction rolls back, so the prior active
/// PIN is preserved.
pub async fn rotate(
    pool: &PgPool,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: OffsetDateTime,
) -> Result<RotateOutcome, sqlx::Error> {
    // The single attempt is passed as a closure so the bounded-retry policy that
    // classifies the race outcome can be exercised deterministically, without a
    // live triple-race that no external test can schedule reliably (see
    // rotate_with_retry's unit tests).
    rotate_with_retry(|| rotate_once(pool, user_id, pin_hash, expires_at)).await
}

/// rotate's bounded single-active-race retry policy over one attempt.
///
/// A lost race is benign: retry the whole supersede-then-insert exactly once so
/// the retry's `DELETE` supersedes the winner's now-committed row. If a further
/// concurrent issuer still holds the slot after the retry, this call's PIN was
/// never persisted, so return [`RotateOutcome::RaceLost`]. The caller then
/// withholds its unstorable clear PIN rather than reporting another issuer's row
/// as this call's success.
async fn rotate_with_retry<A, F>(mut attempt: A) -> Result<RotateOutcome, sqlx::Error>
where
    A: FnMut() -> F,
    F: std::future::Future<Output = Result<InsertOutcome, sqlx::Error>>,
{
    if let InsertOutcome::Inserted(id) = attempt().await? {
        return Ok(RotateOutcome::Issued(id));
    }
    tracing::debug!("recovery-pin rotate lost the single-active race; retrying once");
    match attempt().await? {
        InsertOutcome::Inserted(id) => Ok(RotateOutcome::Issued(id)),
        InsertOutcome::LostActiveSlot => Ok(RotateOutcome::RaceLost),
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
    pool: &PgPool,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: OffsetDateTime,
) -> Result<InsertOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
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
    use time::Duration;

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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
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
        let expired = OffsetDateTime::now_utc() - Duration::minutes(1);
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
        let first = insert(&pool, user_id, "$argon2id$first", expires)
            .await
            .expect("seed first pin");

        let second = match rotate(&pool, user_id, "$argon2id$second", expires)
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);

        // Seed a prior active PIN so both concurrent rotates run the supersede
        // DELETE and then race on the INSERT, the READ COMMITTED interleaving
        // the partial unique index must resolve into a single winner.
        insert(&pool, user_id, "$argon2id$seed", expires)
            .await
            .expect("seed prior pin");

        let pool_a = pool.clone();
        let pool_b = pool.clone();
        let a = tokio::spawn(async move { rotate(&pool_a, user_id, "$argon2id$a", expires).await });
        let b = tokio::spawn(async move { rotate(&pool_b, user_id, "$argon2id$b", expires).await });

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

    #[tokio::test]
    async fn rotate_with_retry_issues_on_the_first_attempt() {
        let id = Uuid::new_v4();
        let mut attempts = 0u32;
        let outcome = rotate_with_retry(|| {
            attempts += 1;
            async move { Ok::<_, sqlx::Error>(InsertOutcome::Inserted(id)) }
        })
        .await
        .expect("uncontended attempt succeeds");
        assert_eq!(attempts, 1, "an uncontended rotate makes no second attempt");
        assert_eq!(outcome, RotateOutcome::Issued(id));
    }

    #[tokio::test]
    async fn rotate_with_retry_issues_after_one_lost_race() {
        let id = Uuid::new_v4();
        let mut attempts = 0u32;
        let outcome = rotate_with_retry(|| {
            let n = attempts;
            attempts += 1;
            async move {
                Ok::<_, sqlx::Error>(if n == 0 {
                    InsertOutcome::LostActiveSlot
                } else {
                    InsertOutcome::Inserted(id)
                })
            }
        })
        .await
        .expect("the retry succeeds");
        assert_eq!(attempts, 2, "one lost race triggers exactly one retry");
        assert_eq!(outcome, RotateOutcome::Issued(id));
    }

    #[tokio::test]
    async fn rotate_with_retry_concedes_race_when_slot_stays_taken() {
        let mut attempts = 0u32;
        let outcome = rotate_with_retry(|| {
            attempts += 1;
            async move { Ok::<_, sqlx::Error>(InsertOutcome::LostActiveSlot) }
        })
        .await
        .expect("a conceded race is not an error");
        assert_eq!(attempts, 2, "rotate retries exactly once before conceding");
        assert_eq!(
            outcome,
            RotateOutcome::RaceLost,
            "a still-taken slot after the retry must report the lost race, not another row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rotate_retries_after_a_real_unique_violation(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);

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
            rotate(&pool_rotate, user_id, "$argon2id$winner", expires).await
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);

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
            let created = OffsetDateTime::now_utc() - Duration::minutes(offset);
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
        let expired = OffsetDateTime::now_utc() - Duration::seconds(1);
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
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
        let original = insert(&pool, user_id, "$argon2id$original", expires)
            .await
            .expect("seed original pin");

        // A pre-0001 expiry trips the timestamptz decode-range CHECK, so the
        // INSERT inside rotate fails after the supersede DELETE has run. An
        // atomic rotate must roll that DELETE back and leave the original PIN
        // active; a non-atomic supersede would destroy it and lock the user out
        // of recovery.
        let out_of_range = OffsetDateTime::new_utc(
            time::Date::from_calendar_date(0, time::Month::January, 1).expect("year-0 date"),
            time::Time::MIDNIGHT,
        );
        let err = rotate(&pool, user_id, "$argon2id$replacement", out_of_range)
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
