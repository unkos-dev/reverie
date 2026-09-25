---
type: ADR
profile-version: 1
id: "REV-ADR-0011"
title: "JSON API conventions for the browser-facing REST surface"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-05-22"
decision-makers:
  - "John Unkovich"
---

# JSON API conventions for the browser-facing REST surface

## Context and problem statement

The API conventions work introduces the first JSON REST surface a browser consumes at scale: `/api/v1/books`,
`/api/v1/books/{id}`, `/api/v1/works/{id}`, plus search, shelves, series, manifest metadata, users, and persisted
settings. Today the only browser-facing JSON paths are auth, theme, cover, and a handful of one-shot endpoints
(ingestion and enrichment triggers, token issue); the dominant read surface is OPDS, an Atom XML feed for e-readers.
This work adds a parallel JSON surface large enough that ad-hoc per-handler choices on naming, error envelope,
pagination, and CSRF would fork the codebase against itself. Every convention here is also referenced by the frontend
`frontend/src/api/` client and must hold across the wire boundary, so deciding it once avoids backend/frontend contract
drift. What set of wire-shape conventions should the JSON API surface adopt before the first handler lands?

## Decision drivers

- Ad-hoc per-handler choices on field naming, error envelope, pagination, and CSRF would fork the codebase against
  itself as more handlers land.
- The frontend `frontend/src/api/` client shares every shape decision with the backend across the wire boundary;
  deciding conventions once avoids backend/frontend contract drift.
- The governing principle is to default to IETF, OWASP, and W3C standards; any deviation needs a conscious decision, a
  measurably better outcome, and its own ADR.
- Reverie's threat model is a multi-user, network-exposed instance, which governs the CSRF and existence-disclosure
  choices.
- The ingestion and enrichment pipeline writes asynchronously to the `manifestations` table, so list pagination must
  stay stable under concurrent inserts and must not degrade as the library grows toward the 50K+ book range Reverie
  targets.

## Considered options

- A fixed convention set for the browser-facing JSON surface
- Keep `{"error": "<msg>"}` error envelope; defer RFC 9457
- Cookie + `SameSite=Lax` only (no synchronizer token)
- Custom CSRF cookie (double-submit pattern)
- Offset pagination
- Bearer-token CSRF defence only
- Custom problem-type host (not `reverie.example`)

## Decision outcome

Chosen option: **a fixed convention set for the browser-facing JSON surface**, because standards anchor the wire and
security choices, while fixing the whole set before the first handler lands keeps the backend and the frontend client on
one shared shape.

The conventions choose snake_case fields, RFC 3339 UTC timestamps, RFC 9457 Problem Details, failure-class HTTP status
codes, explicit nulls, cursor pagination with Link headers, synchronizer-token CSRF protection for cookie-authenticated
mutations, existence-hiding 404 responses, JSON Merge Patch, matching GET representations for resource PATCH endpoints,
and If-Match preconditions where optimistic concurrency is required. These are the selected API contracts; handler and
middleware wiring is outside this decision record.

### Consequences

- Positive: the wire and security choices use public standards where they fit, while local conventions are explicit
  enough for backend and frontend contributors to share one contract.
- Positive: the frontend `frontend/src/api/` client and backend handlers share one shape definition: `snake_case`, RFC
  9457 body, RFC 8288 pagination, RFC 7396 patches. Cross-cutting drift between backend and frontend types is
  structurally bounded.
- Positive: adopting the synchronizer token during the greenfield phase is cheap. Retrofitting after production
  cookie-authed mutation traffic existed would be a months-long rollout.
- Negative: `next_cursor` plus the Link header duplicate the same signal; the redundancy is worth it for the JS-client
  ergonomics win and is documented as deliberate.

## Pros and cons of the options

### A fixed convention set for the browser-facing JSON surface

- Positive: established standards govern the error, pagination, patch, timestamp, and CSRF choices.
- Positive: backend and frontend share one shape definition, so contract drift across the wire boundary is structurally
  bounded.
