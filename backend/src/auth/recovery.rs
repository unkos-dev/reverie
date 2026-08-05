//! Forgot-password recovery: CSPRNG PIN generation and the operator-readable
//! per-user host file.
//!
//! # Tier 2: security-critical
//!
//! The clear PIN is written to a per-user file `<dir>/<user_id>.pin` (mode 0600,
//! inside a recovery directory created mode 0700, outside any web-served
//! directory) as proof-of-host-access: an operator reads it and relays it to the
//! user. The per-user path means concurrent recoveries for different users never
//! collide. The database persists only the PIN's Argon2id hash, a short expiry,
//! and a consumed marker (see [`crate::models::password_reset_pin`]). The PIN is
//! single-use and rate-limited.
//!
//! THREAT: the PIN is never logged (hard rule 3). The file is removed on
//! consumption or expiry. On create, the hash row is written BEFORE the file, so
//! a crash between the two leaves at worst an unconsumed-but-unusable row that
//! expiry sweeps, never a cleartext PIN with no consuming row. [`crate::auth::recovery::issue_pin`]
//! runs both steps under one cross-process lock, so the file and the single
//! active row always describe the same PIN.

use std::fs::{self, OpenOptions, Permissions};
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::password_reset_pin::{IssuanceLock, RotateOutcome, rotate};

/// Generate a 10-digit numeric recovery PIN from the OS CSPRNG. Numeric for
/// operator-to-user transcription; brute force is bounded by per-source rate
/// limiting, single use, and a short expiry.
pub fn generate_pin() -> String {
    let mut bytes = [0u8; 8];
    rand::fill(&mut bytes);
    // Modulo bias over a 2^64 source into 10^10 is negligible.
    let n = u64::from_le_bytes(bytes) % 10_000_000_000;
    format!("{n:010}")
}

/// Path of a user's recovery PIN file: `<dir>/<user_id>.pin`. Per-user so two
/// concurrent recoveries for different accounts write distinct files.
fn pin_file_path(dir: &Path, user_id: Uuid) -> PathBuf {
    dir.join(format!("{user_id}.pin"))
}

/// Staging path a PIN file is written to before it is renamed into place.
fn pin_staging_path(dir: &Path, user_id: Uuid) -> PathBuf {
    dir.join(format!("{user_id}.pin.staged"))
}

/// Write the clear PIN, target email, and expiry to `<dir>/<user_id>.pin` with
/// mode 0600, creating `dir` (mode 0700) if absent and replacing any prior file
/// for the user. Permissions are enforced after open so an existing file/dir
/// with looser perms is corrected.
///
/// THREAT (partial read of a PIN file): the content is written to a staging file
/// in the same directory and renamed into place, so an operator reading the file
/// concurrently sees either the whole prior PIN or the whole new one, never a
/// truncated PIN or an empty file that reads as "recovery is broken". The
/// staging file is created 0600, so the clear PIN is never briefly world
/// readable, and it is removed if any step before the rename fails.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory or file cannot be created,
/// written, or renamed into place.
pub fn write_pin_file(
    dir: &Path,
    user_id: Uuid,
    email: &str,
    pin: &str,
    expires_at: DateTime<Utc>,
) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    let staging = pin_staging_path(dir, user_id);
    match stage_pin_file(&staging, email, pin, expires_at) {
        Ok(()) => fs::rename(&staging, pin_file_path(dir, user_id)),
        Err(e) => {
            if let Err(cleanup) = fs::remove_file(&staging)
                && cleanup.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %cleanup, "failed to remove a staged recovery PIN file");
            }
            Err(e)
        }
    }
}

/// Write one PIN file's content to `staging` at mode 0600 and flush it to disk,
/// so the rename that follows publishes durable content rather than an empty
/// file a crash left behind.
fn stage_pin_file(
    staging: &Path,
    email: &str,
    pin: &str,
    expires_at: DateTime<Utc>,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(staging)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    writeln!(file, "Reverie password recovery")?;
    writeln!(file, "email: {email}")?;
    writeln!(file, "pin: {pin}")?;
    writeln!(file, "expires_at: {expires_at}")?;
    file.sync_all()
}

