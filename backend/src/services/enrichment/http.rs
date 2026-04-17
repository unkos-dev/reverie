//! SSRF-safe HTTP client factory for the metadata enrichment pipeline.
//!
//! Provides two pre-configured [`reqwest::Client`] instances:
//!
//! * [`api_client`] — lightweight client for metadata REST/GraphQL calls.  Plain
//!   redirect following with a 10-second timeout and a 5-hop redirect limit.
//!
//! * [`cover_client`] — client for fetching cover image URLs.  Every redirect hop
//!   is validated against a set of denied IP ranges to prevent SSRF attacks.
//!
//! # Design: sync DNS in a redirect callback
//!
//! reqwest's `redirect::Policy::custom` closure is **synchronous** — you cannot
//! `.await` inside it.  The approach chosen here is `std::net::ToSocketAddrs`
//! which performs a blocking OS-level DNS resolution.  This momentarily blocks the
//! Tokio thread, but is acceptable because:
//!
//! 1. Cover downloads are infrequent background operations.
//! 2. Most redirects point to CDN addresses already cached by the OS resolver.
//! 3. The only alternative (`block_on` a hickory future) would panic or deadlock
//!    inside an existing async runtime.
//!
//! If this ever becomes a bottleneck, the right fix is to pre-validate the URL
//! before handing it to reqwest (see [`validate_hop`]), removing the need for a
//! callback at all.

// These items are public API consumed by the enrichment pipeline.  They are not
// called from within this binary crate yet (the orchestrator wires them up after
// all Phase B agents complete), so dead_code is expected during integration.
#![allow(dead_code)]

use std::net::{IpAddr, Ipv6Addr, ToSocketAddrs};
use std::time::Duration;

use reqwest::redirect;
use tracing::warn;

// ── Public error type ──────────────────────────────────────────────────────

/// Reason a redirect hop was rejected.
#[derive(Debug)]
pub enum HopError {
    /// The resolved IP is in a denied range.
    DenyListed(IpAddr),
    /// DNS resolution failed.
    DnsFailure,
    /// The URL has no host component.
    MissingHost,
}

impl std::fmt::Display for HopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DenyListed(ip) => write!(f, "SSRF: IP {ip} is in a denied range"),
            Self::DnsFailure => write!(f, "SSRF: DNS resolution failed"),
            Self::MissingHost => write!(f, "SSRF: URL has no host"),
        }
    }
}

impl std::error::Error for HopError {}

// ── SSRF guard ─────────────────────────────────────────────────────────────

/// Returns `true` if `ip` falls in any range that must not be reachable from
/// the enrichment pipeline.
///
/// Denied IPv4 ranges:
/// * `127.0.0.0/8`    — loopback
/// * `10.0.0.0/8`     — RFC 1918
/// * `172.16.0.0/12`  — RFC 1918
/// * `192.168.0.0/16` — RFC 1918
/// * `169.254.0.0/16` — link-local / cloud metadata (169.254.169.254)
/// * `100.64.0.0/10`  — CGNAT
/// * `224.0.0.0/4`    — multicast
/// * `0.0.0.0/8`      — unspecified
///
/// Denied IPv6 ranges:
/// * `::1`         — loopback
/// * `fe80::/10`   — link-local
/// * `fc00::/7`    — unique local (fc00:: and fd00::)
/// * `ff00::/8`    — multicast
/// * `::`          — unspecified
///
/// IPv4-mapped IPv6 addresses (`::ffff:x.x.x.x`) are unwrapped to their inner
/// IPv4 address and re-checked against the IPv4 rules above.
pub fn ip_is_denied(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // 0.0.0.0/8
            if o[0] == 0 {
                return true;
            }
            // 10.0.0.0/8
            if o[0] == 10 {
                return true;
            }
            // 100.64.0.0/10  (100.64.0.0 – 100.127.255.255)
            if o[0] == 100 && (o[1] & 0xC0) == 64 {
                return true;
            }
            // 127.0.0.0/8
            if o[0] == 127 {
                return true;
            }
            // 169.254.0.0/16
            if o[0] == 169 && o[1] == 254 {
                return true;
            }
            // 172.16.0.0/12  (172.16.0.0 – 172.31.255.255)
            if o[0] == 172 && (o[1] & 0xF0) == 16 {
                return true;
            }
            // 192.168.0.0/16
            if o[0] == 192 && o[1] == 168 {
                return true;
            }
            // 224.0.0.0/4  (224.0.0.0 – 239.255.255.255)
            if o[0] & 0xF0 == 224 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped addresses and re-check as IPv4.
            if let Some(v4) = to_ipv4_mapped(v6) {
                return ip_is_denied(IpAddr::V4(v4));
            }
            let segs = v6.segments();
            // ::  (unspecified)
            if v6 == Ipv6Addr::UNSPECIFIED {
                return true;
            }
            // ::1 (loopback)
            if v6 == Ipv6Addr::LOCALHOST {
                return true;
            }
            // fe80::/10  (link-local: fe80:: – febf::)
            if segs[0] & 0xFFC0 == 0xFE80 {
                return true;
            }
            // fc00::/7  (unique local: fc00:: – fdff::)
            if segs[0] & 0xFE00 == 0xFC00 {
                return true;
            }
            // ff00::/8  (multicast)
            if segs[0] & 0xFF00 == 0xFF00 {
                return true;
            }
            false
        }
    }
}

