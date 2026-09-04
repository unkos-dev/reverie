//! Password policy enforcement for local accounts.
//!
//! Combines three NIST SP 800-63B / OWASP ASVS V2.1 controls behind a single
//! [`enforce`](crate::auth::password_policy::enforce) entry point so every credential-setting path (registration,
//! admin create, admin reset, self-service change) applies the same gate:
//!
//! 1. Length bounds: a minimum floor and a maximum cap. The cap is a DoS guard,
//!    not a composition rule (NIST forbids composition rules but permits a cap).
//! 2. A zxcvbn strength floor, scored 0..=4, rejecting predictable passwords.
//! 3. An HIBP Pwned Passwords k-anonymity breach check, fail-open.
//!
//! # Tier 2 — security-critical
//!
//! This module is Tier 2 under the comment policy
//! (`docs/adr/0004-tiered-comment-policy-for-an-open-source-codebase.md`); `// THREAT:` annotations mark
//! the non-obvious mitigations. The candidate password is never logged.
//!
//! See `adr/2026-06-30-password-policy-hibp-zxcvbn.md`.

use std::fmt::Write as _;

use sha1::{Digest, Sha1};

use crate::config::Config;

/// The HTTP header that asks HIBP to pad its response with synthetic entries so
/// the on-the-wire response size does not reveal whether the queried prefix had
/// any real hits. Padded entries carry a count of 0 and are ignored.
const HIBP_PADDING_HEADER: &str = "Add-Padding";

/// Resolved password policy: a small owned view of the relevant [`Config`]
/// fields so callers pass this rather than the whole config, and tests can build
/// one directly.
pub struct PasswordPolicy {
    /// Minimum length in characters (Unicode scalar values).
    pub min_length: usize,
    /// Maximum length in characters. A denial-of-service cap, not a composition
    /// rule.
    pub max_length: usize,
    /// Minimum acceptable zxcvbn score, 0..=4. A password scoring below this is
    /// rejected as too weak.
    pub min_zxcvbn_score: u8,
    /// Whether the HIBP breach check runs at all (operator opt-out).
    pub breach_check_enabled: bool,
    /// Base URL of the HIBP Pwned Passwords range API; the 5-char prefix is
    /// appended as a path segment.
    pub breach_check_url: String,
}

impl PasswordPolicy {
    /// Build the policy view from resolved configuration.
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self {
            min_length: config.password_min_length,
            max_length: config.password_max_length,
            min_zxcvbn_score: config.password_min_zxcvbn_score,
            breach_check_enabled: config.password_breach_check_enabled,
            breach_check_url: config.password_breach_check_url.clone(),
        }
    }
}

/// Why a candidate password was rejected. The `Display` text is surfaced
/// verbatim as the 422 problem-detail the frontend renders, so each variant
/// carries actionable, non-enumerating guidance.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// Shorter than [`PasswordPolicy::min_length`].
    #[error("password must be at least {min} characters")]
    TooShort {
        /// The configured minimum.
        min: usize,
    },
    /// Longer than [`PasswordPolicy::max_length`].
    #[error("password must be at most {max} characters")]
    TooLong {
        /// The configured maximum.
        max: usize,
    },
    /// Below the configured zxcvbn floor. Carries the estimator's feedback.
    #[error("{0}")]
    TooWeak(String),
    /// Found in the HIBP Pwned Passwords corpus.
    #[error(
        "this password has appeared in a known data breach; choose a password you have not used elsewhere"
    )]
    Breached,
}

/// Estimate password strength with zxcvbn, penalising any candidate that
/// embeds one of `user_inputs` (e.g. the email or display name).
///
/// Returns the score (0..=4) and any human-readable feedback the estimator
/// produced (a warning plus suggestions), flattened to strings.
#[must_use]
pub fn check_strength(password: &str, user_inputs: &[&str]) -> (u8, Vec<String>) {
    let entropy = zxcvbn::zxcvbn(password, user_inputs);
    let score = u8::from(entropy.score());

    let mut feedback = Vec::new();
    if let Some(fb) = entropy.feedback() {
        if let Some(warning) = fb.warning() {
            feedback.push(warning.to_string());
        }
        feedback.extend(fb.suggestions().iter().map(ToString::to_string));
    }
    (score, feedback)
}

