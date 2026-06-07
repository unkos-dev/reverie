// Declarative loopback defaults used when REVERIE_DEV_HOSTS is unset.
//
// Vite's host-validation middleware (`isHostAllowedInternal` in
// `vite/dist/node/chunks/node.js`) short-circuits *before* iterating the
// allowlist for any IPv4 / IPv6 literal Host header and for any hostname
// equal to `localhost` or ending in `.localhost`. As a result these three
// entries are functionally no-ops at runtime — Vite would accept loopback
// requests even with an empty allowlist. They remain here as a declared
// scope (and the IPv6 literal is in Vite's bracket-stripped form, so it
// would match if Vite's hardcoded ipv6 short-circuit is ever removed).
// The DNS-rebind guard only filters *non-loopback hostnames* the developer
// explicitly adds via REVERIE_DEV_HOSTS.

/**
 * Loopback hostnames that Vite's dev server accepts as the dev-server `Host`
 * header when the developer has not supplied `REVERIE_DEV_HOSTS`. Declared
 * here for documentation; Vite short-circuits any loopback Host header before
 * iterating the allowlist, so removing entries from this array does not
 * tighten the runtime check.
 *
 * Cross-reference: `adr/2026-05-22-frontend-docstring-tooling.md` § Carve-outs
 * for why this file is in the Tier 2 lint scope.
 */
export const DEFAULT_LOOPBACK_HOSTS: readonly string[] = ["localhost", "127.0.0.1", "[::1]"];

// Characters permitted in a single REVERIE_DEV_HOSTS entry: hostname labels
// (letters, digits, `.`, `-`), the IPv6 bracket literal (`[`, `]`, `:`), and
// an optional `:port`. Whitespace, `;`, `/`, and any scheme are excluded so a
// value cannot break out of the `wss://<host>` CSP token it is interpolated
// into (see the THREAT note on parseAllowedHosts).
const HOST_TOKEN = /^[A-Za-z0-9.\-:[\]]+$/;

/**
 * Parse `REVERIE_DEV_HOSTS` into the array Vite's dev server uses as its
 * `server.allowedHosts` value. Whitespace-trimmed, comma-separated entries
 * with empty entries dropped. An undefined, empty, or all-whitespace value
 * resolves to a defensive copy of [[DEFAULT_LOOPBACK_HOSTS]].
 *
 * THREAT: the dev server's DNS-rebinding guard refuses any non-loopback
 * `Host` header not present in the allowlist. A misparse here (an entry
 * silently dropped, or the whole string read as one entry) would either
 * harden the guard beyond reach (developer cannot connect from the
 * workspace hostname) or weaken it (an entry like `foo, evil.example.com`
 * accidentally accepted as a single combined entry). Behaviour: strict
 * replacement — caller-supplied entries fully replace the loopback
 * defaults, not merge. `REVERIE_DEV_HOSTS=workspace.example.com` removes
 * `localhost` from the allowlist; rely on Vite's hardcoded loopback
 * short-circuit (`DEFAULT_LOOPBACK_HOSTS` documentation above) for
 * concurrent localhost access.
 *
 * THREAT: this output also seeds the dev CSP `connect-src` (see
 * `vite-plugins/dev-csp.ts`), where each non-loopback entry is interpolated
 * into a `wss://<host>` source token. An entry carrying whitespace or a `;`
 * would split or terminate the directive — injecting an attacker-shaped CSP
 * directive into the dev response header. Each entry is therefore validated
 * against a host-token character set (hostname / IPv4 / bracketed-IPv6 /
 * optional `:port` only) and a non-conforming value throws at config load rather
 * than reaching either consumer. Cross-reference: `adr/2026-05-08-tiered-comment-policy.md` § Tier 2.
 *
 * Cross-references:
 *   - `adr/2026-05-08-tiered-comment-policy.md` § Tier 2.
 *   - `frontend/CLAUDE.md` § Dev environment variables (operator contract).
 *
 * @param envValue - Raw `REVERIE_DEV_HOSTS` value or undefined.
 * @returns Mutable array safe to hand to Vite's config (never aliased onto
 *   the readonly default).
 */
export function parseAllowedHosts(envValue: string | undefined): string[] {
  if (envValue === undefined) return [...DEFAULT_LOOPBACK_HOSTS];
  const entries = envValue
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (entries.length === 0) return [...DEFAULT_LOOPBACK_HOSTS];
  for (const entry of entries) {
    if (!HOST_TOKEN.test(entry)) {
      throw new Error(
        `REVERIE_DEV_HOSTS entries must be bare hostnames (optionally with :port), ` +
          `got ${JSON.stringify(entry)}. Remove any scheme, path, or whitespace.`,
      );
    }
  }
  return entries;
}