/// One recovery-PIN issuance: the clear PIN to publish and everything the
/// database row it must agree with needs.
///
/// Holds a SECRET (`pin`) and deliberately implements neither `Debug` nor
/// `Serialize`, so the clear PIN cannot reach a log line or a response body by
/// accident (CWE-532).
pub struct PinIssuance {
    /// Account the PIN recovers.
    pub user_id: Uuid,
    /// Address the PIN file names, for the operator relaying it.
    pub email: String,
    /// The clear PIN. SECRET: file only, never logged.
    pub pin: String,
    /// Argon2id PHC of `pin`, the only form persisted.
    pub pin_hash: String,
    /// When the PIN stops being valid.
    pub expires_at: DateTime<Utc>,
}

/// Whether an issuance published its PIN or stood down for a concurrent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinIssueOutcome {
    /// The PIN is the sole active row and is the content of the user's PIN file.
    Published,
    /// A concurrent issuance owns this user's recovery slot. Nothing was
    /// persisted and nothing was published, so the file still describes that
    /// issuance's PIN. Callers MUST NOT present this call's PIN to anyone.
    Withheld,
}

/// Failure modes of [`issue_pin`].
#[derive(Debug, thiserror::Error)]
pub enum IssuePinError {
    /// Locking or rotation failed; no PIN was published.
    #[error("recovery PIN issuance failed in the database")]
    Db(#[from] sqlx::Error),
    /// The row is persisted but the PIN file could not be published, so no
    /// usable PIN exists; the row expires harmlessly.
    #[error("recovery PIN file publication failed")]
    Publish(#[source] std::io::Error),
    /// The issuance task did not run to completion.
    #[error("the recovery PIN issuance task did not complete")]
    Task(#[from] tokio::task::JoinError),
}

/// Rotate a user's recovery PIN and publish it to the operator file as one
/// serialized step, so the file always describes the single active row.
///
/// THREAT (a superseded issuance publishing last): rotation and publication are
/// writes to two stores. A rotation that commits proves only that this issuance
/// held the active slot at commit time; without serialization a slower issuance
/// could commit, be superseded by a newer one that published its own PIN, and
/// then overwrite the file with a PIN no stored hash verifies, silently denying
/// the account recovery. The HTTP handler and the `reset-password` CLI are
/// separate processes, so the ordering is imposed by a per-user Postgres
/// advisory lock ([`IssuanceLock`]) held across both writes: the issuance that
/// wins the lock is the last writer of the file. An issuance that cannot take
/// the lock within its wait budget reports [`PinIssueOutcome::Withheld`] rather
/// than proceeding unserialized, so contention degrades to "ask again", never to
/// a mismatched PIN.
///
/// The whole section runs on a detached task, so a caller that goes away (a
/// client disconnect cancelling an Axum handler) cannot drop the future between
/// the rotation and the release of the lock.
///
/// # Errors
///
/// - [`IssuePinError::Db`] when the lock or the rotation fails; nothing was
///   published.
/// - [`IssuePinError::Publish`] when the row was persisted but the file could
///   not be written; no usable PIN exists and the row expires.
/// - [`IssuePinError::Task`] when the issuance task panicked or was aborted.
pub async fn issue_pin(
    pool: &PgPool,
    dir: &Path,
    issuance: PinIssuance,
) -> Result<PinIssueOutcome, IssuePinError> {
    let pool = pool.clone();
    let dir = dir.to_path_buf();
    tokio::spawn(async move { issue_pin_locked(&pool, &dir, &issuance).await }).await?
}

/// The serialized issuance section: take the user's issuance lock, rotate and
/// publish under it, and release it on every exit path.
async fn issue_pin_locked(
    pool: &PgPool,
    dir: &Path,
    issuance: &PinIssuance,
) -> Result<PinIssueOutcome, IssuePinError> {
    let Some(mut lock) = IssuanceLock::try_acquire(pool, issuance.user_id).await? else {
        return Ok(PinIssueOutcome::Withheld);
    };
    let result = rotate_and_publish(lock.connection(), dir, issuance).await;
    lock.release().await;
    result
}

/// Rotate then publish, in that order, with the caller holding the issuance
/// lock. The row is persisted first: a failure to publish leaves an unusable row
/// that expiry sweeps, never a cleartext PIN no row can consume.
async fn rotate_and_publish(
    conn: &mut sqlx::PgConnection,
    dir: &Path,
    issuance: &PinIssuance,
) -> Result<PinIssueOutcome, IssuePinError> {
    match rotate(
        conn,
        issuance.user_id,
        &issuance.pin_hash,
        issuance.expires_at,
    )
    .await?
    {
        RotateOutcome::RaceLost => Ok(PinIssueOutcome::Withheld),
        RotateOutcome::Issued(_) => {
            write_pin_file(
                dir,
                issuance.user_id,
                &issuance.email,
                &issuance.pin,
                issuance.expires_at,
            )
            .map_err(IssuePinError::Publish)?;
            Ok(PinIssueOutcome::Published)
        }
    }
}

/// Remove a user's PIN file. An already-absent file is success (idempotent
/// cleanup on consume or expiry).
///
/// # Errors
///
/// Returns [`std::io::Error`] for failures other than the file being absent.
pub fn remove_pin_file(dir: &Path, user_id: Uuid) -> std::io::Result<()> {
    match fs::remove_file(pin_file_path(dir, user_id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_pin_is_ten_digits() {
        for _ in 0..50 {
            let pin = generate_pin();
            assert_eq!(pin.len(), 10, "PIN is zero-padded to 10 digits: {pin}");
            assert!(
                pin.chars().all(|c| c.is_ascii_digit()),
                "PIN is numeric: {pin}"
            );
        }
    }

    #[test]
    fn pin_file_is_written_0600_with_pin_then_removed() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let dir = tmp.path();
        let user_id = Uuid::new_v4();
        let expires = Utc::now() + chrono::TimeDelta::minutes(15);

        write_pin_file(dir, user_id, "user@example.com", "1234567890", expires).expect("write");
        let path = pin_file_path(dir, user_id);
        let meta = fs::metadata(&path).expect("metadata");
        assert_eq!(
            meta.permissions().mode() & 0o777,
            0o600,
            "PIN file must be mode 0600"
        );
        let contents = fs::read_to_string(&path).expect("read");
        assert!(
            contents.contains("pin: 1234567890"),
            "clear PIN is in the file"
        );
        assert!(
            contents.contains("user@example.com"),
            "email is in the file"
        );

        remove_pin_file(dir, user_id).expect("remove");
        assert!(!path.exists(), "file removed on cleanup");
        // Idempotent: removing an absent file is success.
        remove_pin_file(dir, user_id).expect("remove-absent is ok");
    }

    #[test]
    fn rewriting_a_pin_file_leaves_no_staging_file() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let dir = tmp.path();
        let user_id = Uuid::new_v4();
        let expires = Utc::now() + chrono::TimeDelta::minutes(15);

        write_pin_file(dir, user_id, "user@example.com", "1111111111", expires).expect("first");
        write_pin_file(dir, user_id, "user@example.com", "2222222222", expires).expect("second");

        let contents = fs::read_to_string(pin_file_path(dir, user_id)).expect("read");
        assert!(
            contents.contains("pin: 2222222222") && !contents.contains("1111111111"),
            "the newest PIN wholly replaces the prior one: {contents}"
        );
        assert!(
            !pin_staging_path(dir, user_id).exists(),
            "the staging file is renamed away, never left behind"
        );
    }

    async fn insert_user(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO users (display_name, email) \
             VALUES ('Recovery Test', 'recovery@example.com') RETURNING id"
        )
        .fetch_one(pool)
        .await
        .expect("insert user")
    }

    fn issuance_for(user_id: Uuid, pin: &str) -> PinIssuance {
        PinIssuance {
            user_id,
            email: "recovery@example.com".to_owned(),
            pin: pin.to_owned(),
            pin_hash: crate::auth::password::hash_password(pin.as_bytes()).expect("hash pin"),
            expires_at: Utc::now() + chrono::TimeDelta::minutes(15),
        }
    }

    fn published_pin(dir: &Path, user_id: Uuid) -> String {
        let contents = fs::read_to_string(pin_file_path(dir, user_id)).expect("read pin file");
        contents
            .lines()
            .find_map(|line| line.strip_prefix("pin: "))
            .expect("the file names a PIN")
            .to_owned()
    }

    async fn assert_published_pin_matches_active_row(pool: &PgPool, dir: &Path, user_id: Uuid) {
        let active = crate::models::password_reset_pin::find_active_by_user(pool, user_id)
            .await
            .expect("find active")
            .expect("one active row");
        crate::auth::password::verify_password(
            published_pin(dir, user_id).as_bytes(),
            &active.pin_hash,
        )
        .expect("the published PIN must verify against the single active row");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn issue_pin_publishes_the_pin_the_active_row_hashes(pool: PgPool) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let user_id = insert_user(&pool).await;

        let outcome = issue_pin(&pool, tmp.path(), issuance_for(user_id, "1234567890"))
            .await
            .expect("issue");

        assert_eq!(outcome, PinIssueOutcome::Published);
        assert_eq!(published_pin(tmp.path(), user_id), "1234567890");
        assert_published_pin_matches_active_row(&pool, tmp.path(), user_id).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_held_issuance_lock_withholds_both_the_row_and_the_file(pool: PgPool) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let user_id = insert_user(&pool).await;

        // A first issuance publishes normally, then a second issuer takes the
        // user's issuance lock and holds it, standing in for an in-flight
        // issuance that has committed its row but not yet published its PIN.
        issue_pin(&pool, tmp.path(), issuance_for(user_id, "1111111111"))
            .await
            .expect("first issuance");
        let held = IssuanceLock::try_acquire(&pool, user_id)
            .await
            .expect("acquire the issuance lock")
            .expect("the lock is free");

        let outcome = issue_pin(&pool, tmp.path(), issuance_for(user_id, "2222222222"))
            .await
            .expect("the contended issuance is not an error");

        assert_eq!(
            outcome,
            PinIssueOutcome::Withheld,
            "an issuance that cannot take the lock must not rotate or publish"
        );
        assert_eq!(
            published_pin(tmp.path(), user_id),
            "1111111111",
            "the withheld issuance must not overwrite the live PIN file"
        );
        assert_published_pin_matches_active_row(&pool, tmp.path(), user_id).await;

        // Releasing the lock lets the next issuance through, so contention is a
        // wait, never a lockout.
        held.release().await;
        let outcome = issue_pin(&pool, tmp.path(), issuance_for(user_id, "3333333333"))
            .await
            .expect("issue after release");
        assert_eq!(outcome, PinIssueOutcome::Published);
        assert_eq!(published_pin(tmp.path(), user_id), "3333333333");
        assert_published_pin_matches_active_row(&pool, tmp.path(), user_id).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_issuances_leave_the_file_matching_the_active_row(pool: PgPool) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let user_id = insert_user(&pool).await;
        let dir = tmp.path().to_path_buf();

        let (pool_a, pool_b) = (pool.clone(), pool.clone());
        let (dir_a, dir_b) = (dir.clone(), dir.clone());
        let a = tokio::spawn(async move {
            issue_pin(&pool_a, &dir_a, issuance_for(user_id, "4444444444")).await
        });
        let b = tokio::spawn(async move {
            issue_pin(&pool_b, &dir_b, issuance_for(user_id, "5555555555")).await
        });
        let (a, b) = tokio::join!(a, b);
        let a = a.expect("join a").expect("issuance a");
        let b = b.expect("join b").expect("issuance b");
        assert!(
            a == PinIssueOutcome::Published || b == PinIssueOutcome::Published,
            "at least one concurrent issuance must publish, got {a:?} and {b:?}"
        );

        let active_count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM password_reset_pins
               WHERE user_id = $1 AND consumed_at IS NULL AND expires_at > now()"#,
            user_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count active pins");
        assert_eq!(active_count, 1, "exactly one active PIN survives");
        assert_published_pin_matches_active_row(&pool, &dir, user_id).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_failed_publication_still_releases_the_issuance_lock(pool: PgPool) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let user_id = insert_user(&pool).await;
        // A regular file where the recovery directory should be: create_dir_all
        // fails, so the row is persisted but publication cannot happen.
        let blocked = tmp.path().join("not-a-directory");
        fs::write(&blocked, b"").expect("create the blocking file");

        let err = issue_pin(&pool, &blocked, issuance_for(user_id, "6666666666"))
            .await
            .expect_err("publication must fail");
        assert!(
            matches!(err, IssuePinError::Publish(_)),
            "expected a publication failure, got {err:?}"
        );

        let dir = tmp.path().join("recovery");
        let outcome = issue_pin(&pool, &dir, issuance_for(user_id, "7777777777"))
            .await
            .expect("the next issuance must not be locked out");
        assert_eq!(outcome, PinIssueOutcome::Published);
        assert_published_pin_matches_active_row(&pool, &dir, user_id).await;
    }
}
