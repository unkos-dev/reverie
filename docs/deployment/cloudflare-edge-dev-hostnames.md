# Cloudflare edge — dev/staging hostnames

Operators who front Reverie dev or staging environments with Cloudflare
(via a Tunnel, an Origin, or a Worker) get one auto-injected script they
likely do not want on a dev surface: the Cloudflare Web Analytics RUM
beacon.

This page explains what shows up, why the dev CSP allowlists it, and
when to revisit.

## What gets injected

Cloudflare auto-injects a script tag into HTML responses when:

- the zone has Web Analytics enabled with **Automatic Setup**, **and**
- the hostname is proxied through Cloudflare

The tag looks like:

```html
<script
  defer
  src="https://static.cloudflareinsights.com/beacon.min.js/v[...]"
  data-cf-beacon='{"token":"[...]"}'
></script>
```

The beacon script loads from `static.cloudflareinsights.com` and POSTs
telemetry back to `cloudflareinsights.com`.

## Why the dev CSP allowlists the beacon origins

Reverie's dev `Content-Security-Policy` (see
[`frontend/vite.config.ts`](../../frontend/vite.config.ts)) is intentionally
relaxed for tooling — `'unsafe-inline'`/`'unsafe-eval'` for HMR overlays
and Tailwind JIT — and it explicitly allowlists the beacon origins so the
auto-injected script does not trip a CSP violation on every page load:

```text
script-src  'self' 'unsafe-inline' 'unsafe-eval' https://static.cloudflareinsights.com
connect-src 'self' ws://localhost:5173 ws://127.0.0.1:5173 https://cloudflareinsights.com
```

The cleaner alternative — a per-hostname **Exclude** rule under
**Web Analytics → Manage site → Advanced options** — requires a
Cloudflare **Pro** subscription. On Free, the only granular control is
the zone-wide RUM Enable/Disable toggle, which is too coarse for a zone
that hosts more than the dev surface.

## When to revisit

| Trigger                                             | Action                                                                                    |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Upgrade to Cloudflare Pro on the zone               | Switch to a per-hostname Exclude rule and remove the dev-CSP allowlist (cleaner posture). |
| Zone no longer hosts non-dev services that need RUM | Disable RUM zone-wide; drop the dev-CSP allowlist.                                        |
| Beacon origin or path changes upstream              | Update the dev CSP allowlist. (Cloudflare has changed beacon hostnames historically.)     |

## Production CSP

The production CSP (served by the Reverie backend — see
[`backend/src/security/csp.rs`](../../backend/src/security/csp.rs)) is
hash-based and strict and does **not** allowlist
`static.cloudflareinsights.com`. RUM on the production hostname is a
separate decision against that file, independent of the dev CSP.

If you want RUM on production, scope that decision deliberately — do not
copy the dev allowlist across.

## Related

- [Reverse proxy topology](./reverse-proxy.md)
- [Content Security Policy and security headers](../security/content-security-policy.md)
- Cloudflare docs:
  [Web Analytics rules](https://developers.cloudflare.com/web-analytics/configuration-options/rules/)
  (Pro+ feature)
