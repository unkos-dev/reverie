//! Argon2id password hashing and verification for local accounts.
//!
//! # Tier 2 — security-critical
//!
//! [`Argon2::default`] is Argon2id with the RustCrypto-recommended parameters
//! (m=19456 KiB, t=2, p=1). Hashes are PHC strings: the algorithm, parameters,
//! salt, and digest travel together, so a future parameter change verifies old
//! hashes without a schema migration. Verification is constant-time internally.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

/// Hash a password into an Argon2id PHC string with a fresh random salt.
///
/// # Errors
///
/// Returns [`argon2::password_hash::Error`] if hashing fails (e.g. an invalid
/// parameter set); not expected with [`Argon2::default`].
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

/// Process-stable dummy PHC, hashed once on first use. Built at runtime so it is
/// always a well-formed PHC under the current parameters (no hand-authored
/// base64 to drift). If the one-time hash somehow failed it is left empty, which
/// makes [`verify_against_dummy`] error out immediately; that is acceptable
/// because the dummy only narrows a timing side channel and the generic response
/// is the primary control.
static DUMMY_PHC: LazyLock<String> =
    LazyLock::new(|| hash_password(b"reverie-anti-enumeration-dummy-secret").unwrap_or_default());

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
