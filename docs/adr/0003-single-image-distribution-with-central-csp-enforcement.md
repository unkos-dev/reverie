---
type: ADR
profile-version: 1
id: "REV-ADR-0003"
title: "Single-image distribution with central CSP enforcement"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-05"
decision-makers:
  - "John Unkovich"
informed:
  - "Reverie contributors"
---

# Single-image distribution with central CSP enforcement

## Context and problem statement

Reverie ships as a single Docker image (`ghcr.io/unkos-dev/reverie:vX.Y.Z`) where the Rust backend compiles into one
binary that serves both the JSON API and the React/Vite frontend bundle on the same port (`:3000`). Frontend assets
land in `/srv/frontend` at image-build time, and the backend reads `REVERIE_FRONTEND_DIST_PATH` at startup to mount
the SPA fallback router. This shape was introduced when the Dockerfile was first written and became load-bearing when
Content-Security-Policy (CSP) enforcement was added: a build-time `cspHashPlugin` in Vite emits `dist/csp-hashes.json`,
and the backend reads that sidecar on startup and serves `index.html` with a strict CSP header containing those exact
script and style hashes.

This coupling had never been recorded as a decision. It resurfaced while planning a staging deployment, where a
reasonable-looking default for "deploy a frontend with hot iteration" is to split frontend and backend into separate
images so the frontend can rebuild without recompiling Rust. That split was proposed before the existing single-image
coupling was inspected, and was reversed once the code was read. Which distribution shape should Reverie use, and
where should CSP enforcement live?

## Decision drivers

- CSP enforcement must have one enforcement point: the hash sidecar emitted at frontend build time and the header
  emitted at backend startup must not drift apart.
- The self-hoster install path must stay a single `docker run` command with no mandatory reverse proxy or
  multi-container compose stack.
- The auth flow (cookies, CSRF, `SameSite`, OIDC redirect URIs) depends on same-origin requests end-to-end.

## Considered options

- Single Docker image, backend-served frontend, CSP enforced centrally by the backend
- Two-image split: separate `reverie-backend` and `reverie-frontend` images behind a reverse proxy
- Embed the frontend bundle into the Rust binary via `include_dir!` or `rust-embed`
- Separate frontend image served by Nginx, with CSP injected by Nginx config templated at build time
- Cloudflare Workers or other edge-side CSP injection

## Decision outcome

Chosen option: **single Docker image, backend-served frontend, CSP enforced centrally by the backend**, because it
keeps CSP enforcement at one point, preserves same-origin for the auth flow without a reverse proxy, and matches the
`docker run` install path the self-hosting audience expects.

Concretely:

- Build: a single multi-stage `Dockerfile` produces one image. The Vite build emits `dist/` (including
  `csp-hashes.json`) into the frontend stage, the Rust build produces `reverie-api`, and the runtime stage copies
  both: frontend dist into `/srv/frontend`, binary into `/usr/local/bin`.
- Runtime: the backend reads `REVERIE_FRONTEND_DIST_PATH` at startup, validates the directory and the
  `csp-hashes.json` sidecar (exits non-zero if either is missing or malformed), and mounts the SPA-fallback router.
  All HTTP traffic, API and frontend, terminates at the same Axum listener on `:3000`.
- Security headers: the CSP HTML header is emitted by `backend/src/security/headers.rs` using the hashes loaded from
  the sidecar. There is one CSP enforcement point in the stack.
- Distribution to self-hosters: `docker run -p 3000:3000 -e ... ghcr.io/unkos-dev/reverie:vX.Y.Z` is the supported
  install path, with no reverse proxy and no multi-container compose stack required for the minimal install.
- Dev-time iteration: Vite's dev server runs separately on `:5173` with HMR, and forwards `/api`, `/auth`, and
  `/opds` to the backend on `:3000`. The backend's static serving is bypassed entirely in dev; same-origin is
  preserved by Vite's proxy instead.
- Visibility from outside the workspace is solved separately, by tunnelling the Vite dev server, not by image
  rebuilds; active-dev visibility is decoupled from the image-distribution decision.

### Consequences

- Positive: single CSP enforcement point. The hash sidecar pattern ensures policy and assets are built together and
  consumed together, with no drift between the policy emitter and the asset hashes, and a one-file audit scope for
  CSP changes.
- Positive: zero CORS in production and dev. Cookies, CSRF, `SameSite`, OIDC redirect URIs, and `credentials: include`
  requests all work because requests are same-origin end-to-end, reducing both attack surface and operational
  complexity.
- Positive: a simple self-hoster install. `docker run` plus a Postgres container is the entire baseline, matching how
  the target audience of homelabbers and small self-hosting communities consumes software.
- Positive: an atomic deploy unit. One image tag is one rollback target, with no version-skew possible between a
  frontend and backend that disagree on API shape.
- Positive: a single healthcheck and a single failure mode. One `/health` endpoint covers both halves, simplifying
  orchestration and deploy automation.
