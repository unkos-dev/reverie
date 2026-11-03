//! Transactional password admission keyed on lower-case email, including unknown accounts.
//!
//! THREAT: the row lock spans verification and accounting so replicas cannot verify through a block.

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

const WINDOW: TimeDelta = TimeDelta::minutes(15);
const FAILURE_LIMIT: usize = 10;

/// Account admission before any credential lookup or hashing.
pub enum Admission {
    /// Owns the serialised credential outcome until commit or rollback.
    Allowed(Attempt),
    /// Positive, rounded-up seconds until the existing block expires.
    Blocked(u64),
}

/// A locked account row and its uncommitted credential result.
pub struct Attempt {
    tx: Transaction<'static, Postgres>,
    email: String,
    failures: Vec<DateTime<Utc>>,
    locked_until: Option<DateTime<Utc>>,
}

fn retry_after(deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<u64> {
    deadline.filter(|deadline| *deadline > now).map(|deadline| {
        let remaining = deadline - now;
        (remaining.num_seconds() + i64::from(remaining.subsec_nanos() > 0))
            .unsigned_abs()
            .max(1)
    })
}

fn complete_failure(
    failures: &mut Vec<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    failures.retain(|failure| *failure > now - WINDOW);
    failures.push(now);
    failures.sort_unstable();
    (failures.len() >= FAILURE_LIMIT).then_some(now + WINDOW)
}

async fn database_time(connection: &mut PgConnection) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(connection)
        .await
}

async fn lock(pool: &PgPool, email: &str) -> Result<Attempt, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let email = email.to_lowercase();
    let row = sqlx::query!(
        "INSERT INTO local_login_throttle (email_lower) VALUES ($1) \
         ON CONFLICT (email_lower) DO UPDATE SET email_lower = EXCLUDED.email_lower \
         RETURNING completed_failures, locked_until",
        email,
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(Attempt {
        tx,
        email,
        failures: row.completed_failures,
        locked_until: row.locked_until,
    })
}

/// Lock and admit an email using wall time obtained after lock acquisition.
///
/// # Errors
/// Returns database errors without admitting the request.
pub async fn admit(pool: &PgPool, email: &str) -> Result<Admission, sqlx::Error> {
    let mut attempt = lock(pool, email).await?;
    let now = database_time(attempt.connection()).await?;
    if let Some(seconds) = retry_after(attempt.locked_until, now) {
        attempt.tx.rollback().await?;
        return Ok(Admission::Blocked(seconds));
    }
    Ok(Admission::Allowed(attempt))
}

impl Attempt {
    /// Use the held connection for decisive credential and cohort queries.
    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    /// Commit a completed failure before exposing its credential result.
    ///
    /// # Errors
    /// Returns query or commit errors; an unknown commit outcome is never replayed.
    pub async fn fail(mut self) -> Result<(), sqlx::Error> {
        let now = database_time(self.connection()).await?;
        let locked_until = complete_failure(&mut self.failures, now);
        sqlx::query!(
            "UPDATE local_login_throttle SET completed_failures = $2, locked_until = $3 WHERE email_lower = $1",
            self.email,
            &self.failures,
            locked_until,
        )
        .execute(&mut *self.tx)
        .await?;
        self.tx.commit().await
    }

    /// Clear accounting and commit before session storage is accessed.
    ///
    /// # Errors
    /// Returns query or commit errors without replaying an unknown outcome.
    pub async fn succeed(mut self) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE local_login_throttle SET completed_failures = '{}', locked_until = NULL WHERE email_lower = $1",
            self.email,
        )
        .execute(&mut *self.tx)
        .await?;
        self.tx.commit().await
    }
}

/// Clear accounting under the same row lock as password admission.
///
/// # Errors
/// Returns acquisition, update or commit errors.
pub async fn reset(pool: &PgPool, email: &str) -> Result<(), sqlx::Error> {
    lock(pool, email).await?.succeed().await
}

