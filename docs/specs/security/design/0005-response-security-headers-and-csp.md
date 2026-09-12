---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0005"
title: "Response security headers and CSP"
satisfies:
  - "REV-REQ-0011"
  - "REV-REQ-0012"
  - "REV-REQ-0013"
governed-by:
  - "REV-ADR-0003"
---

# Response security headers and CSP

This Design covers the response headers every request to the Reverie backend passes through: the four uniform security
headers, the two-class Content Security Policy that separates HTML and asset responses from API responses, the build
step that produces the HTML policy's script hashes, the startup check that refuses to serve a build whose hashes are
missing or broken, and the composite fallback that decides, for a request no route matches, whether the answer is a
machine-readable "not found" or the single-page application.

## Purpose and boundaries

This subject owns the uniform security-headers middleware on every response the composite router produces; the two
`Content-Security-Policy` builders and the two per-router layers that attach them; the composite fallback and its
dispatch by route class, including the reserved prefixes that keep `/api`, `/auth`, `/health` and `/opds` off the
single-page application; the Vite plugin that hashes the frontend's one inline script and writes the hash sidecar into
the build; and the startup step that reads the sidecar, validates it and builds the HTML policy from it before the
server accepts a request. It also owns the two headers computed from operator configuration, `Strict-Transport-Security`
and `Reporting-Endpoints`.

It does not own CSRF enforcement: `backend/src/security/csrf.rs` implements the synchronizer-token check as a
neighbouring layer, and this Design depends only on its position in the layer order. It does not own session cookie
attributes: the `SessionManagerLayer` built in `backend/src/lib.rs` sets `Secure`, `SameSite` and expiry independently.
It does not own the Problem Details body of the `404` and `405` responses the fallback returns; `AppError::NotFound` and
`AppError::MethodNotAllowed` (`backend/src/error/mod.rs`) and `problem_instance_layer` (`backend/src/error/instance.rs`)
belong to the API error contract. It does not own the dev-only Vite plugins that relax the policy, allow extra dev
hostnames or move the HMR port; they run only under `vp dev`.

Depends on: the frontend build's output directory and its hash sidecar, produced by `vite build`; the routed handlers
and the OPDS router mounted under `/api`, `/auth` and `/opds`; `crate::state::AppState` for the `SecurityConfig` every
layer here reads.

Depended on by: every response the server sends, including every handler under the reserved prefixes and every file in
the built frontend; and the CSRF layer, whose rejection responses carry no policy because of where it sits in the layer
order.

## Structure

### Component relationships

- `backend/src/security/mod.rs` is the module root. It declares `csp`, `csrf`, `dist_validation` and `headers`, and its
  module docs mark everything beneath it as security-critical.
- `backend/src/security/csp.rs` holds two pure builders. `build_html_csp` produces eleven directives:
  `default-src 'self'`, a hash-allowlisted `script-src 'self'`, `style-src 'self' 'unsafe-inline'`,
  `img-src 'self' data:`, `font-src 'self'`, `connect-src 'self'`, `frame-ancestors 'none'`, `base-uri 'self'`,
  `form-action 'self'`, `object-src 'none'` and `upgrade-insecure-requests`. `'unsafe-inline'` appears on `style-src`
  alone and `data:` on `img-src` alone; `frame-ancestors` and `object-src` are outright denials. The Requirements this
  Design satisfies bind the policy's presence and the hash set behind `script-src`; the remaining directives are
  recorded here and bound by none of them. `build_api_csp` produces
  `default-src 'none'; frame-ancestors 'none'; base-uri 'none'`. Both add `report-to` and `report-uri` directives when a
  reporting endpoint is configured. `build_api_csp` runs once at startup; `build_html_csp` runs at most once, and only
  when a frontend build directory is configured.
- `backend/src/security/dist_validation.rs` (`validate_frontend_dist`) checks a configured build directory in a fixed
  order: the path exists, is a directory, contains `index.html`, and carries a `csp-hashes.json` whose
  `script-src-hashes` array is present, non-empty and holds only strings matching the anchored pattern
  `^sha(256|384|512)-[A-Za-z0-9+/]+={0,2}$`. It returns a `ValidatedFrontendDist` only when every check passes.
- `backend/src/security/headers.rs` holds the request-time pieces: `security_headers` (the uniform-headers middleware),
  `api_csp_layer` and `html_csp_layer` (the two per-router policy layers), and `composite_fallback` with its helpers
  `api_404_with_csp` and `spa_fallback_response`. The `RESERVED_PREFIXES` constant (`/api`, `/auth`, `/health`, `/opds`)
  and the `is_reserved_prefix` predicate live here too.