/// Query the HIBP Pwned Passwords range API for `password` via k-anonymity.
///
/// SHA-1s the candidate (HIBP's protocol mandates SHA-1, unrelated to the
/// device-token SHA-256 path), uppercases the hex, and sends only the first
/// five characters as a path segment; the 35-char suffix is matched locally.
///
/// THREAT (information disclosure): only the 5-char prefix leaves the process,
/// so the full hash — and thus the password — is never transmitted. The
/// `Add-Padding` header masks the real hit count via padded synthetic entries
/// (count 0), which are discarded here.
///
/// THREAT (availability): fails OPEN. Any transport error, non-2xx status
/// (e.g. a 429/503 HIBP outage), or unreadable body returns `false` (treat as
/// not-breached) and emits a `warn`, so a transient HIBP problem never blocks
/// logins or account creation on an otherwise-healthy instance. The candidate
/// password is never logged.
pub async fn check_breached(client: &reqwest::Client, password: &str, base_url: &str) -> bool {
    let digest = Sha1::digest(password.as_bytes());
    let hash = digest
        .iter()
        .fold(String::with_capacity(digest.len() * 2), |mut acc, byte| {
            // write! to a String is infallible; .ok() discards the unreachable Err.
            write!(acc, "{byte:02X}").ok();
            acc
        });
    let (prefix, suffix) = hash.split_at(5);
    let url = format!("{}/{prefix}", base_url.trim_end_matches('/'));

    let response = match client
        .get(&url)
        .header(HIBP_PADDING_HEADER, "true")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "HIBP breach check unreachable; failing open");
            return false;
        }
    };

    // THREAT (availability): do NOT use error_for_status() here. A 429/503 must
    // fail open, not closed; only a 2xx body is parsed for the suffix match.
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "HIBP breach check non-2xx; failing open");
        return false;
    }

    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(%error, "HIBP breach response unreadable; failing open");
            return false;
        }
    };

    // Each line is `SUFFIX:count`. A real hit has count > 0; padding entries
    // carry count 0 and must not register as a breach.
    body.lines().any(|line| {
        let mut parts = line.splitn(2, ':');
        let candidate = parts.next().unwrap_or_default();
        if !candidate.eq_ignore_ascii_case(suffix) {
            return false;
        }
        // Only the row matching our hash suffix decides the verdict. A malformed
        // count here would otherwise read as "not breached" with no trace, so log
        // it and stay fail-open: an unreadable count is not a confirmed breach.
        parts
            .next()
            .map(str::trim)
            .and_then(|c| c.parse::<u64>().ok())
            .map_or_else(
                || {
                    tracing::warn!(
                        "HIBP returned an unparsable count for the matching suffix; failing open"
                    );
                    false
                },
                |count| count > 0,
            )
    })
}

/// Enforce the full policy against a candidate password.
///
/// THREAT (availability/DoS): the max-length cap is checked FIRST, before any
/// zxcvbn or HIBP work, because both costs grow with input length and the
/// registration path reaches this unauthenticated. A multi-megabyte password is
/// a CPU/network amplification vector that this bound closes cheaply.
///
/// `user_inputs` are forwarded to zxcvbn so a password containing the user's own
/// email or name scores lower. The breach check runs only when enabled.
pub async fn enforce(
    password: &str,
    user_inputs: &[&str],
    policy: &PasswordPolicy,
    breach_client: &reqwest::Client,
) -> Result<(), PolicyError> {
    let length = password.chars().count();
    if length > policy.max_length {
        return Err(PolicyError::TooLong {
            max: policy.max_length,
        });
    }
    if length < policy.min_length {
        return Err(PolicyError::TooShort {
            min: policy.min_length,
        });
    }

    let (score, feedback) = check_strength(password, user_inputs);
    if score < policy.min_zxcvbn_score {
        let detail = if feedback.is_empty() {
            "password is too weak; choose a longer, less predictable password".to_string()
        } else {
            feedback.join(" ")
        };
        return Err(PolicyError::TooWeak(detail));
    }

    if policy.breach_check_enabled
        && check_breached(breach_client, password, &policy.breach_check_url).await
    {
        return Err(PolicyError::Breached);
    }

    Ok(())
}

