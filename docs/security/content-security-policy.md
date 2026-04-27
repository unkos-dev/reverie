# Content Security Policy and security headers

Reverie ships strict security response headers by default (UNK-106). This
document is for operators: it explains what ships, why, and how to tune it.

## What ships by default

Every response carries four unconditional headers:

| Header                    | Value                                       | Purpose                                     |
| ------------------------- | ------------------------------------------- | ------------------------------------------- |
| `X-Content-Type-Options`  | `nosniff`                                   | Disables MIME sniffing                      |
| `Referrer-Policy`         | `no-referrer`                               | Omits `Referer` on outgoing navigations     |
| `Permissions-Policy`      | `camera=(), microphone=(), geolocation=(), payment=(), usb=(), midi=(), magnetometer=(), accelerometer=(), gyroscope=()` | Denies every high-risk browser capability |
| `X-Frame-Options`         | `DENY`                                      | Legacy clickjacking defence (CSP covers this too) |

A `Content-Security-Policy` differentiated by route class:

- **HTML responses** (`/` and SPA deep links): a hash-based CSP that permits
  one known inline FOUC script (via `'sha256-...'`). No `'unsafe-inline'` for
  scripts.
- **API responses** (`/api/*`, `/auth/*`, `/health/*`, `/opds/*`):
  `default-src 'none'; frame-ancestors 'none'; base-uri 'none'` — APIs never
  render, so everything is locked down.

## Opt-in: HSTS

`Strict-Transport-Security` is disabled by default because Reverie's container
speaks plain HTTP. Turn it on only when a TLS-terminating reverse proxy sits
in front of the backend:

```bash
REVERIE_BEHIND_HTTPS=true
```

This emits `max-age=31536000` (one year). To extend the lock-out to sibling
subdomains, also set:

```bash
REVERIE_HSTS_INCLUDE_SUBDOMAINS=true
```

> **Footgun.** `includeSubDomains` forces every sibling of Reverie's host
> onto HTTPS-only for the max-age window. If Reverie runs at
> `reverie.example.com` and you later set up `anything.example.com` without
> a certificate, browsers that cached Reverie's HSTS will refuse to reach
> it. Enable this flag **only on dedicated-apex deployments** where you
> control every subdomain.

