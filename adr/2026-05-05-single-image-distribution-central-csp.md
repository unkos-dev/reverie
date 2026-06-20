---
status: accepted
date: 2026-05-05
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Single-image distribution with backend-served frontend and central CSP enforcement

## Context and Problem Statement

Reverie ships as a single Docker image (`ghcr.io/unkos-dev/reverie:vX.Y.Z`)
where the Rust backend compiles into one binary that serves both the
JSON API and the React/Vite frontend bundle on the same port (`:3000`).
Frontend assets land in `/srv/frontend` at image-build time, and the
backend reads `REVERIE_FRONTEND_DIST_PATH` at startup to mount the SPA
fallback router. This shape was introduced when the Dockerfile was
first written and reinforced by the Content-Security-Policy (CSP) implementation
(Content-Security-Policy), which wired a build-time `cspHashPlugin` in
Vite that emits `dist/csp-hashes.json`; the backend reads that sidecar
on startup and serves `index.html` with a strict CSP header containing
those exact script/style hashes.

The decision has never been captured as an ADR. It was effectively
inherited from the original scaffold and made load-bearing by the CSP implementation.
This becomes a problem when planning a staging runtime
(the staging runtime work): a reasonable
default for "deploy a frontend with hot iteration" is to split frontend
and backend into separate images so the frontend can rebuild without
recompiling Rust. That split was proposed and partially planned across
the homelab and Reverie agents on 2026-05-05 before the existing
single-image coupling was inspected. Splitting would have:

- Fragmented CSP enforcement (the sidecar lives in the frontend build
  but the header is currently emitted by the backend)
- Forced the self-hoster install path from `docker run reverie` to a
  multi-image compose stack with reverse proxy
- Introduced same-origin / CORS friction across the auth flow that
  the current architecture sidesteps entirely

The reversal happened mid-plan when the actual code was read. This ADR
records the architecture that already exists, the alternatives that
were considered, and the conditions that would force a future
reversal, so the next agent or contributor can see the load-bearing
constraints before proposing the same split again.

## Decision

Reverie distributes as **a single Docker image**. The Rust backend
serves both the JSON API and the static frontend bundle from one
binary on a single port. **CSP is centrally enforced by the backend**
using a hash sidecar (`csp-hashes.json`) emitted at frontend build
time and consumed at backend startup. There is no separate frontend
container in the production or staging deploy.

Concretely:

- **Build**: a single multi-stage `Dockerfile` produces one image. The
  Vite build emits `dist/` (including `csp-hashes.json`) into the
  frontend stage, the Rust build produces `reverie-api`, and the
  runtime stage copies both: frontend dist into `/srv/frontend`,
  binary into `/usr/local/bin`
- **Runtime**: the backend reads `REVERIE_FRONTEND_DIST_PATH` at
  startup, validates the directory and the `csp-hashes.json` sidecar
  (returns `Err` from `main` and exits with a non-zero status if
  either is missing or malformed), and mounts the SPA-fallback
  router. All HTTP traffic (API + frontend) terminates at the same
  Axum listener on `:3000`
- **Security headers**: the CSP HTML header is emitted by
  `backend/src/security/headers.rs` using the hashes loaded from the
  sidecar. There is one CSP enforcement point in the entire stack
- **Distribution to self-hosters**: `docker run -p 3000:3000 -e ... ghcr.io/unkos-dev/reverie:vX.Y.Z`
  is the supported install path. No reverse proxy required, no
  multi-container compose required for the minimal install
- **Dev-time iteration**: Vite's dev server runs separately on `:5173`
  with HMR; it proxies `/api`, `/auth`, and `/opds` to the backend on
  `:3000` (configured in `vite.config.ts`). The backend's static
  serving is bypassed entirely in dev. Production same-origin is
  preserved by serving everything from one process; dev same-origin is
  preserved by Vite's proxy
