//! Argon2id password hashing and verification for local accounts.
//!
//! # Tier 2: security-critical
//!
//! [`argon2::Argon2::default`] is Argon2id with the RustCrypto-recommended parameters
//! (m=19456 KiB, t=2, p=1). Hashes are PHC strings: the algorithm, parameters,
//! salt, and digest travel together, so a future parameter change verifies old
//! hashes without a schema migration. Verification is constant-time internally.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinError;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use rand::RngExt as _;
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;

/// Maximum time a request waits for hashing capacity.
pub const HASH_WAIT_LIMIT: Duration = Duration::from_secs(30);

/// Process-wide request-path Argon2 capacity.
#[derive(Clone)]
pub struct HashLimiter {
    semaphore: Arc<Semaphore>,
    wait_limit: Duration,
    #[cfg(test)]
    before_compute: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Hashing capacity was unavailable within the wait limit.
#[derive(Debug)]
pub struct HashBusy;

/// Failure to compute a password hash.
#[derive(Debug, thiserror::Error)]
pub enum HashError {
    /// Argon2 rejected the hashing operation.
    #[error("password hashing failed: {0}")]
    Argon2(#[from] argon2::password_hash::Error),
    /// The blocking task did not complete.
    #[error("password hashing task failed: {0}")]
    Join(#[from] JoinError),
}

/// Capacity retained until both the caller and its blocking computation finish.
pub struct HashPermit {
    permit: Arc<OwnedSemaphorePermit>,
    #[cfg(test)]
    before_compute: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl HashLimiter {
    /// Construct the process-wide limit.
    #[must_use]
    pub fn new(concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            wait_limit: HASH_WAIT_LIMIT,
            #[cfg(test)]
            before_compute: None,
        }
    }

    /// Override the wait limit for deterministic tests.
    #[cfg(test)]
    #[must_use]
    pub const fn with_wait_limit(mut self, wait_limit: Duration) -> Self {
        self.wait_limit = wait_limit;
        self
    }

    /// Run a test hook inside every blocking computation.
    #[cfg(test)]
    #[must_use]
    pub fn with_before_compute(mut self, hook: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.before_compute = Some(hook);
        self
    }

    /// Wait in FIFO order for hashing capacity.
    ///
    /// # Errors
    /// Returns `HashBusy` if the wait expires or the semaphore is closed.
    pub async fn acquire(&self) -> Result<HashPermit, HashBusy> {
        match tokio::time::timeout(self.wait_limit, Arc::clone(&self.semaphore).acquire_owned())
            .await
        {
            Ok(Ok(permit)) => Ok(HashPermit {
                permit: Arc::new(permit),
                #[cfg(test)]
                before_compute: self.before_compute.clone(),
            }),
            Ok(Err(error)) => {
                tracing::error!(%error, "authentication hashing semaphore closed");
                Err(HashBusy)
            }
            Err(_) => Err(HashBusy),
        }
    }

    /// Acquire capacity for one password hash.
    ///
    /// # Errors
    /// Returns admission failure outside the computation result.
    pub async fn hash(&self, password: String) -> Result<Result<String, HashError>, HashBusy> {
        Ok(self.acquire().await?.hash(password).await)
    }
}

impl HashPermit {
    /// Compute a hash without occupying an async worker.
    ///
    /// # Errors
    /// Returns the Argon2 or blocking-task failure.
    pub async fn hash(&self, password: String) -> Result<String, HashError> {
        let permit = Arc::clone(&self.permit);
        #[cfg(test)]
        let hook = self.before_compute.clone();
        Ok(tokio::task::spawn_blocking(move || {
            // THREAT: caller cancellation must not release a running hash's capacity.
            let _permit = permit;
            #[cfg(test)]
            if let Some(hook) = hook {
                hook();
            }
            hash_password(password.as_bytes())
        })
        .await??)
    }

    /// Verify an owned credential on the blocking pool.
    ///
    /// # Errors
    /// Returns a blocking-task failure; malformed and mismatched hashes return false.
    pub async fn verify(&self, password: String, phc: String) -> Result<bool, JoinError> {
        let permit = Arc::clone(&self.permit);
        #[cfg(test)]
        let hook = self.before_compute.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            #[cfg(test)]
            if let Some(hook) = hook {
                hook();
            }
            verify_password(password.as_bytes(), &phc).is_ok()
        })
        .await
    }