/// Convenience for request handlers: build the [`PasswordPolicy`] and the breach
/// client from `config`, then [`enforce`]. The breach client carries the SSRF
/// resolver (via [`crate::services::enrichment::http::api_client`]) so the
/// operator-overridable HIBP URL cannot be aimed at an internal address.
pub async fn enforce_from_config(
    config: &Config,
    password: &str,
    user_inputs: &[&str],
) -> Result<(), PolicyError> {
    let policy = PasswordPolicy::from_config(config);
    let client = crate::services::enrichment::http::api_client(&config.user_agent());
    enforce(password, user_inputs, &policy, &client).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn policy(url: String) -> PasswordPolicy {
        PasswordPolicy {
            min_length: 8,
            max_length: 256,
            min_zxcvbn_score: 2,
            breach_check_enabled: true,
            breach_check_url: url,
        }
    }

    #[test]
    fn check_strength_rejects_a_trivial_password() {
        let (score, feedback) = check_strength("password", &[]);
        assert!(score < 2, "expected a weak score, got {score}");
        assert!(!feedback.is_empty(), "weak password should carry feedback");
    }

    #[test]
    fn check_strength_accepts_a_strong_passphrase() {
        let (score, _) = check_strength("correct-horse-battery-staple-7!", &[]);
        assert!(score >= 3, "expected a strong score, got {score}");
    }

    #[tokio::test]
    async fn check_breached_reports_a_hit() {
        let server = MockServer::start().await;
        // SHA-1 of "password" is 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8;
        // prefix 5BAA6, suffix 1E4C9B93F3F0682250B6CF8331B7EE68FD8.
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:42\nFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF:0";
        Mock::given(method("GET"))
            .and(path_regex(r"^/5BAA6$"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
        )]
        let client = reqwest::Client::new();
        assert!(check_breached(&client, "password", &server.uri()).await);
    }

    #[tokio::test]
    async fn check_breached_padding_entries_do_not_register() {
        let server = MockServer::start().await;
        // The real suffix is present but with count 0 (padding): not a breach.
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:0";
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
        )]
        let client = reqwest::Client::new();
        assert!(!check_breached(&client, "password", &server.uri()).await);
    }

    #[tokio::test]
    async fn check_breached_malformed_count_for_matching_suffix_fails_open() {
        let server = MockServer::start().await;
        // The matching suffix is present but its count is not a number. An
        // unreadable matching row must fail open (not breached), never silently
        // miss without a trace.
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:not-a-number";
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
        )]
        let client = reqwest::Client::new();
        assert!(!check_breached(&client, "password", &server.uri()).await);
    }

    #[tokio::test]
    async fn check_breached_reports_a_miss() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:9"),
            )
            .mount(&server)
            .await;

        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
        )]
        let client = reqwest::Client::new();
        assert!(!check_breached(&client, "a-passphrase-not-in-this-mock", &server.uri()).await);
    }

    #[tokio::test]
    async fn check_breached_fails_open_when_unreachable() {
        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against a dead loopback port is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): no WAF in the path"
        )]
        let client = reqwest::Client::new();
        // Reserved TEST-NET-1 address; the connection cannot succeed.
        assert!(!check_breached(&client, "password", "http://192.0.2.1:1").await);
    }

    #[tokio::test]
    async fn check_breached_fails_open_on_non_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        #[expect(
            clippy::disallowed_methods,
            reason = "bare Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
        )]
        let client = reqwest::Client::new();
        assert!(!check_breached(&client, "password", &server.uri()).await);
    }

    #[tokio::test]
    async fn enforce_rejects_over_max_length_before_any_other_work() {
        // breach_check_url points nowhere reachable; if the length guard did not
        // short-circuit first, the breach call would still fail open, so this
        // test asserts the TooLong variant specifically.
        let over = "a".repeat(300);
        let result = enforce(
            &over,
            &[],
            &policy("http://192.0.2.1:1".to_string()),
            &dummy_client(),
        )
        .await;
        assert!(matches!(result, Err(PolicyError::TooLong { max: 256 })));
    }

    #[tokio::test]
    async fn enforce_rejects_too_short() {
        let result = enforce(
            "short",
            &[],
            &policy("http://192.0.2.1:1".to_string()),
            &dummy_client(),
        )
        .await;
        assert!(matches!(result, Err(PolicyError::TooShort { min: 8 })));
    }

    #[tokio::test]
    async fn enforce_rejects_weak_password_with_feedback() {
        let result = enforce(
            "password",
            &[],
            &policy("http://192.0.2.1:1".to_string()),
            &dummy_client(),
        )
        .await;
        match result {
            Err(PolicyError::TooWeak(detail)) => assert!(!detail.is_empty()),
            other => panic!("expected TooWeak, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn enforce_allows_a_strong_password_when_breach_check_fails_open() {
        // breach URL unreachable -> fail open -> a strong password is accepted.
        let result = enforce(
            "correct-horse-battery-staple-7!",
            &[],
            &policy("http://192.0.2.1:1".to_string()),
            &dummy_client(),
        )
        .await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[tokio::test]
    async fn enforce_rejects_a_breached_password() {
        let server = MockServer::start().await;
        // SHA-1 of "correct-horse-battery-staple-7!" is
        // A07F01DC7C1181DF05D6B47FC8F0DC0826A173C1: prefix A07F0, the rest the
        // suffix. Reporting that suffix as seen means a strong-but-breached
        // password is rejected by enforce(), not accepted on strength alone.
        let body = "1DC7C1181DF05D6B47FC8F0DC0826A173C1:9";
        Mock::given(method("GET"))
            .and(path_regex(r"^/A07F0$"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let result = enforce(
            "correct-horse-battery-staple-7!",
            &[],
            &policy(server.uri()),
            &dummy_client(),
        )
        .await;
        assert!(
            matches!(result, Err(PolicyError::Breached)),
            "expected Breached, got {result:?}"
        );
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "bare Client::new() in tests is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md)"
    )]
    fn dummy_client() -> reqwest::Client {
        reqwest::Client::new()
    }
}