- **Visibility from outside the workspace**: solved separately by a
  Cloudflare Tunnel exposing the Vite dev server
  (the Cloudflare Tunnel setup), not by image
  rebuilds. Active-dev visibility is decoupled from the image
  distribution decision

## Consequences

- Good: **single CSP enforcement point**. The hash sidecar pattern
  ensures policy and assets are built together and consumed together.
  No drift between the policy emitter and the asset hashes. Audit
  scope for CSP changes is one file
- Good: **zero CORS in production and dev**. Cookies, CSRF,
  `SameSite`, OIDC redirect URIs, and XHR `credentials: include` all
  Just Work because requests are same-origin end-to-end. This is a
  meaningful reduction in attack surface and operational complexity
- Good: **simple self-hoster install**. `docker run` plus a Postgres
  container is the entire baseline. Matches how the target audience
  (homelabbers and small self-hosting communities) actually consume
  software
- Good: **atomic deploy unit**. One image tag = one rollback target.
  No version-skew possible between a `frontend@vX` and a
  `backend@vY` that disagree on API shape. CI bumps both halves in
  one PR (release-please already enforces this via `Cargo.toml` +
  `package.json` co-versioning)
- Good: **single healthcheck, single failure mode**. One `/health`
  endpoint covers both halves. Simplifies orchestration
  (`depends_on: condition: service_healthy`), Traefik routing, and
  deploy automation
- Bad: **frontend-only edits trigger an image rebuild that includes
  a Rust stage**. In practice, Docker layer caching means the Rust
  stage hits cache on frontend-only changes, so the rebuild is
  ~10–20s vs ~5s for a hypothetical split. Cost is real but
  small; addressed by NOT using image rebuilds for active dev
  iteration (see below)
- Bad: **the frontend stack and backend stack are coupled through
  the image**. You cannot deploy a frontend-only hotfix without
  shipping the backend binary. Acceptable for a project at Reverie's
  scale; would be a problem at a scale where independent FE/BE
  release cadence becomes a deliberate strategy
- Bad: **the backend Rust binary embeds the responsibility of
  static-asset serving and HTML response shaping**. Slightly outside
  the "API server" archetype most Rust-Axum tutorials present.
  Mitigated by keeping the static-serve module narrow
  (`backend/src/routes/spa.rs`) and the CSP module isolated
  (`backend/src/security/headers.rs`)
- Bad: **the build-time sidecar contract (`csp-hashes.json`) is an
  invariant the test suite must protect**. If a frontend refactor
  drops the plugin or changes the schema, the backend will panic at
  startup. Mitigated by the existing tests in
  `frontend/vite-plugins/__tests__/csp-hash.test.ts` and the backend
  startup validation that fails fast and loud
- Neutral: **iteration speed for the active-dev loop is unaffected**
  by the image-distribution decision. Active dev runs Vite + cargo
  watch directly in the workspace; the image is only relevant for
  staging and production deploys

## Alternatives Considered

- **Two-image split (separate `reverie-backend` + `reverie-frontend`
  images, served behind a reverse proxy that fronts both at the same
  origin).** Rejected for three independent reasons:
  1. CSP central enforcement breaks. The hash sidecar pattern would
     either need to be duplicated in the frontend image (CSP policy
     drift risk, two enforcement points) or relocated to the reverse
     proxy (proxy needs build-artefact access, brittle, hash
     freshness becomes a deploy-coordination problem)
  2. Self-hoster install regresses from `docker run` to a multi-image
     compose stack with mandatory reverse proxy. Real friction for
     the audience the project targets
  3. Same-origin auth model collapses. The split forces either a
     reverse proxy that fakes same-origin (mandatory infrastructure,
     not optional) or accepting CORS complexity (preflight, cookie
     `SameSite`, OIDC redirect fragility, CSRF posture changes)

  The motivating benefit (faster frontend-only iteration) is
  ~10–20s in practice with Docker layer caching, and the active-dev
  loop runs Vite directly anyway. The cost-benefit is decisively
  against the split