- `backend/src/config/security.rs` defines `SecurityConfig`: the operator settings (`behind_https`,
  `hsts_include_subdomains`, `hsts_preload`, `csp_report_endpoint`, `frontend_dist_path`) and the two policy strings
  (`csp_html_header`, `csp_api_header`) that stay `None` until startup fills them. Its `hsts_header_value` and
  `reporting_endpoints_header_value` methods compute the two conditional headers from those settings.
- `backend/src/routes/spa.rs` supplies the file-serving pieces the fallback calls but this subject does not own:
  `router_enabled` builds the `/assets/*` `ServeDir` mount (`None` when no build directory is configured), and
  `try_dist_file` serves any other file under the build root, treating only a `404` from `ServeDir` as "no such file"
  and passing every other status (`304`, `412`, `416`, `405`) through.
- `frontend/vite-plugins/csp-hash.ts` (`cspHashPlugin`) runs in `transformIndexHtml` on every dev and build run. It
  reads `frontend/src/fouc/fouc.js`, rejects it if the source contains a closing-script-tag literal, injects it into
  `index.html` at the `<!-- reverie:fouc-hash -->` marker (failing if the marker is missing or appears twice), hashes
  the script body with SHA-256 and rejects a digest that is not standard, non-URL-safe base64. Only on `vite build` does
  it write `{"script-src-hashes": ["sha256-<digest>"]}` to `<outDir>/csp-hashes.json`. The plugin's dev-server behaviour
  is outside this subject.

### Writer census: who sets Content-Security-Policy

One function writes the header on any given response, and the response's class decides which:

| Response class | Policy | Writer |
| -------------- | ------ | ------ |
| A matched route under `/api`, `/auth`, `/health` or `/opds`, whatever status the handler returns | API | `api_csp_layer` |
| A matched `/assets/*` request: a served file, or `ServeDir`'s own `404`, `304` or `405` | HTML | `html_csp_layer` |
| An unmatched request under a reserved prefix | API | `api_404_with_csp`, called from `composite_fallback` |
| An unmatched request naming a real file elsewhere in the build (fonts, brand assets, favicons Vite copies from `public/`) | HTML | `composite_fallback`'s file arm (`attach_html_csp`) |
| An unmatched request answered with the application's `index.html` | HTML | `spa_fallback_response` (`attach_html_csp`) |
| A session-authenticated mutation the CSRF layer rejects (`428`, `403` or `500`) | None | Nothing reaches this response |
| The plain `404` from the fallback when no build is configured or `index.html` cannot be read | None | Nothing reaches this response |
| An empty `500` the session layer substitutes when saving the session fails | None | Nothing reaches this response, including `security_headers` |

No response class has two writers. A second one can appear only outside the application: a reverse proxy that adds its
own `Content-Security-Policy` does not replace Reverie's, the browser enforces both policies at once, and the effective
policy stops being the one this table assigns. `docs/deployment/reverse-proxy.md` tells operators not to add these
headers at the proxy. `security_headers` is the single writer of the four uniform headers (`X-Content-Type-Options`,
`Referrer-Policy`, `Permissions-Policy`, `X-Frame-Options`) and of the two conditional ones
(`Strict-Transport-Security`, `Reporting-Endpoints`). It wraps the composite router, so it reaches every row above
except the last: the session layer sits outside it, and the response it substitutes on a failed save carries none of
these headers.

## Interfaces and dependencies

- The build directory and its `csp-hashes.json` sidecar are the interface between `frontend/vite-plugins/csp-hash.ts`
  and `backend/src/security/dist_validation.rs`: a JSON object with one key, `script-src-hashes`, holding an array of
  strings. Neither side reads a schema from the other; the anchored pattern on the reading side is the contract check.
- `REVERIE_FRONTEND_DIST_PATH`, `REVERIE_BEHIND_HTTPS`, `REVERIE_HSTS_INCLUDE_SUBDOMAINS`, `REVERIE_HSTS_PRELOAD` and
  `REVERIE_CSP_REPORT_ENDPOINT` are the operator-facing settings, loaded into `SecurityConfig` by the configuration code
  in `backend/src/config/`. `REVERIE_CSP_REPORT_ENDPOINT` passes through `de_csp_endpoint`, which rejects `"`, `;`,
  carriage return and line feed in the raw string before `url::Url::parse` runs, because the parser would otherwise
  percent-encode a quote instead of rejecting it, and which accepts only `http` and `https`.
- Every response the composite router produces carries the four uniform headers, the two conditional headers when
  configured, and at most one `Content-Security-Policy` value, as the writer census assigns. The session layer's empty
  `500` on a failed save is produced outside the composite router and carries none of them.
- `crate::routes::spa::router_enabled` and `crate::routes::spa::try_dist_file` are used, not owned, by the fallback;
  path-traversal protection for both is `ServeDir`'s.
