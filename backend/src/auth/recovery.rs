//! Forgot-password recovery: CSPRNG PIN generation and the operator-readable
//! per-user host file.
//!
//! # Tier 2 — security-critical
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
//! expiry sweeps, never a cleartext PIN with no consuming row.

use std::fs::{self, OpenOptions, Permissions};
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use time::OffsetDateTime;
use uuid::Uuid;

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

/// Write the clear PIN, target email, and expiry to `<dir>/<user_id>.pin` with
/// mode 0600, creating `dir` (mode 0700) if absent and replacing any prior file
/// for the user. Permissions are enforced after open so an existing file/dir
/// with looser perms is corrected.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory or file cannot be created or
/// written.
pub fn write_pin_file(
    dir: &Path,
    user_id: Uuid,
    email: &str,
    pin: &str,
    expires_at: OffsetDateTime,
) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    let path = pin_file_path(dir, user_id);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    writeln!(file, "Reverie password recovery")?;
    writeln!(file, "email: {email}")?;
    writeln!(file, "pin: {pin}")?;
    writeln!(file, "expires_at: {expires_at}")?;
    Ok(())
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
        let dir = std::env::temp_dir().join(format!("reverie-recovery-test-{}", generate_pin()));
        let user_id = Uuid::new_v4();
        let expires = OffsetDateTime::now_utc() + time::Duration::minutes(15);

        write_pin_file(&dir, user_id, "user@example.com", "1234567890", expires).expect("write");
        let path = pin_file_path(&dir, user_id);
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

        remove_pin_file(&dir, user_id).expect("remove");
        assert!(!path.exists(), "file removed on cleanup");
        // Idempotent: removing an absent file is success.
        remove_pin_file(&dir, user_id).expect("remove-absent is ok");
    }
}
