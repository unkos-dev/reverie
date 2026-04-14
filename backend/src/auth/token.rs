use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64ct::{Base64UrlUnpadded, Encoding};

/// Generate a cryptographically random device token (32 bytes, base64url).
/// Returns (plaintext_token, argon2_hash).
pub fn generate_device_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let plaintext = Base64UrlUnpadded::encode_string(&bytes);
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .expect("argon2 hash failed")
        .to_string();
    (plaintext, hash)
}

/// Verify a plaintext token against a stored argon2 hash.
pub fn verify_device_token(plaintext: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_device_token_format() {
        let (plaintext, hash) = generate_device_token();
        // 32 bytes base64url unpadded = 43 chars
        assert_eq!(plaintext.len(), 43);
        // Hash should be a valid PHC string starting with $argon2
        assert!(hash.starts_with("$argon2"));
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
