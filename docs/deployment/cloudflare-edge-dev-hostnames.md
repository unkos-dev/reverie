# Cloudflare edge — dev/staging hostnames

Operators who front Reverie dev or staging environments with Cloudflare
(via a Tunnel, an Origin, or a Worker) get one auto-injected script they
likely do not want on a dev surface: the Cloudflare Web Analytics RUM
beacon.

This page covers why it shows up and how to exclude dev hostnames from it.

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

## Why this matters on dev hostnames

Reverie's dev `Content-Security-Policy` (see
[`frontend/vite.config.ts`](../../frontend/vite.config.ts)) is intentionally
relaxed for tooling — `'unsafe-inline'`/`'unsafe-eval'` for HMR overlays
and Tailwind JIT — but it does **not** allowlist
`static.cloudflareinsights.com`. The browser blocks the injected script
and logs a CSP violation on every page load:

```text
Refused to load the script
'https://static.cloudflareinsights.com/beacon.min.js/v[...]'
because it violates the following Content Security Policy directive:
"script-src 'self' 'unsafe-inline' 'unsafe-eval'".
```

Cosmetic — the page still loads — but the violation noise hides real CSP
issues during dev.

## Recommended fix: exclude dev hostnames from Web Analytics

Dev surfaces have no consumers worth measuring, so the cleanest fix is to
stop Cloudflare from injecting the beacon for those hostnames rather than
extend the dev CSP to allowlist a tracker.

### Dashboard steps

1. Cloudflare dashboard → **Analytics & Logs** → **Web Analytics**
2. Find the site for your zone → **Manage site**
3. **Advanced options** → **Add rule**
4. Action: **Exclude**
5. Hostname: the dev hostname (e.g. `dev.example.com`,
   `hmr.example.com`)
6. Path: leave default (matches all paths) unless you only want to
   exclude a sub-route
7. **Update**

The exclusion takes effect on the next request. Cloudflare stops
injecting the beacon tag for matching traffic.

### Why exclude rather than extend CSP

| Option                                                      | Cost                                                                                                                                                         |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Extend dev CSP to allowlist `static.cloudflareinsights.com` | Reverie's dev config has to know about a Cloudflare zone feature. Adds a third-party tracker to the dev allowlist. Generalises poorly as dev hostnames grow. |
| Cloudflare rule to exclude the hostname                     | Single dashboard click. CF zone config stays at the CF zone. Generalises by adding more excluded hostnames.                                                  |

The trade-off flips if you intentionally use Web Analytics across many
dev hostnames. At ~3+ hostnames the per-hostname exclude becomes toil
and extending the CSP wins.

## Production CSP

The production CSP (served by the Reverie backend — see
[`backend/src/security/csp.rs`](../../backend/src/security/csp.rs)) is
hash-based and strict. If you want RUM on the production hostname, that
is a separate, scoped decision that does **not** ride on the dev CSP.
See
[Content Security Policy and security headers](../security/content-security-policy.md).

## Related

- [Reverse proxy topology](./reverse-proxy.md)
- [Content Security Policy and security headers](../security/content-security-policy.md)
- Cloudflare docs:
  [Web Analytics rules](https://developers.cloudflare.com/web-analytics/configuration-options/rules/)