/// Extract the inner IPv4 address from an IPv4-mapped IPv6 address
/// (`::ffff:x.x.x.x`), or `None` for any other IPv6 address.
fn to_ipv4_mapped(v6: Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    // IPv4-mapped: first 80 bits zero, next 16 bits all-ones (0xFFFF), then 32-bit IPv4.
    let segs = v6.segments();
    if segs[0] == 0
        && segs[1] == 0
        && segs[2] == 0
        && segs[3] == 0
        && segs[4] == 0
        && segs[5] == 0xFFFF
    {
        let bytes = v6.octets();
        Some(std::net::Ipv4Addr::new(
            bytes[12], bytes[13], bytes[14], bytes[15],
        ))
    } else {
        None
    }
}

/// Validate a single URL before following it as a redirect hop.
///
/// Resolves the URL's host via the OS DNS resolver (blocking call — see
/// module-level design note) and checks every resolved IP against the denied
/// ranges via [`ip_is_denied`].
///
/// Returns `Ok(())` only if at least one address resolved **and** none of them
/// are denied.
pub fn validate_hop(url: &reqwest::Url) -> Result<(), HopError> {
    let host = url.host_str().ok_or(HopError::MissingHost)?;

    // `to_socket_addrs` requires a port; use 0 as a placeholder.
    let addrs = (host, 0u16)
        .to_socket_addrs()
        .map_err(|_| HopError::DnsFailure)?;

    let mut found_any = false;
    for sock_addr in addrs {
        found_any = true;
        let ip = sock_addr.ip();
        if ip_is_denied(ip) {
            warn!(%ip, url = %url, "SSRF: redirect to denied IP blocked");
            return Err(HopError::DenyListed(ip));
        }
    }

    if !found_any {
        return Err(HopError::DnsFailure);
    }

    Ok(())
}

// ── Client constructors ────────────────────────────────────────────────────

/// Build a reqwest client suitable for metadata REST/GraphQL API calls.
///
/// * 10-second timeout.
/// * Maximum 5 redirect hops.
/// * Compiled with rustls TLS — no OpenSSL dependency.
pub fn api_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(redirect::Policy::limited(5))
        .build()
        .expect("failed to build api_client — TLS init should not fail in this environment")
}