- Negative: a frontend-only edit triggers an image rebuild that includes a Rust build stage. In practice Docker layer
  caching means the Rust stage hits cache on frontend-only changes, so the cost is real but small, and is not paid
  during active development, which runs Vite directly.
- Negative: the frontend stack and the backend stack are coupled through the image. A frontend-only hotfix cannot ship
  without the backend binary. Acceptable at Reverie's current scale; would become a problem if independent
  frontend/backend release cadence became a deliberate strategy.
- Negative: the backend Rust binary carries static-asset serving and HTML response shaping, slightly outside the
  typical Rust-Axum "API server" archetype. Mitigated by keeping the static-serve module narrow
  (`backend/src/routes/spa.rs`) and the CSP module isolated (`backend/src/security/headers.rs`).
- Negative: the build-time sidecar contract (`csp-hashes.json`) is an invariant the test suite must protect. If a
  frontend refactor drops the plugin or changes its schema, the backend panics at startup. Mitigated by the tests in
  `frontend/vite-plugins/__tests__/csp-hash.test.ts` and by startup validation that fails fast and loud.
- Positive: the active-dev iteration loop is unaffected by the image-distribution decision, since it runs Vite and
  `cargo watch` directly in the workspace; the image only matters for staging and production deploys.

### Confirmation

`backend/src/security/headers.rs` emits the CSP header from the loaded hashes and serves the SPA fallback response;
`backend/src/security/dist_validation.rs` validates the `csp-hashes.json` sidecar and the frontend dist directory at
backend startup and fails the process when either is missing or malformed; `frontend/vite-plugins/csp-hash.ts` emits
that sidecar at build time, with its contract covered by `frontend/vite-plugins/__tests__/csp-hash.test.ts`. `Dockerfile`
is the single multi-stage build that produces the one distributed image, and `.github/workflows/docker-publish.yml`
publishes it.

## Pros and cons of the options

### Single Docker image, backend-served frontend, CSP enforced centrally by the backend

- Positive: one enforcement point for CSP, one deploy artefact, no reverse proxy required for the minimal install.
- Negative: frontend and backend release cadence are coupled through the image.

### Two-image split

- Negative: CSP central enforcement breaks. The hash sidecar pattern would either need duplicating in the frontend
  image, risking policy drift between two enforcement points, or relocating to a reverse proxy, which would need
  build-artefact access and turns hash freshness into a deploy-coordination problem.
- Negative: the self-hoster install path regresses from `docker run` to a multi-image compose stack with a mandatory
  reverse proxy.
- Negative: the same-origin auth model collapses, forcing either a reverse proxy that fakes same-origin or accepting
  CORS complexity (preflight, cookie `SameSite`, OIDC redirect fragility, CSRF posture changes).
- Neutral: the motivating benefit, faster frontend-only iteration, is on the order of ten to twenty seconds in
  practice with Docker layer caching, and the active-dev loop already runs Vite directly.

### Embed the frontend bundle into the Rust binary

- Negative: loses the ability to swap frontend assets without recompiling Rust and inflates the binary size.
- Negative: complicates the CSP sidecar pattern, since the JSON file would need a parallel embedded slot with no
  compile-time guarantee it stays in sync with the embedded asset hashes.

### Separate frontend image served by Nginx with templated CSP

- Negative: the Vite hash sidecar is JSON, not Nginx config syntax. Templating it needs either a build-time
  post-processor that emits Nginx fragments, adding a toolchain step the CI pipeline must validate, or a runtime
  Nginx module that reads the sidecar, adding a non-trivial dependency.
- Negative: the policy no longer lives with the rest of the security-relevant backend code.

### Cloudflare Workers or other edge-side CSP injection

- Negative: edge-side policy assumes a deployment model with a controlled edge in front of every install. Reverie is
  self-hosted, targeting homelabs and small self-hosting communities, not a hosted service with an owned edge.

## More information

Reading `docs/security/content-security-policy.md` alongside this record helps explain why the rejection of the
two-image split is more than a preference: CSP is the reason the coupling is load-bearing, not just a historical
accident of how the Dockerfile was first written.

Open a superseding record if any of the following happen:

- The distribution model changes, for example a pivot to a Helm chart, a multi-container Docker stack, or a hosted
  service as the primary install path, which would remove the self-hoster install-path rationale.
- CSP enforcement moves outside the backend, for example an edge-side policy adopted across all services, which would
  remove the central-enforcement argument against the split.
- The frontend stack adopts server-side rendering or a runtime the backend cannot co-locate with, since the frontend
  would no longer be just a static-asset bundle and the single-binary model would break regardless of this decision.
- Independent frontend/backend release cadence becomes a deliberate strategy, so that frontend hotfixes need to ship
  without bumping the backend version.
- A security model requiring per-service trust boundaries applies, since some compliance regimes mandate that the web
  tier and the app tier run as separate processes for least-privilege isolation.

Re-recorded from adr/2026-05-05-single-image-distribution-central-csp.md (decided 2026-05-05); history holds the original.
