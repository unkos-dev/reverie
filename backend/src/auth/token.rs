//! Device-token generation and constant-time verification for Reverie.
//!
//! Device tokens are long-lived credentials used by OPDS clients and other
//! automation that cannot participate in the interactive OIDC login flow.
//! A token is a cryptographically random 32-byte value encoded as
//! base64url (no padding). Only its SHA-256 hex digest is stored in the
//! database; the plaintext is shown to the user once at creation and never
//! persisted.
//!
//! # Threat model
//!
//! Storing only the hash means a database read (or dump) does not directly
//! yield usable credentials. An attacker with read access to the `device_tokens`
//! table would need to brute-force 256 bits of entropy to forge a token,
//! which is computationally infeasible.
//!
//! Verification uses `subtle::ConstantTimeEq` to compare the computed hash
//! against the stored hash; non-constant-time comparison would expose the
//! token prefix via timing side-channel. See [`crate::auth::token::verify_device_token`].

use std::fmt::Write as _;

use base64ct::{Base64UrlUnpadded, Encoding};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Prefix on every issued device-token credential.
///
/// Used as `{TOKEN_PREFIX}{token_id}.{secret}`
/// (Bearer) or `{TOKEN_PREFIX}{token_id}:{secret}` (Basic username/password
/// split). Distinctive and greppable so a leaked token is caught by
/// TruffleHog/gitleaks (already in CI); the `_pat_` marker mirrors GitHub's
/// `github_pat_` convention.
pub const TOKEN_PREFIX: &str = "rvpat_";

fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    digest
        .iter()
        .fold(String::with_capacity(digest.len() * 2), |mut s, b| {
            // write! to String via fmt::Write is infallible; .ok() discards the unreachable Err.
            write!(s, "{b:02x}").ok();
            s
        })
}

/// Generate a cryptographically random device token.
///
/// Fills 32 bytes from the OS CSPRNG, encodes as base64url (no padding, 43
/// characters), and returns both the plaintext and its SHA-256 hex digest.
///
/// The caller must store only the hash and present the plaintext to the user
/// exactly once. The hash is the value written to `device_tokens.token_hash`.
///
/// # Return value
///
/// Returns `(plaintext_token, sha256_hex_hash)`. The plaintext is 43
/// characters of base64url; the hash is 64 lowercase hex characters.
pub fn generate_device_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let plaintext = Base64UrlUnpadded::encode_string(&bytes);
    let hash = sha256_hex(plaintext.as_bytes());
    (plaintext, hash)
}

/// Verify a plaintext token against a stored SHA-256 hex digest.
///
/// Constant-time comparison via [`subtle::ConstantTimeEq`]: non-constant-time
/// comparison would expose the hash prefix via timing side-channel, allowing
/// an attacker to narrow valid-token guesses byte by byte.
///
/// Computes SHA-256 of `plaintext`, then compares the resulting 64-character
/// hex string against `hash` in constant time. Returns `true` only when the
/// digests match exactly.
///
/// Note: this function is called inside a full-iteration loop in
/// [`crate::auth::middleware::verify_basic`] to close a secondary timing
/// side-channel on token position within the user's token list.
pub fn verify_device_token(plaintext: &str, hash: &str) -> bool {
    let computed = sha256_hex(plaintext.as_bytes());
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_device_token_format() {
        let (plaintext, hash) = generate_device_token();
        // 32 bytes base64url unpadded = 43 chars
        assert_eq!(plaintext.len(), 43);
        // SHA-256 hex = 64 lowercase hex chars
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_correct_token() {
        let (plaintext, hash) = generate_device_token();
        assert!(verify_device_token(&plaintext, &hash));
    }

    #[test]
    fn verify_wrong_token() {
        let (_plaintext, hash) = generate_device_token();
        assert!(!verify_device_token("wrong-token", &hash));
    }

    #[test]
    fn verify_malformed_hash() {
        assert!(!verify_device_token("any-token", "not-a-valid-hash"));
    }
}