- `crate::error::AppError::NotFound` and `crate::error::AppError::MethodNotAllowed` supply the bodies of the fallback
  `404` and the API router's `405`; their Problem Details shape belongs to the API error contract.

## Data and state

- **`SecurityConfig`** is loaded once from the environment at startup and never reloaded; unlike the operator settings
  in `backend/src/services/settings.rs`, which reload live, nothing here re-reads configuration while the process runs.
  `behind_https`, `hsts_include_subdomains` and `hsts_preload` form a validated ladder (`validate_security_config`, part
  of configuration validation): subdomains need HTTPS, and preload needs subdomains. An inconsistent combination stops
  startup instead of being ignored.
- **`csp_html_header` and `csp_api_header`** start as `None` and are filled once, in `reverie_api::run`, before the
  router is built or the listener bound. `csp_api_header` depends only on the optional reporting endpoint.
  `csp_html_header` also depends on `validate_frontend_dist`, so it is set only when a build directory is configured and
  validates.
- **The hash set** is computed at `vite build` time from the bytes of `frontend/src/fouc/fouc.js` and read once at
  backend startup from `csp-hashes.json`. The running server's `csp_html_header` reflects the set that was valid when
  `run` built it, even if the `index.html` that `spa_fallback_response` re-reads on each request later changes on disk
  because a build was replaced without a restart. The hash set's validity and its match with the bytes being served are
  separate properties; this subject enforces the first.
- **`RESERVED_PREFIXES`** and **`PERMISSIONS_POLICY_VALUE`** are compile-time constants in `headers.rs`; changing either
  needs a code change.

## Runtime behaviour

**From a build to an enforced policy**, from `vite build` to the first request the server accepts:

1. `cspHashPlugin`'s `transformIndexHtml` hook reads `frontend/src/fouc/fouc.js`, rejects a closing-script-tag literal,
   and confirms the template has exactly one `<!-- reverie:fouc-hash -->` marker before injecting the script there.
2. It hashes the script's bytes with SHA-256, rejects a digest that fails the standard-base64 check, and, because the
   command is `build`, writes `{"script-src-hashes": ["sha256-<digest>"]}` to `csp-hashes.json` in the output directory.
3. At backend startup, `reverie_api::run` calls `build_api_csp` and stores the result in
   `config.security.csp_api_header`.
4. If `config.security.frontend_dist_path` is set, `run` calls `validate_frontend_dist` on it. Any failure (a missing
   path or one that is not a directory, no `index.html`, a sidecar that cannot be read or is not valid JSON, a
   `script-src-hashes` value that is missing, not an array or empty, or an entry failing the pattern) returns an error
   that `run` propagates, and the process exits non-zero before `build_router` runs and before the listener binds.
5. On success, `run` calls `build_html_csp` with the validated hashes, stores the result in
   `config.security.csp_html_header`, builds the router and binds the listener. No request is handled between an
   operator's mistake and the process exiting.

**A request reaching the composite router** once the server is serving:

- A request with a safe method matching a route under `/api`, `/auth`, `/health` or `/opds` passes through
  `csrf_required` unchanged, reaches its handler, and `api_csp_layer` attaches the API policy to whatever status the
  handler returns.
- A session-authenticated mutation (`POST`, `PUT`, `PATCH` or `DELETE` on a session carrying a user id) with a missing
  or mismatched `X-CSRF-Token` is rejected by `csrf_required` before `next.run` is called.
  `build_router_with_session_store` adds that layer after `api_csp_layer`, so it wraps outside it: the rejection (`428`
  missing, `403` mismatch, or `500` if the session store read fails) never reaches `api_csp_layer` and carries no
  `Content-Security-Policy`. Only the uniform headers from `security_headers`, further out, apply.
- A request matching no route falls to `composite_fallback`, which first checks `is_reserved_prefix` against the raw
  path (`req.uri().path()`, not a decoded form). A path equal to a reserved prefix, or starting with one followed by
  `/`, goes to `api_404_with_csp`, which returns an `AppError::NotFound` Problem Details body
  (`application/problem+json`) with the API policy. A path that shares only a leading string with a prefix, such as
  `/apiology` or `/authed`, is not reserved and moves on.
- Otherwise `composite_fallback` calls `try_dist_file` against the whole build directory. A path naming a real file
  there, such as the fonts, brand assets and favicons Vite copies from `public/` to the build root outside `/assets`, is
  served with its own bytes (including `304` for a matching `If-None-Match`, or `405` with `Allow` for a non-GET), with
  the HTML policy. Anything else gets `index.html` with `200` and the HTML policy, whatever the `Accept` header says:
  RFC 9110 §12.5.1 allows this when no acceptable representation exists, and every client-side route is such a case.
