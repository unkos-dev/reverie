//! Per-source (per-IP) login rate limiting.
//!
//! # Tier 2: security-critical
//!
//! A keyed [`governor`] limiter caps login/recovery attempts per client IP. It
//! is the hard blocker against credential-stuffing and unknown-email spray; the
//! DB-backed per-account backoff ([`crate::models::login_throttle`]) is the
//! IP-independent backstop. The two are complementary: the per-IP limiter bounds
//! how fast an attacker can probe, the per-account backoff escalates against a
//! single targeted account.
//!
//! THREAT (client-IP spoofing): behind a reverse proxy the TCP peer is the
//! proxy, not the client, so a naive per-peer limiter would throttle the whole
//! deployment as one key. The standard fix is a configurable trusted-proxy
//! boundary: [`client_ip`](crate::auth::rate_limit::client_ip) trusts a
//! forwarded-for header ONLY when the operator has explicitly named one
//! (`trusted_client_ip_header`), because an
//! unauthenticated forwarded header is attacker-spoofable (RFC 7239 security
//! considerations). With no header configured it keys on the TCP peer.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};

/// Never-rejecting extractor for the TCP peer address.
///
/// `Option<ConnectInfo<_>>` does not work as an axum 0.8 extractor (it requires
/// `OptionalFromRequestParts`, which `ConnectInfo` does not implement), so this
/// reads the `ConnectInfo` out of the request extensions directly and yields
/// `None` when absent. The test harness (`axum_test`) supplies no peer, so a
/// non-optional `ConnectInfo` extractor would 500 every request; the per-account
/// backoff is the IP-independent backstop when the peer is unknown.
pub struct PeerAddr(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for PeerAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0),
        ))
    }
}

/// Per-IP login limiter. Keyed on the resolved client [`IpAddr`].
pub type LoginLimiter = DefaultKeyedRateLimiter<IpAddr>;

/// Build a per-IP login limiter admitting `per_min` attempts per minute per key
/// (burst equal to the rate). `per_min` is [`NonZeroU32`] so a zero quota (which
/// would lock everyone out) is unrepresentable; config validation enforces the
/// non-zero invariant at the boundary.
pub fn build_login_limiter(per_min: NonZeroU32) -> Arc<LoginLimiter> {
    Arc::new(RateLimiter::keyed(Quota::per_minute(per_min)))
}

/// Resolve the client IP for rate-limiting from the TCP `peer` and, when the
/// operator has opted in by naming a `trusted_header`, that forwarded-for
/// header.
///
/// THREAT: the forwarded header is honoured ONLY when `trusted_header` is
/// `Some` (operator opt-in). An unauthenticated forwarded header is
/// attacker-spoofable, so defaulting to trust it would let any client forge its
/// rate-limit key. With no trusted header configured this returns the TCP peer.
/// When the header is configured but absent or unparsable, it falls back to the
/// peer rather than failing open. The leftmost token of a comma-separated list
/// is the originating client (`X-Forwarded-For` convention); a value that does
/// not parse as a bare `IpAddr` (e.g. an RFC 7239 `for=` form) falls back to the
/// peer.
pub fn client_ip(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted_header: Option<&str>,
) -> Option<IpAddr> {
    if let Some(name) = trusted_header
        && let Some(forwarded) = headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .and_then(|first| first.parse::<IpAddr>().ok())
    {
        return Some(forwarded);
    }
    peer
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn limiter_blocks_over_quota_per_key() {
        let limiter = build_login_limiter(NonZeroU32::new(1).expect("non-zero"));
        let one = ip(10, 0, 0, 1);
        let two = ip(10, 0, 0, 2);

        assert!(limiter.check_key(&one).is_ok(), "first attempt admitted");
        assert!(
            limiter.check_key(&one).is_err(),
            "second attempt over quota"
        );
        assert!(
            limiter.check_key(&two).is_ok(),
            "a different IP has an independent budget"
        );
    }

    #[test]
    fn client_ip_uses_peer_when_no_trusted_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "9.9.9.9".parse().expect("header"));
        // Header present but untrusted (None) → ignored, peer wins.
        assert_eq!(
            client_ip(&headers, Some(ip(127, 0, 0, 1)), None),
            Some(ip(127, 0, 0, 1))
        );
    }

    #[test]
    fn client_ip_trusts_configured_header_first_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "9.9.9.9, 10.0.0.1".parse().expect("header"),
        );
        assert_eq!(
            client_ip(&headers, Some(ip(127, 0, 0, 1)), Some("x-forwarded-for")),
            Some(ip(9, 9, 9, 9)),
            "leftmost forwarded token is the originating client"
        );
    }

    #[test]
    fn client_ip_falls_back_to_peer_on_missing_or_bad_header() {
        let headers = HeaderMap::new();
        // Trusted header configured but absent → peer.
        assert_eq!(
            client_ip(&headers, Some(ip(127, 0, 0, 1)), Some("x-forwarded-for")),
            Some(ip(127, 0, 0, 1))
        );

        let mut garbage = HeaderMap::new();
        garbage.insert("x-forwarded-for", "not-an-ip".parse().expect("header"));
        assert_eq!(
            client_ip(&garbage, Some(ip(127, 0, 0, 1)), Some("x-forwarded-for")),
            Some(ip(127, 0, 0, 1)),
            "unparsable forwarded value falls back to peer"
        );
    }

    #[test]
    fn client_ip_returns_none_when_no_peer_and_no_trusted_header() {
        let headers = HeaderMap::new();
        assert_eq!(client_ip(&headers, None, None), None);
    }
}