/// Delete at most `batch` inactive rows, skipping current admission and reset owners.
///
/// # Errors
/// Returns database errors without changing protection on retained rows.
pub async fn delete_inactive(pool: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "WITH expired AS (SELECT email_lower FROM local_login_throttle \
         WHERE (locked_until IS NULL OR locked_until <= clock_timestamp()) \
         AND clock_timestamp() - interval '15 minutes' >= ALL(completed_failures) \
         ORDER BY updated_at, email_lower LIMIT $1 FOR UPDATE SKIP LOCKED) \
         DELETE FROM local_login_throttle AS throttle USING expired \
         WHERE throttle.email_lower = expired.email_lower \
         AND (throttle.locked_until IS NULL OR throttle.locked_until <= clock_timestamp()) \
         AND clock_timestamp() - interval '15 minutes' >= ALL(throttle.completed_failures)",
        batch,
    )
    .execute(pool)
    .await?
    .rows_affected())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub async fn seed(
        pool: &PgPool,
        email: &str,
        failures: &[DateTime<Utc>],
        deadline: Option<DateTime<Utc>>,
    ) {
        sqlx::query!("INSERT INTO local_login_throttle (email_lower, completed_failures, locked_until) VALUES ($1, $2, $3) ON CONFLICT (email_lower) DO UPDATE SET completed_failures = EXCLUDED.completed_failures, locked_until = EXCLUDED.locked_until", email.to_lowercase(), failures, deadline)
            .execute(pool).await.unwrap();
    }

    pub async fn snapshot(
        pool: &PgPool,
        email: &str,
    ) -> (Vec<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let row = sqlx::query!("SELECT completed_failures, locked_until FROM local_login_throttle WHERE email_lower = $1", email.to_lowercase())
            .fetch_one(pool).await.unwrap();
        (row.completed_failures, row.locked_until)
    }

    async fn allowed(pool: &PgPool, email: &str) -> Attempt {
        match admit(pool, email).await.unwrap() {
            Admission::Allowed(attempt) => attempt,
            Admission::Blocked(seconds) => panic!("unexpected block: {seconds}"),
        }
    }

    #[test]
    fn password_admission_exact_window_and_block_boundaries() {
        let now = DateTime::parse_from_rfc3339("2026-10-19T00:15:01Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut failures = vec![now - WINDOW, now - WINDOW + TimeDelta::microseconds(1)];
        assert!(complete_failure(&mut failures, now).is_none());
        assert_eq!(
            failures,
            vec![now - WINDOW + TimeDelta::microseconds(1), now]
        );
        assert_eq!(retry_after(Some(now), now), None);
        assert_eq!(
            retry_after(Some(now + TimeDelta::microseconds(1)), now),
            Some(1)
        );
        assert_eq!(
            retry_after(
                Some(now + TimeDelta::seconds(1) + TimeDelta::microseconds(1)),
                now
            ),
            Some(2)
        );
        assert_eq!(
            retry_after(Some(now - TimeDelta::microseconds(1)), now),
            None
        );
        let start = now - TimeDelta::minutes(15) - TimeDelta::seconds(1);
        let mut failures = vec![start];
        failures.extend(vec![
            start + TimeDelta::minutes(14) + TimeDelta::seconds(59);
            8
        ]);
        assert!(complete_failure(&mut failures, now).is_none());
        assert_eq!(failures.len(), 9);
        assert_eq!(complete_failure(&mut failures, now), Some(now + WINDOW));
        assert_eq!(failures.len(), 10);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_threshold_rejection_and_reset(pool: PgPool) {
        for count in 1..=10 {
            allowed(&pool, "User@Example.com")
                .await
                .fail()
                .await
                .unwrap();
            let (history, deadline) = snapshot(&pool, "USER@example.com").await;
            assert_eq!(history.len(), count);
            assert_eq!(deadline.is_some(), count == 10);
        }
        let before = snapshot(&pool, "user@example.com").await;
        assert!(matches!(
            admit(&pool, "user@example.com").await.unwrap(),
            Admission::Blocked(1..=900)
        ));
        assert_eq!(snapshot(&pool, "user@example.com").await, before);
        reset(&pool, "USER@EXAMPLE.COM").await.unwrap();
        assert_eq!(snapshot(&pool, "user@example.com").await, (vec![], None));
        allowed(&pool, "user@example.com")
            .await
            .fail()
            .await
            .unwrap();
        assert_eq!(snapshot(&pool, "user@example.com").await.0.len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_drop_rolls_back_and_commit_survives_cancellation(pool: PgPool) {
        seed(&pool, "cancel", &[], None).await;
        let mut attempt = allowed(&pool, "cancel").await;
        sqlx::query!("UPDATE local_login_throttle SET locked_until = clock_timestamp() + interval '15 minutes' WHERE email_lower = 'cancel'")
            .execute(attempt.connection()).await.unwrap();
        drop(attempt);
        allowed(&pool, "cancel").await.fail().await.unwrap();
        assert_eq!(snapshot(&pool, "cancel").await.0.len(), 1);
        let task_pool = pool.clone();
        let (committed, observed) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            allowed(&task_pool, "cancel").await.fail().await.unwrap();
            committed.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        observed.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(snapshot(&pool, "cancel").await.0.len(), 2);
    }
}