    /// Spend dummy verification work on the blocking pool.
    ///
    /// # Errors
    /// Returns a blocking-task failure.
    pub async fn verify_dummy(&self, password: String) -> Result<(), JoinError> {
        let permit = Arc::clone(&self.permit);
        #[cfg(test)]
        let hook = self.before_compute.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            #[cfg(test)]
            if let Some(hook) = hook {
                hook();
            }
            verify_against_dummy(password.as_bytes());
        })
        .await
    }
}

/// Hash a password into an Argon2id PHC string with a fresh random salt.
///
/// # Errors
///
/// Returns [`argon2::password_hash::Error`] if hashing fails (e.g. an invalid
/// parameter set); not expected with [`argon2::Argon2::default`].
pub fn hash_password(password: &[u8]) -> Result<String, argon2::password_hash::Error> {
    Ok(Argon2::default().hash_password(password)?.to_string())
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
    let secret: [u8; 32] = UnwrapErr(SysRng).random();
    #[expect(
        clippy::expect_used,
        reason = "DUMMY_PHC is the anti-enumeration timing control (CWE-208); a failure to hash the dummy secret is an unrecoverable startup fault. Failing loud here is correct, not silently disabling the control."
    )]
    hash_password(&secret).expect("DUMMY_PHC: Argon2 must hash the anti-enumeration dummy secret")
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
#[expect(
    clippy::must_use_candidate,
    reason = "the boolean is always false; callers spend the work for its timing and discard the value by design"
)]
pub fn verify_against_dummy(password: &[u8]) -> bool {
    verify_password(password, &DUMMY_PHC).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocking_hook() -> (
        Arc<dyn Fn() + Send + Sync>,
        tokio::sync::mpsc::UnboundedReceiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (release, wait) = std::sync::mpsc::channel();
        let wait = std::sync::Mutex::new(wait);
        (
            Arc::new(move || {
                entered.send(()).unwrap();
                wait.lock().unwrap().recv().unwrap();
            }),
            receiver,
            release,
        )
    }

    #[tokio::test]
    async fn auth_hash_active_bound() {
        let (hook, mut entered, release) = blocking_hook();
        let limiter = HashLimiter::new(2).with_before_compute(hook);
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let limiter = limiter.clone();
            tasks.push(tokio::spawn(async move {
                limiter.hash("password".into()).await
            }));
        }
        entered.recv().await.unwrap();
        entered.recv().await.unwrap();
        assert_eq!(limiter.semaphore.available_permits(), 0);
        assert!(entered.try_recv().is_err());
        release.send(()).unwrap();
        entered.recv().await.unwrap();
        release.send(()).unwrap();
        release.send(()).unwrap();
        for task in tasks {
            task.await.unwrap().unwrap().unwrap();
        }
        assert_eq!(limiter.semaphore.available_permits(), 2);
    }

    #[tokio::test]
    async fn auth_hash_wait_limit_busy() {
        let limiter = HashLimiter::new(1).with_wait_limit(Duration::from_millis(10));
        let _held = limiter.acquire().await.unwrap();
        assert!(matches!(limiter.acquire().await, Err(HashBusy)));
    }

    #[tokio::test]
    async fn auth_hash_cancel_keeps_permit() {
        let (hook, mut entered, release) = blocking_hook();
        let limiter = HashLimiter::new(1).with_before_compute(hook);
        let task_limiter = limiter.clone();
        let task = tokio::spawn(async move { task_limiter.hash("password".into()).await });
        entered.recv().await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(limiter.semaphore.available_permits(), 0);
        release.send(()).unwrap();
        let permit = tokio::time::timeout(Duration::from_secs(5), limiter.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(limiter.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn auth_hash_panic_releases_permit() {
        let limiter = HashLimiter::new(1).with_before_compute(Arc::new(|| panic!("hash hook")));
        let permit = limiter.acquire().await.unwrap();
        assert!(
            permit
                .verify("password".into(), "malformed".into())
                .await
                .unwrap_err()
                .is_panic()
        );
        drop(permit);
        assert_eq!(limiter.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn auth_hash_results_match_kernels() {
        let limiter = HashLimiter::new(1);
        let phc = limiter.hash("password".into()).await.unwrap().unwrap();
        verify_password(b"password", &phc).unwrap();
        let permit = limiter.acquire().await.unwrap();
        assert!(permit.verify("password".into(), phc.clone()).await.unwrap());
        assert!(!permit.verify("wrong".into(), phc).await.unwrap());
        assert!(
            !permit
                .verify("password".into(), "malformed".into())
                .await
                .unwrap()
        );
        permit.verify_dummy("password".into()).await.unwrap();
        drop(permit);
    }

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