/// Build a reqwest client suitable for fetching cover image URLs.
///
/// Identical to [`api_client`] but with a configurable redirect limit and
/// timeout, plus an SSRF guard on every redirect hop.
///
/// The redirect policy resolves each redirect target via the OS DNS resolver
/// (blocking) and rejects any hop whose IP falls in a denied range.  See the
/// module-level design note for the rationale behind using a blocking resolver.
///
/// # Panics
///
/// Panics if the underlying TLS stack cannot be initialised — this should
/// never happen in a normally configured environment.
pub fn cover_client(redirect_limit: usize, timeout_secs: u64) -> reqwest::Client {
    let policy = redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= redirect_limit {
            return attempt.error("too many redirects");
        }
        match validate_hop(attempt.url()) {
            Ok(()) => attempt.follow(),
            Err(e) => attempt.error(e),
        }
    });

    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(policy)
        .build()
        .expect("failed to build cover_client — TLS init should not fail in this environment")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    // ── IPv4 private / denied ─────────────────────────────────────────────

    #[test]
    fn loopback_v4_denied() {
        assert!(ip_is_denied(v4(127, 0, 0, 1)));
        assert!(ip_is_denied(v4(127, 255, 255, 255)));
    }

    #[test]
    fn rfc1918_10_denied() {
        assert!(ip_is_denied(v4(10, 0, 0, 1)));
        assert!(ip_is_denied(v4(10, 255, 255, 255)));
    }

    #[test]
    fn rfc1918_172_denied() {
        assert!(ip_is_denied(v4(172, 16, 0, 1)));
        assert!(ip_is_denied(v4(172, 31, 255, 255)));
    }

    #[test]
    fn rfc1918_172_edge_allowed() {
        // 172.15.x.x and 172.32.x.x are outside the /12 range.
        assert!(!ip_is_denied(v4(172, 15, 255, 255)));
        assert!(!ip_is_denied(v4(172, 32, 0, 0)));
    }

    #[test]
    fn rfc1918_192_168_denied() {
        assert!(ip_is_denied(v4(192, 168, 1, 1)));
    }

    #[test]
    fn link_local_denied() {
        assert!(ip_is_denied(v4(169, 254, 169, 254))); // cloud metadata
        assert!(ip_is_denied(v4(169, 254, 0, 1)));
    }

    #[test]
    fn cgnat_denied() {
        assert!(ip_is_denied(v4(100, 64, 0, 1)));
        assert!(ip_is_denied(v4(100, 127, 255, 255)));
    }

    #[test]
    fn multicast_denied() {
        assert!(ip_is_denied(v4(224, 0, 0, 1)));
        assert!(ip_is_denied(v4(239, 255, 255, 255)));
    }

    #[test]
    fn unspecified_v4_denied() {
        assert!(ip_is_denied(v4(0, 0, 0, 0)));
        assert!(ip_is_denied(v4(0, 255, 255, 255)));
    }

    // ── IPv4 public — should pass ─────────────────────────────────────────

    #[test]
    fn public_ip_allowed() {
        assert!(!ip_is_denied(v4(8, 8, 8, 8))); // Google DNS
        assert!(!ip_is_denied(v4(1, 1, 1, 1))); // Cloudflare
        assert!(!ip_is_denied(v4(93, 184, 216, 34))); // example.com
    }

    // ── IPv6 ──────────────────────────────────────────────────────────────

    #[test]
    fn loopback_v6_denied() {
        assert!(ip_is_denied(v6("::1")));
    }

    #[test]
    fn unspecified_v6_denied() {
        assert!(ip_is_denied(v6("::")));
    }

    #[test]
    fn link_local_v6_denied() {
        assert!(ip_is_denied(v6("fe80::1")));
        assert!(ip_is_denied(v6("febf::1")));
    }

    #[test]
    fn unique_local_v6_denied() {
        assert!(ip_is_denied(v6("fc00::1")));
        assert!(ip_is_denied(v6("fd00::1")));
    }

    #[test]
    fn multicast_v6_denied() {
        assert!(ip_is_denied(v6("ff02::1")));
    }

    #[test]
    fn public_v6_allowed() {
        assert!(!ip_is_denied(v6("2001:4860:4860::8888"))); // Google DNS IPv6
    }

    // ── IPv4-mapped IPv6 ──────────────────────────────────────────────────

    #[test]
    fn ipv4_mapped_private_denied() {
        // ::ffff:10.0.0.1
        assert!(ip_is_denied(v6("::ffff:10.0.0.1")));
        // ::ffff:192.168.1.1
        assert!(ip_is_denied(v6("::ffff:192.168.1.1")));
        // ::ffff:169.254.169.254 — cloud metadata via IPv4-mapped
        assert!(ip_is_denied(v6("::ffff:169.254.169.254")));
    }

    #[test]
    fn ipv4_mapped_public_allowed() {
        // ::ffff:8.8.8.8
        assert!(!ip_is_denied(v6("::ffff:8.8.8.8")));
    }
}
