---
severity: medium
surfaces: [security, server-operator]
adopted: 2026-06-27
adopted-because: PR #511 review (CodeRabbit); trusted_client_ip_header is honoured by name with no trusted-proxy peer binding
lift-when-class: feature-flag
lift-when: forwarded-IP trust is bound to an allow-listed reverse-proxy peer CIDR (honour the header only when the TCP peer is in-CIDR), with a test covering an off-CIDR spoof
---

# Forwarded client-IP header is trusted by name, not by proxy identity

`auth::rate_limit::client_ip` reads `trusted_client_ip_header` (when configured)
to derive the per-source key for the login and recovery limiters. It trusts the
header on presence alone, without checking that the request actually arrived
through the proxy that is supposed to set it.

When the backend is reachable directly (not only through the trusted edge), any
caller can send that header with a rotating value and pick its own limiter key,
evading the per-IP login and recovery limits. The per-account throttle is the
IP-independent backstop and still bounds per-email brute force, so this is
defense-in-depth rather than an unbounded hole, and it is accepted until the
trust is bound to proxy identity.

Lift by honouring the forwarded header only when the TCP peer is in an
operator-configured reverse-proxy CIDR allow-list (falling back to the socket
peer otherwise), with a test that a spoofed header from an off-CIDR peer is
ignored. Tracked under the rate-limit coverage audit.
