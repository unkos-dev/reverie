//! AES-256-GCM sealing of queued mail payloads.

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, Generate, Payload},
};
use base64ct::{Base64UrlUnpadded, Encoding};
use secrecy::{ExposeSecret, SecretBox};
use uuid::Uuid;

use super::FailureCategory;

const KEY_LEN: usize = 32;

/// The 32-byte queue encryption key. Its `Debug` output is redacted and it
/// has no `Clone` or `Serialize`.
#[derive(Debug)]
pub struct QueueKey(SecretBox<[u8; KEY_LEN]>);

/// The key text was not exactly 32 bytes of canonical unpadded base64url.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("queue key must be 32 bytes of base64url without padding")]
pub struct QueueKeyError;

impl QueueKey {
    /// Decode the operator-supplied key.
    ///
    /// # Errors
    ///
    /// Returns [`QueueKeyError`] unless `encoded` is the canonical unpadded
    /// base64url form of exactly 32 bytes.
    pub fn parse(encoded: &str) -> Result<Self, QueueKeyError> {
        let mut decoded_len = 0;
        let key = SecretBox::<[u8; KEY_LEN]>::init_with_mut(|buffer| {
            if let Ok(decoded) = Base64UrlUnpadded::decode(encoded, buffer) {
                decoded_len = decoded.len();
            }
        });
        if decoded_len == KEY_LEN {
            Ok(Self(key))
        } else {
            Err(QueueKeyError)
        }
    }

    /// Wrap raw key bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    fn cipher(&self) -> Result<Aes256Gcm, CryptoError> {
        Aes256Gcm::new_from_slice(self.0.expose_secret()).map_err(|_| CryptoError::Seal)
    }
}

/// A sealed payload: the fresh nonce and the ciphertext with its tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    /// The 96-bit nonce generated for this encryption.
    pub nonce: Vec<u8>,
    /// The ciphertext followed by the authentication tag.
    pub ciphertext: Vec<u8>,
}

/// Why sealing or opening failed. The variants carry no key, payload or
/// library detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    /// The payload could not be encrypted.
    #[error("payload could not be sealed")]
    Seal,
    /// Authentication failed or the stored fields are malformed. There is no
    /// fallback to plaintext or another algorithm.
    #[error("payload could not be opened")]
    Open,
}

impl CryptoError {
    /// The category recorded against the job.
    #[must_use]
    pub const fn category(self) -> FailureCategory {
        FailureCategory::PayloadUnreadable
    }
}

/// The job UUID bytes followed by the purpose, so a payload cannot be moved to
/// another job or purpose.
fn associated_data(job_id: Uuid, purpose: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(16 + purpose.len());
    aad.extend_from_slice(job_id.as_bytes());
    aad.extend_from_slice(purpose.as_bytes());
    aad
}

/// Encrypt `plaintext` for a job under a fresh random nonce.
///
/// # Errors
///
/// Returns [`CryptoError::Seal`] when the system random source or the cipher fails.
pub fn seal(
    key: &QueueKey,
    job_id: Uuid,
    purpose: &str,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    let cipher = key.cipher()?;
    let nonce = aes_gcm::Nonce::<<Aes256Gcm as AeadCore>::NonceSize>::try_generate()
        .map_err(|_| CryptoError::Seal)?;
    let aad = associated_data(job_id, purpose);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| CryptoError::Seal)?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