- A missing file under `/assets/*` and a missing file elsewhere in the build resolve differently. The first matches the
  `nest_service("/assets", …)` route, so `ServeDir`'s own `404` is the response and `html_csp_layer` attaches the HTML
  policy. The second matches no route, so `try_dist_file` treats `ServeDir`'s `404` as "no such file", the fallback
  moves to its index arm, and the response is `index.html` with `200`. Both carry the HTML policy, so a path that moves
  between the two cases as files come and go keeps the same policy.

## Failure and recovery

- **Startup validation failure.** Every error `validate_frontend_dist` can return stops the server before it starts, as
  the steps above show. There is no degraded mode: the server either starts with a valid hash set behind its HTML
  policy, or does not run.
- **An HSTS combination that would lock out a deployment.** `hsts_include_subdomains` without `behind_https`, or
  `hsts_preload` without `hsts_include_subdomains`, is rejected by `validate_security_config` when configuration loads,
  before `SecurityConfig` reaches the request path. `Strict-Transport-Security` is never sent while `behind_https` is
  `false`, because `hsts_header_value` returns `None` in that case whatever the other two flags say; the middleware
  relies on that instead of checking again.
- **The plain `404` from the fallback.** Two conditions reach it: no `frontend_dist_path` configured (API-only
  development, where Vite serves the frontend and this path never activates), and `index.html` failing to read after
  startup (removed, or its permissions changed, while the process runs; logged at `warn` with the path and the I/O
  error). Both return a bare "not found" body with no `Content-Security-Policy`; the uniform headers from
  `security_headers` still apply.
- **A CSRF rejection carries no policy**, for the layering reason in Runtime behaviour. This applies to every
  session-authenticated mutation the CSRF layer refuses.
- **A failed session save.** `build_router_with_session_store` adds the `SessionManagerLayer` outside
  `security_headers`. Sessions are saved on every request (`with_always_save(true)`), so when a request carries a
  non-empty session, its response is not already a `5xx`, and the save fails, the session layer discards the response
  and returns an empty `500` built from `Response::default()`. That response carries no `Content-Security-Policy` and
  none of the uniform headers.
- **A reserved-prefix miss never becomes an application route.** The tests for `is_reserved_prefix`
  (`is_reserved_prefix_matches_bare_and_subpaths`, `is_reserved_prefix_rejects_spa_paths`) pin both directions: `/api`,
  `/api/v1/books`, `/auth/callback`, `/health/ready` and `/opds/library` are reserved, and `/`, `/library`, `/apiology`
  and `/authed` are not. A change that widened or narrowed the list would put a Problem Details response on a real
  application route, or `index.html` on a stale API client's request.
- **Path traversal against file serving.** A percent-encoded traversal attempt such as `/%2e%2e%2f%2e%2e%2fetc%2fpasswd`
  does not escape the build root, because `ServeDir` resolves and rejects it before this subject's dispatch runs. The
  request is answered as an ordinary application route (`index.html`, `200`, HTML policy), which neither reads outside
  the tree nor confirms whether the path exists.

## Security and operations

Every module in this subject is security-critical under the repository's comment policy, and its module docs say so.
Drift between the two policy layers and the fallback, or a second `Content-Security-Policy` source such as a reverse
proxy, breaks the route-class split without any error.

The hash pattern in `dist_validation.rs` is anchored (`^...$`) to close two gaps a looser check would leave: a
base64url-encoded hash (using `-` and `_`), which browsers ignore as a policy source instead of rejecting, and an
embedded carriage return and line feed, which would split the response header. `de_csp_endpoint` checks the reporting
endpoint for `"`, `;`, carriage return and line feed in the raw string before `url::Url::parse` runs, because that
parser percent-encodes a quote, which would defeat a check applied afterwards.

`Permissions-Policy` denies every capability it lists (`camera`, `microphone`, `geolocation`, `payment`, `usb`, `midi`,
`magnetometer`, `accelerometer`, `gyroscope`) with no runtime setting to relax it. `behind_https` gates
`Strict-Transport-Security` here; it also sets the session cookie's `Secure` attribute in the `SessionManagerLayer`,
which this Design does not own.

A self-hoster who puts Reverie behind a TLS-terminating reverse proxy sets `REVERIE_BEHIND_HTTPS=true` to get HSTS, and
must not let the proxy add its own `Content-Security-Policy` or `X-Frame-Options`. An operator who sets
`REVERIE_CSP_REPORT_ENDPOINT` points Reverie at a receiver that must accept unauthenticated browser `POST` requests;
Reverie does not host one.

## More information

- [Content Security Policy and security headers](../../../security/content-security-policy.md): the operator guide to
  what ships and how to tune it.
- [Reverse proxy](../../../deployment/reverse-proxy.md): the deployment notes, including the warning against duplicating
  these headers.