- **Embed the frontend bundle into the Rust binary via
  `include_dir!` or `rust-embed`.** Rejected: would lose the
  ability to swap frontend assets without recompiling Rust, would
  inflate the binary size, and would complicate the CSP sidecar
  pattern (the JSON file would need a parallel `include_str!` slot
  with no compile-time guarantee it stays in sync with the embedded
  asset hashes). The current `REVERIE_FRONTEND_DIST_PATH` env-var
  pattern keeps the boundary cleanly volume-mounted at runtime,
  which matches how Docker layer caching wants to operate

- **Separate frontend image served by Nginx, with CSP injected by
  Nginx config templated at build time.** Rejected: the Vite hash
  sidecar is JSON, not Nginx config syntax. Templating it requires
  either (a) a build-time post-processor that emits Nginx
  fragments, adding a second toolchain step that the CI pipeline
  must validate, or (b) a runtime Nginx Lua module that reads the
  sidecar, adding a non-trivial dependency. Either way the policy
  no longer lives in the security review's natural home
  (`backend/src/security/headers.rs`)

- **Cloudflare Workers / edge-side CSP injection.** Rejected for
  this project: Reverie is self-hosted; the deployment target is
  homelabs and small self-hosting communities, not a hosted SaaS
  with a Cloudflare-owned edge in front of every install. Edge-side
  policy works for projects whose deployment model includes a
  controlled edge; Reverie's doesn't

## Revisit Conditions

Open a superseding ADR if any of the following happen:

- **Distribution model changes**. If Reverie pivots to ship as a
  Helm chart, multi-container Docker stack, or hosted service as
  the primary install path, the single-image rationale (item 2 in
  the rejection list above) loses force. The whole decision is
  worth revisiting from scratch
- **CSP enforcement moves outside the backend**. If we adopt
  edge-side policy injection across all services (e.g. an Authentik
  forward-auth plugin or a homelab-wide Traefik middleware that
  emits CSP for every routed app), the central-enforcement argument
  no longer differentiates against the split
- **Frontend stack adopts SSR or a runtime that the backend cannot
  co-locate with**. If a future React Server Components or Next.js
  adoption requires a Node.js process to render HTML, the frontend
  is no longer just a static-asset bundle and the single-binary
  model breaks anyway
- **Independent FE/BE release cadence becomes a deliberate
  strategy**. If the project grows enough that frontend hotfixes
  need to ship without bumping the backend version (e.g. the design
  system iterates faster than the API), the coupling cost crosses
  the threshold where the split's ergonomic wins matter
- **A new security model requires per-service trust boundaries**.
  Some compliance regimes mandate that web tier and app tier run as
  separate processes for least-privilege isolation. Not a current
  concern but a real reason to revisit

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Related: [`adr/2026-04-30-adopt-architecture-decision-records.md`](2026-04-30-adopt-architecture-decision-records.md):
  the meta-ADR that established this format and process
- Related: the CSP introduction (Done) that made the single-image model
  load-bearing for security. Reading the CSP documentation alongside this ADR is
  necessary to understand why the rejection of the split is more
  than a preference
- Related: the staging runtime master ticket. This ADR's parent context;
  the decision recorded here directly shapes the compose stack,
  Dockerfile, and CI scope of the staging deploy
- Tracker: the ticket commissioning this ADR
- Code references:
  - `Dockerfile`: single multi-stage build
  - `backend/src/lib.rs::build_router_with_session_store`:
    `routes::spa::router_enabled` mount
  - `backend/src/lib.rs::run`: `frontend_dist_path` startup
    validation
  - `backend/src/security/headers.rs`: CSP enforcement, including
    `spa_fallback_response`
  - `backend/src/security/dist_validation.rs`: startup-time
    sidecar contract enforcement
  - `frontend/vite-plugins/csp-hash.ts`: the build-time sidecar
    emitter
  - `frontend/vite.config.ts`: dev-mode proxy + relaxed dev CSP
