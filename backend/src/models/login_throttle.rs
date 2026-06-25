//! Per-account login throttle: DB-backed escalating backoff keyed on normalized
//! email.
//!
//! # Tier 2 — security-critical
//!
//! State lives in Postgres, not memory, so an out-of-band `reverie
//! unlock-account` CLI process can clear it (an in-memory map cannot be cleared
//! cross-process). Keyed on the lower-cased email rather than `user_id` so the
//! throttle exists independent of whether the email resolves to an account,
//! keeping the failed-login path account-existence uniform.
//!
//! THREAT (lockout DoS): this escalating per-account backoff must
//! NOT block a correct password. The login handler verifies the password first
//! and only calls [`record_failure`] on a *failed* attempt; a success calls
//! [`reset`]. Per-source (per-IP) rate limiting does the hard blocking; this is
//! the IP-independent backstop.

use sqlx::PgPool;
use time::OffsetDateTime;

/// Lower-case an email for use as the throttle key. Applied consistently across
/// all three operations so the same address always maps to the same row.
fn key(email: &str) -> String {
    email.to_lowercase()
}

/// Record a failed login for an email and return the new `locked_until`.
///
/// The lock window grows by capped exponential backoff: `min(cap, base *
/// 2^prior_failures)`. The first failure waits `min(cap, base)`; each subsequent
/// failure doubles the base until it saturates at `cap`. Both bounds are
/// operator-configured (seconds).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the upsert.
#[allow(dead_code)] // Consumed by the local-login route in this PR
pub async fn record_failure(
    pool: &PgPool,
    email: &str,
    base_secs: i32,
    cap_secs: i32,
) -> Result<OffsetDateTime, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO local_login_throttle (email_lower, fail_count, locked_until)
           VALUES ($1, 1, now() + make_interval(secs => LEAST($3::double precision,
                                                              $2::double precision)))
           ON CONFLICT (email_lower) DO UPDATE SET
             fail_count = local_login_throttle.fail_count + 1,
             locked_until = now() + make_interval(secs => LEAST($3::double precision,
                 $2::double precision * power(2, local_login_throttle.fail_count)))
           RETURNING locked_until AS "locked_until!""#,
        key(email),
        f64::from(base_secs),
        f64::from(cap_secs),
    )
    .fetch_one(pool)
    .await
}

/// Clear an email's throttle row. Called on a successful login and
/// by the `reverie unlock-account` CLI command.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `DELETE`.
#[allow(dead_code)] // Consumed by the local-login route + CLI in this PR
pub async fn reset(pool: &PgPool, email: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM local_login_throttle WHERE email_lower = $1",
        key(email),
    )
    .execute(pool)
    .await
    .map(|_| ())
}

/// The time until which an email is backed off, or `None` if it is not currently
/// locked (no row, or the lock has elapsed).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `SELECT`.
#[allow(dead_code)] // Consumed by the local-login route in this PR
pub async fn backoff_until(
    pool: &PgPool,
    email: &str,
) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT locked_until AS "locked_until?" FROM local_login_throttle
           WHERE email_lower = $1 AND locked_until > now()"#,
        key(email),
    )
    .fetch_optional(pool)
    .await
    .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: i32 = 2;
    const CAP: i32 = 60;

    #[sqlx::test(migrations = "./migrations")]
    async fn unlocked_email_has_no_backoff(pool: PgPool) {
        assert!(
            backoff_until(&pool, "nobody@example.com")
                .await
                .expect("query")
                .is_none(),
            "an email with no throttle row is not backed off"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn failures_escalate_and_reset_clears(pool: PgPool) {
        let first = record_failure(&pool, "User@Example.com", BASE, CAP)
            .await
            .expect("first failure");
        let second = record_failure(&pool, "user@example.com", BASE, CAP)
            .await
            .expect("second failure");
        assert!(second > first, "the lock window grows with each failure");

        assert!(
            backoff_until(&pool, "USER@EXAMPLE.COM")
                .await
                .expect("query")
                .is_some(),
            "case-insensitive key resolves the same row"
        );

        reset(&pool, "user@example.com").await.expect("reset");
        assert!(
            backoff_until(&pool, "user@example.com")
                .await
                .expect("query")
                .is_none(),
            "reset clears the backoff (success path)"
        );
    }
}