/// Decrypt and authenticate a stored payload.
///
/// # Errors
///
/// Returns [`CryptoError::Open`] for a wrong key, altered ciphertext, nonce or
/// job/purpose binding, or malformed stored fields.
pub fn open(
    key: &QueueKey,
    job_id: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = key.cipher().map_err(|_| CryptoError::Open)?;
    let nonce = aes_gcm::Nonce::<<Aes256Gcm as AeadCore>::NonceSize>::try_from(nonce)
        .map_err(|_| CryptoError::Open)?;
    let aad = associated_data(job_id, purpose);
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| CryptoError::Open)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENCODED: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    const PLAINTEXT: &[u8] = b"recipient@example.com\r\nsecret-body-marker";

    fn key() -> QueueKey {
        QueueKey::parse(ENCODED).unwrap()
    }

    fn sealed(job: Uuid) -> Sealed {
        seal(&key(), job, "email_recovery", PLAINTEXT).unwrap()
    }

    #[test]
    fn smtp_crypto_round_trip_recovers_plaintext() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        assert_eq!(sealed.nonce.len(), 12);
        assert_ne!(sealed.ciphertext, PLAINTEXT);
        let opened = open(
            &key(),
            job,
            "email_recovery",
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert_eq!(opened.unwrap(), PLAINTEXT);
    }

    #[test]
    fn smtp_crypto_empty_plaintext_round_trips() {
        let job = Uuid::new_v4();
        let sealed = seal(&key(), job, "notice", b"").unwrap();
        let opened = open(&key(), job, "notice", &sealed.nonce, &sealed.ciphertext).unwrap();
        assert_eq!(opened, Vec::<u8>::new());
    }

    #[test]
    fn smtp_crypto_fresh_nonce_per_encryption() {
        let job = Uuid::new_v4();
        let first = sealed(job);
        let second = sealed(job);
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn smtp_crypto_every_tampered_ciphertext_byte_is_refused() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        for index in 0..sealed.ciphertext.len() {
            let mut altered = sealed.ciphertext.clone();
            altered[index] ^= 0x01;
            let result = open(&key(), job, "email_recovery", &sealed.nonce, &altered);
            assert_eq!(result, Err(CryptoError::Open), "byte {index}");
        }
    }

    #[test]
    fn smtp_crypto_tampered_nonce_is_refused() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        for index in 0..sealed.nonce.len() {
            let mut altered = sealed.nonce.clone();
            altered[index] ^= 0x80;
            let result = open(&key(), job, "email_recovery", &altered, &sealed.ciphertext);
            assert_eq!(result, Err(CryptoError::Open), "byte {index}");
        }
    }

    #[test]
    fn smtp_crypto_malformed_nonce_and_truncated_ciphertext_are_refused() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        for nonce in [&[][..], &sealed.nonce[..11], &[0_u8; 13][..]] {
            let result = open(&key(), job, "email_recovery", nonce, &sealed.ciphertext);
            assert_eq!(result, Err(CryptoError::Open));
        }
        for len in [0, 1, 15, sealed.ciphertext.len() - 1] {
            let result = open(
                &key(),
                job,
                "email_recovery",
                &sealed.nonce,
                &sealed.ciphertext[..len],
            );
            assert_eq!(result, Err(CryptoError::Open), "length {len}");
        }
    }

    #[test]
    fn smtp_crypto_job_and_purpose_binding_is_authenticated() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        let other_job = open(
            &key(),
            Uuid::new_v4(),
            "email_recovery",
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert_eq!(other_job, Err(CryptoError::Open));
        for purpose in ["verification", "email_recover", "email_recovery ", ""] {
            let result = open(&key(), job, purpose, &sealed.nonce, &sealed.ciphertext);
            assert_eq!(result, Err(CryptoError::Open), "{purpose:?}");
        }
    }

    #[test]
    fn smtp_crypto_wrong_key_is_refused() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        let other = QueueKey::from_bytes([7; 32]);
        let result = open(
            &other,
            job,
            "email_recovery",
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert_eq!(result, Err(CryptoError::Open));
    }

    #[test]
    fn smtp_crypto_refusal_maps_to_payload_unreadable() {
        assert_eq!(
            CryptoError::Open.category(),
            FailureCategory::PayloadUnreadable
        );
        assert_eq!(
            FailureCategory::PayloadUnreadable.as_str(),
            "payload_unreadable"
        );
    }

    #[test]
    fn smtp_crypto_diagnostics_never_expose_key_or_payload() {
        let job = Uuid::new_v4();
        let sealed = sealed(job);
        let error = open(&key(), job, "email_recovery", &sealed.nonce, &[0; 20]).unwrap_err();
        let diagnostic = format!(
            "{:?} {:?} {error} {error:?} {:?} {QueueKeyError} {QueueKeyError:?}",
            key(),
            Some(key()),
            sealed.nonce,
        );
        assert!(!diagnostic.contains(ENCODED));
        assert!(!diagnostic.contains("secret-body-marker"));
        assert!(!format!("{:?}", key()).contains("1, 2, 3"));
        assert!(format!("{:?}", key()).contains("REDACTED"));
    }

    #[test]
    fn smtp_crypto_key_parse_accepts_only_canonical_32_bytes() {
        assert!(QueueKey::parse(ENCODED).is_ok());
        let encoded_zeros = Base64UrlUnpadded::encode_string(&[0_u8; 32]);
        assert!(QueueKey::parse(&encoded_zeros).is_ok());
        for bad in [
            "",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8A",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh+",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh9",
            " AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        ] {
            assert_eq!(QueueKey::parse(bad).err(), Some(QueueKeyError), "{bad:?}");
        }
    }
}