To submit the host to the
[HSTS preload list](https://hstspreload.org/):

```bash
REVERIE_HSTS_PRELOAD=true
```

Preload requires `REVERIE_HSTS_INCLUDE_SUBDOMAINS=true`, which in turn
requires `REVERIE_BEHIND_HTTPS=true`. The backend refuses to start with
inconsistent combinations.

## Opt-in: CSP violation reporting

Point Reverie at a log sink and every CSP violation a browser encounters
produces a JSON report delivered via HTTP POST:

```bash
REVERIE_CSP_REPORT_ENDPOINT=https://log.example.com/csp
```

- Emitted as both `report-uri <url>` (legacy, Safari/Firefox older versions)
  and `Reporting-Endpoints: csp-endpoint="<url>"` + `report-to csp-endpoint`
  (modern Reporting API).
- Browsers POST to the endpoint with no authentication. Your sink must
  accept unauthenticated requests from browsers.
- Content types: browsers send `application/csp-report` (legacy) or
  `application/reports+json` (Reporting API). Accept both or accept any.
- Reverie does not host a reporting endpoint. Typical targets:
  [Sentry CSP endpoint](https://docs.sentry.io/product/security-policy-reporting/),
  Loki push API, a custom webhook handler, or a reverse proxy's log pipeline.

### Security of the reporting URL

The URL flows into a response header. Reverie rejects any URL containing
`"`, `;`, CR, or LF to prevent header-splitting injection. Must be a valid
absolute `http(s)://` URL.

## Dev mode vs production

| Surface           | Dev (Vite dev server)                              | Production (Docker container)                              |
| ----------------- | -------------------------------------------------- | ---------------------------------------------------------- |
| HTML CSP          | `'unsafe-inline' 'unsafe-eval'` + HMR WebSocket    | Strict hash-based, no `'unsafe-inline'`/`'unsafe-eval'`    |
| API CSP           | Vite proxies `/api`, `/auth`, `/opds` to the backend; backend's API CSP applies to those responses | `default-src 'none'; frame-ancestors 'none'; base-uri 'none'` |
| HSTS              | Off                                                | Off by default; on behind TLS with `REVERIE_BEHIND_HTTPS=true` |
| `font-src` policy | `'self'` (matches prod)                            | `'self'` (declared in `csp.rs::build_html_csp`)            |
| index.html source | Vite dev server, transformed with plugin markers   | Pre-built `dist/index.html` served by the backend          |

**Dev relaxations do not ship to prod.** `'unsafe-inline' 'unsafe-eval'` in
dev are declared in `frontend/vite.config.ts` `server.headers` and apply
only when running `npm run dev`.

### Fonts

Reverie self-hosts variable woff2 fonts at
`frontend/public/fonts/fontshare/files/`; the `font-src 'self'` directive
is sufficient for the default deployment. Operators who need fonts from
a CDN (e.g., Google Fonts, custom asset host) must edit
`backend/src/security/csp.rs::build_html_csp` to allowlist the required
origin(s) and rebuild. No runtime configuration knob exists for this —
the policy is intentionally code-declared so every deployment has an
identical, auditable font policy out of the box.

The canonical theme tree (`frontend/src/styles/themes/`,
`frontend/src/styles/fonts.css`) declares Author + Satoshi as variable
woff2 from Fontshare and JetBrains Mono Regular. Author and Satoshi
italics are pulled from Fontshare's per-font download endpoint (the
public weight CSS API does not expose italic variable axes). FFL
clause-02 acceptance is documented in
`frontend/public/fonts/fontshare/README.md`.

## Cookies

Reverie sets two cookies on authenticated browsers:

| Name            | HttpOnly | Max-Age     | Path | SameSite | Purpose                                    | Lifecycle                                          |
| --------------- | -------- | ----------- | ---- | -------- | ------------------------------------------ | -------------------------------------------------- |
| `id`            | **Yes**  | Session     | `/`  | `Lax`    | tower-sessions session cookie (auth state) | Cleared on logout; short-lived                     |
| `reverie_theme` | **No**   | 365 days    | `/`  | `Lax`    | Dark/Light/System preference for FOUC      | Survives logout by design (device state, not PII)  |

`reverie_theme` is intentionally not `HttpOnly` because JavaScript must
read it synchronously before React hydrates to avoid a theme flicker. It
carries no PII — only the literal string `system`, `light`, or `dark`.
See `docs/design/visual-identity.md` § Theme Cookie Lifecycle for the
full rationale and the contrast rule: any future *session-state* cookie
MUST be `HttpOnly` and MUST be cleared on logout; `reverie_theme` is the
explicit counterexample.

`reverie_theme` is always emitted with `Secure`. Reverie's threat model
is "multi-user exposed instance," and a publicly-reachable HTTP-only
deployment in 2026 is a misconfiguration we don't bend the design to
support. Localhost dev still works because Chrome (≥v89) and Firefox
treat `http://localhost` as a secure context and accept Secure cookies
on it. An operator running Reverie behind a public DNS name on plain
HTTP will see the browser silently reject the cookie — the documented
signal to put the deployment behind TLS, whether terminated at a proxy
or directly.

The session cookie (`id`) does not set `Secure` today; that's tracked as
a follow-up to apply the same treatment.

## `style-src 'unsafe-inline'` — why it's still there

The HTML CSP allows inline styles:

```
style-src 'self' 'unsafe-inline'
```

This is a pragmatic concession for:

- **Tailwind CSS JIT** — generates `style=""` on some utilities.
- **Radix UI portals** — positions popovers/dialogs with runtime inline
  styles.

CSS injection impact is far narrower than script injection: a CSS injection
can restyle the page and exfiltrate attribute values, but cannot run
JavaScript. If a future migration off Tailwind/Radix eliminates runtime
inline styles, this concession can be dropped by editing
`backend/src/security/csp.rs::build_html_csp`.

## Testing your deployment

### Automated

```bash
curl -sI https://reverie.example.com/ | grep -iE '^(content-security-policy|strict-transport-security|x-content-type-options|referrer-policy|permissions-policy|x-frame-options):'
```

All six should be present on every HTML response. The API response loses
HSTS if the request was served before TLS termination (unchanged behaviour).

### Third-party auditors

- [securityheaders.com](https://securityheaders.com) — expect **A+**.
- [Mozilla Observatory](https://observatory.mozilla.org) — expect **A+**.

If either returns less than A, check:

1. `REVERIE_BEHIND_HTTPS=true` is set (HSTS is required for A+).
2. Your reverse proxy is not stripping or overriding headers.
3. TLS certificate is valid (Observatory penalises weak ciphers separately).

### Manual browser verification

1. Open the application.
2. Open DevTools → Console.
3. Navigate between routes.
4. Watch for `Refused to execute inline script because it violates the
   following Content Security Policy directive` — if you see one, a
   legitimate inline script landed in a PR without a hash. File an issue.

## Further reading

- [MDN: Content-Security-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Security-Policy)
- [W3C CSP Level 3](https://www.w3.org/TR/CSP3/)
- [MDN: Reporting-Endpoints](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Reporting-Endpoints)