- Negative: adopting several conventions at once (RFC 9457, the synchronizer token) touches existing tests and existing
  endpoints that predate the decision.

### Keep `{"error": "<msg>"}` error envelope; defer RFC 9457

- Positive: cheaper in the near term, zero test churn; the frontend would parse a `{status, error}` pair into
  `ApiError`.
- Negative: rejected on standards-default grounds. The frontend has to grow typed error handling either way, because
  problem-slug discrimination matters for CSRF-rotation retry, role-changed retry, and similar flows. Building that on
  top of a non-standard envelope means a future adopter coming from any other RFC 9457-aware stack has to learn
  Reverie's shape, and migrating later would mean re-versioning every endpoint that used the old shape.

### Cookie + `SameSite=Lax` only (no synchronizer token)

- Positive: the cheapest CSRF story; in practice many small apps stop here.
- Negative: rejected because Reverie's threat model is the multi-user exposed instance. External contributors and
  self-hosters audit Reverie expecting OWASP-default defences, and the synchronizer token is the OWASP-blessed primary;
  not adopting it would be a deviation requiring its own ADR.

### Custom CSRF cookie (double-submit pattern)

- Positive: also an OWASP-acceptable pattern; a second cookie carries the token and the frontend reads it via JS and
  echoes it as a header.
- Negative: rejected because the synchronizer token is the OWASP "strongest" recommendation, and Reverie already has a
  server-side session store (`tower-sessions`), so the cost of the synchronizer pattern is essentially one more session
  key, whereas double-submit needs a second cookie and JS to read it.

### Offset pagination

- Positive: simpler client code, `?page=N&size=20`.
- Negative: rejected on correctness grounds. Reverie's enrichment pipeline writes asynchronously, so the row count of
  the `manifestations` table shifts mid-scroll; offset pagination would display duplicates and skip rows under that
  workload.

### Bearer-token CSRF defence only

- Positive: treats the API as if it were a public API, requiring `Authorization: Bearer <token>` on every request and
  skipping CSRF entirely.
- Negative: rejected because the browser UI uses cookie sessions for the same reason the existing `/auth/login` and
  `/auth/me` flows do (OIDC-driven login, no per-request token management on the client side). Hybrid stacks need
  browser-CSRF defence for the cookie surface and token-auth for the API client surface separately: Reverie ships CSRF
  for browser cookies, and bearer tokens exist on a separate endpoint set (`/api/v1/tokens` issues device tokens; those
  endpoints sit behind `BasicOnly`/`Bearer` extractors that bypass the cookie session entirely and thus don't need CSRF,
  per `backend/src/auth/middleware.rs`).

### Custom problem-type host (not `reverie.example`)

- Positive: a resolvable host such as `https://reverie.unkos.dev/probs/...` would let a problem-type URI actually
  dereference.
- Negative: deferred rather than rejected outright. RFC 9457 §3.1 explicitly says the URI does not need to dereference
  at first, and nothing today depends on the URI resolving. When a canonical project URL lands, a single pass through
  `error/problems.rs` swaps the prefix, because the URIs are stable in their slugs.

## More information

IETF specs cited: RFC 9457 (Problem Details, formerly RFC 7807), RFC 8288 (Web Linking / Link header), RFC 7396 (JSON
Merge Patch), RFC 9110 §12 (content negotiation), RFC 9110 §13.1 (`If-Match`), RFC 3339 (date format), RFC 8259 (JSON).
OWASP cheat sheet: Cross-Site Request Forgery Prevention.

Sibling ADR: [backend auxiliary crates](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md) (the
`axum-extra`, `serde_with`, and `subtle` dependency choices this decision relies on).

Sibling ADR: [frontend data layer dependencies](./0010-frontend-data-layer-dependencies-react-query-and-dnd-kit.md) (the
frontend dependency adoptions for the same API conventions work).
