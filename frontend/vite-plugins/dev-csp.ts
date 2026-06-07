// Dev-only Content-Security-Policy builder. The production CSP is a strict,
// hash-based policy served by the backend (see backend/src/security/csp.rs);
// this file never feeds it. The dev policy is intentionally relaxed with
// 'unsafe-inline' / 'unsafe-eval' so Vite HMR, esbuild error overlays, and
// Tailwind JIT work, and these relaxations do not ship to prod.
//
// `static.cloudflareinsights.com` (script-src) and `cloudflareinsights.com`
// (connect-src) accommodate the Cloudflare Web Analytics RUM beacon that the
// edge auto-injects into HTML when a dev hostname is proxied through a CF zone
// with RUM enabled. Per-hostname Exclude rules are a Cloudflare Pro feature,
// so the cheapest fix is allowlisting the beacon origins here. The allowlist
// is dev-only and does not appear in the production CSP. See
// docs/deployment/cloudflare-edge-dev-hostnames.md.
//
// The `connect-src` HMR websocket origins are derived from the same
// `REVERIE_DEV_HOSTS` input the dev server's allowedHosts guard consumes
// (vite-plugins/allowed-hosts.ts), so any non-loopback dev host already in
// the server allowlist automatically gets its HMR origin permitted in CSP —
// no separate env var, no hardcoded hostname.
//
// THREAT: each non-loopback host is interpolated into a `wss://<host>` token
// inside the CSP header value. A host carrying whitespace or `;` would break
// out of that token and inject an attacker-shaped directive into the dev
// response header. The host entries are validated upstream in
// `parseAllowedHosts` (which rejects any non-host-token character before this
// module sees them), so `buildDevCsp` assumes well-formed bare hostnames.
// Dev-only — the production CSP (backend/src/security/csp.rs) is unaffected.
// Cross-reference: adr/2026-05-08-tiered-comment-policy.md § Tier 2.

import { DEFAULT_LOOPBACK_HOSTS, parseAllowedHosts } from "./allowed-hosts";

// Loopback HMR websocket origins, kept verbatim regardless of
// REVERIE_DEV_HOSTS so a browser at localhost:5173 always has an allowed HMR
// socket. These mirror the two literals the dev CSP has always carried; they
// are not derived from the allowlist so the unset case stays byte-identical
// to the historical policy (the IPv6 `[::1]` default never appeared here and
// is deliberately not emitted). Vite serves HMR on the loopback the browser
// actually dialed, so localhost + 127.0.0.1 cover direct dev.
const LOOPBACK_WS_ORIGINS: readonly string[] = ["ws://localhost:5173", "ws://127.0.0.1:5173"];

/**
 * True for hostnames Vite treats as loopback, so they are not turned into a
 * derived `wss://` remote origin. Matches the declarative loopback defaults
 * plus the `*.localhost` suffix Vite short-circuits. Of these, only
 * `localhost` and `127.0.0.1` appear in [[LOOPBACK_WS_ORIGINS]]; `[::1]` and
 * `*.localhost` hosts are intentionally absent from the websocket allowlist
 * (see the [[LOOPBACK_WS_ORIGINS]] comment) — they are filtered here so they
 * are never emitted as a remote `wss://` origin either.
 */
function isLoopbackHost(host: string): boolean {
  return DEFAULT_LOOPBACK_HOSTS.includes(host) || host.endsWith(".localhost");
}

/**
 * Build the dev-only Content-Security-Policy header value.
 *
 * The `connect-src` directive permits the loopback HMR sockets unconditionally
 * and, for every non-loopback host in `devHosts`, a `wss://<host>` origin. The
 * authority is emitted bare (no port), which by CSP matches only the `wss`
 * scheme default — port 443. This is correct for the supported topology (a TLS
 * edge terminating on 443, so the browser dials `wss://<host>/`); an edge on a
 * non-default TLS port would need an explicit port and is out of scope. When
 * `devHosts` is unset/empty the result reproduces the historical loopback-only
 * policy exactly.
 *
 * @param devHosts - Raw `REVERIE_DEV_HOSTS` value (comma-separated) or
 *   undefined. Parsed via [[parseAllowedHosts]] — the same input the dev
 *   server's allowedHosts guard consumes.
 * @returns The `Content-Security-Policy` header value for the dev server.
 */
export function buildDevCsp(devHosts: string | undefined): string {
  const remoteWsOrigins = parseAllowedHosts(devHosts)
    .filter((host) => !isLoopbackHost(host))
    .map((host) => `wss://${host}`);

  return [
    "default-src 'self'",
    "script-src 'self' 'unsafe-inline' 'unsafe-eval' https://static.cloudflareinsights.com",
    "style-src 'self' 'unsafe-inline'",
    [
      "connect-src 'self'",
      ...LOOPBACK_WS_ORIGINS,
      ...remoteWsOrigins,
      "https://cloudflareinsights.com",
    ].join(" "),
    "img-src 'self' data:",
    "font-src 'self'",
  ].join("; ");
}
