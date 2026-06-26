//! Argon2id password hashing and verification for local accounts.
//!
//! # Tier 2 — security-critical
//!
//! [`argon2::Argon2::default`] is Argon2id with the RustCrypto-recommended parameters
//! (m=19456 KiB, t=2, p=1). Hashes are PHC strings: the algorithm, parameters,
//! salt, and digest travel together, so a future parameter change verifies old
//! hashes without a schema migration. Verification is constant-time internally.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

/// Hash a password into an Argon2id PHC string with a fresh random salt.
///
/// # Errors
///
/// Returns [`argon2::password_hash::Error`] if hashing fails (e.g. an invalid
/// parameter set); not expected with [`argon2::Argon2::default`].
pub fn hash_password(password: &[u8]) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password, &salt)?
        .to_string())
}

/// Verify a password against a stored Argon2id PHC string. `Ok(())` on a match.
///
/// # Errors
///
/// Returns [`argon2::password_hash::Error`] when the password does not match or
/// the stored hash is not a well-formed PHC string. Callers MUST treat both as
/// an identical generic failure (no enumeration); never branch user-visible
/// behaviour on which error occurred.
pub fn verify_password(password: &[u8], phc: &str) -> Result<(), argon2::password_hash::Error> {
    Argon2::default().verify_password(password, &PasswordHash::new(phc)?)
}

/// Process-stable dummy PHC, hashed once on first use from random bytes. Built at
/// runtime so it is always a well-formed PHC under the current parameters (no
/// hand-authored base64 to drift) and carries no hard-coded secret.
///
/// THREAT (CWE-208, timing side channel): this hash IS the anti-enumeration
/// timing control. An empty or malformed value would make [`verify_against_dummy`]
/// return in microseconds, so the no-account path would run measurably faster
/// than the wrong-password path and leak account existence. The input plaintext is
/// never recovered or compared against a credential, so it is random rather than
/// fixed; hashing it with default Argon2 params cannot fail except on an
/// unrecoverable environment fault, so this fails loud at startup rather than
/// silently shipping the degraded control.
static DUMMY_PHC: LazyLock<String> = LazyLock::new(|| {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    #[allow(
        clippy::expect_used,
        reason = "DUMMY_PHC is the anti-enumeration timing control (CWE-208); a failure to hash random bytes with default Argon2 params is an unrecoverable startup fault. Failing loud here is correct, not silently disabling the control."
    )]
    let phc = hash_password(&secret)
        .expect("DUMMY_PHC: Argon2 must hash the anti-enumeration dummy secret");
    phc
});

/// Spend Argon2id-verification-equivalent work on a login attempt whose email
/// resolves to no account (or no local credential), then report the (always
/// false) match.
///
/// THREAT (account enumeration via timing): a generic *response* does not hide
/// account existence if the unknown-email path skips the expensive verify and
/// returns faster than the wrong-password path. The login handler calls this on
/// the no-account path so latency is independent of account existence (OWASP
/// Authentication Cheat Sheet: uniform response AND timing). The boolean is
/// always `false`; callers spend the work and ignore the value (it is not
/// `#[must_use]` precisely because discarding it is the intended use).
pub fn verify_against_dummy(password: &[u8]) -> bool {
    verify_password(password, &DUMMY_PHC).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_verifies() {
        let phc = hash_password(b"correct horse battery staple").expect("hash");
        assert!(phc.starts_with("$argon2id$"));
        verify_password(b"correct horse battery staple", &phc).expect("verify matches");
    }

    #[test]
    fn wrong_password_rejected() {
        let phc = hash_password(b"the right one").expect("hash");
        assert!(verify_password(b"the wrong one", &phc).is_err());
    }

    #[test]
    fn same_password_yields_distinct_phc() {
        let a = hash_password(b"same input").expect("hash a");
        let b = hash_password(b"same input").expect("hash b");
        assert_ne!(a, b, "fresh salt must make each hash distinct");
        verify_password(b"same input", &a).expect("a verifies");
        verify_password(b"same input", &b).expect("b verifies");
    }

    #[test]
    fn dummy_path_runs_a_verify_and_never_matches() {
        // The anti-enumeration helper must spend real verify work (the dummy
        // hashes successfully) yet authenticate nothing.
        assert!(!verify_against_dummy(b"anything at all"));
        assert!(
            PasswordHash::new(&DUMMY_PHC).is_ok(),
            "dummy must be a well-formed PHC so verify spends work"
        );
    }

    #[test]
    fn malformed_phc_is_an_error_not_a_panic() {
        assert!(verify_password(b"pw", "not-a-phc-string").is_err());
    }
}
