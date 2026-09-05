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
`frontend/src/api/` client and must hold across the wire boundary, so deciding it once avoids backend/frontend
contract drift. What set of wire-shape conventions should the JSON API surface adopt before the first handler lands?

## Decision drivers

- Ad-hoc per-handler choices on field naming, error envelope, pagination, and CSRF would fork the codebase against
  itself as more handlers land.
- The frontend `frontend/src/api/` client shares every shape decision with the backend across the wire boundary;
  deciding conventions once avoids backend/frontend contract drift.
- The governing principle is to default to IETF, OWASP, and W3C standards; any deviation needs a conscious decision,
  a measurably better outcome, and its own ADR.
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

Chosen option: **a fixed convention set for the browser-facing JSON surface**, because each convention is an IETF,
OWASP, or W3C standard, and fixing them before the first handler lands keeps the backend and the frontend
`frontend/src/api/` client on one shared shape.

Field naming uses `snake_case` (for example `cover_url`, `next_cursor`, `created_at`). Reverie's `User` struct
(`backend/src/models/user.rs`) already emits `snake_case` because Rust struct fields are `snake_case` by convention
and serde's default behaviour is round-trip-preserving with no `#[serde(rename_all)]`. RFC 8259 (the JSON spec) is
naming-agnostic; the de-facto convention across modern public APIs is mixed (Stripe and Slack use `snake_case`;
GitHub mixes `snake_case` body fields with `camelCase` GraphQL). Reverie picks `snake_case` for one reason that
overrides convention preference: it removes a per-struct serde attribute that would otherwise be the load-bearing
line in every handler. The cost is one `eslint-plugin-camelcase` carve-out on the frontend's API client surface.

Timestamps serialise as RFC 3339 strings (for example `"2026-05-22T13:42:00.123Z"`). Two invariants follow from
Postgres normalising `timestamptz` to UTC. Values are `Z`-terminated, never a numeric offset. Sub-second digits are
emitted only when non-zero, so consumers must accept variable fractional precision rather than a fixed width. Which
datetime crate the backend uses, and therefore what makes a DTO field carry this shape, is a separate decision
recorded in [chrono is the first-party datetime crate](../../adr/2026-08-05-first-party-datetime-crate.md); this
convention governs the wire format only. The RFC 3339 shape matches the existing OPDS Atom feed (Atom's `<updated>`
and `<published>` use the same shape per RFC 4287 §3.3), so the two surfaces stay consistent for any operator who
reads both.

Errors emit `application/problem+json` per RFC 9457 (formerly RFC 7807; same wire format, `application/problem+json`
unchanged) with the following body shape:

```json
{
  "type": "https://reverie.example/probs/<problem-slug>",
  "title": "<short human-readable summary>",
  "status": 422,
  "detail": "<longer explanation, may include instance-specific info>",
  "instance": "/api/v1/books/abc-123"
}
```

`type` is a stable URI per error variant (`not-found`, `unauthorized`, `forbidden`, `validation`, `malformed-header`,
`csrf-missing`, `csrf-mismatch`, `if-match-required`, `if-match-mismatch`, `system-shelf-immutable`, `internal`). Per
RFC 9457 §3.1 the URI identifies the problem type and does not need to dereference at first; Reverie registers
concrete URIs as the deployment story matures, and `reverie.example` is a placeholder host until that decision lands.
`title` and `status` mirror the HTTP status reason phrase and numeric code. `detail` is the caller-visible message.
`instance` is the request path (RFC 9457 §3.1 makes this optional but recommended; Reverie always includes it for
debuggability). The `Content-Type` is `application/problem+json`, not `application/json`, which signals to RFC
9457-aware clients that the body is a Problem Details document and not a domain object with an `error` field: this
matters for `fetch().then(res => res.json())` flows that branch on shape. `instance` is the request path; `AppError`
is a value, not a request-coupled construct, so the path is captured by a `problem_instance_layer` tower middleware
(`backend/src/error/instance.rs`) that stores the request path into a `tokio::task_local!` slot on request entry, and
`AppError::into_response` reads from that task-local. The middleware mounts on the outermost composite router,
wrapping matched API routes and the composite fallback, so that reserved-prefix typos (`/api/v1/__nope__`,
`/auth/__nope__`) carry the `instance` field too. The slot is `None` outside an HTTP request (for example, unit tests
calling `AppError::Validation(...).into_response()` directly), in which case `instance` is omitted from the body,
which RFC 9457 §3.1 permits.

Status codes are assigned by failure class, per the definitions in RFC 9110 §15.5. §15.5.1 (400 Bad Request) covers a
request the server cannot or will not process due to a client error in the request's own grammar, and two failure
classes map here. Syntax failures at the decode boundary (extractors, query/path deserialisation: the request cannot
be parsed into the shape the handler expects) are mapped by `AppError::MalformedQuery`. Header failures are mapped by
`AppError::MalformedHeader` with problem type `malformed-header`: this covers both malformed header field syntax and
a syntactically valid header form the API refuses by policy (an `If-Match` wildcard, an entity-tag list, or a
repeated header instance), the latter under §15.5.1's broader "cannot or will not process the request due to a client
error" clause rather than a grammar violation as such. RFC 9110 defines "content" as the message body, so 422
Unprocessable Content (RFC 9110 §15.5.21) stays scoped to the body: requests whose content parses correctly but whose
instructions violate the documented contract (unknown or invalid field values, business-rule rejections) are 422,
mapped by `AppError::Validation`. Semantic codes are reserved for well-formed requests that fail against current
server state rather than against their own shape: 404 for existence (including the deliberate 404-over-403 ownership
convention below), 405 Method Not Allowed (RFC 9110 §15.5.6) when the target resource exists but does not support the
request's method, emitted as problem details with the `Allow` header intact, 409 for conflict, 412 Precondition
Failed when a precondition evaluates false (RFC 9110 §13.1), and 428 Precondition Required when a required
precondition is missing entirely (RFC 6585 §3). Any new error path is checked against two tests before it picks a
status code. The recovery-guidance test: a status code whose standard recovery action cannot succeed for that input
is the wrong code. The motivating shape is an `If-Match` header that is syntactically malformed (not a well-formed
entity-tag): a 412's implied recovery (refresh the tag and retry) can never succeed against a grammar error, since no
refreshed tag will ever parse, so that failure belongs in 400, because the defect is in the request's own grammar,
not its instructions. The closed-domain test: input or stored data that is valid within its own domain (a database
enum variant, a schema-legal document) must never surface as 500, because internal errors are reserved for genuine
invariant violations, not for values the domain already accepts as legitimate members. The deliberate, documented
exceptions to these tests stand: the schema-drift decode boundary in the library module intentionally fails loudly,
and the existence-hiding 404s below are a security choice, not drift.

Nullable fields serialise as `null`, never omitted. TypeScript consumers read `field: T | null` (always present,
sometimes null), not `field?: T` (sometimes absent, sometimes the value). The distinction matters: `field?: T`
collapses "field absent" and "field present with `undefined`" into the same shape, which breaks JSON Merge Patch (RFC 7396) semantics, where `{"field": null}` means "clear" and `{}` means "leave unchanged". Reverie's model structs
already follow this pattern, with no `skip_serializing_if` on a nullable field anywhere.

`/api/v1/books` and every list endpoint paginates with cursors, not offsets. Cursors are opaque base64url payloads
carrying the sort key(s) plus a tiebreaker; see the multi-column sort stack ADR
(../../adr/2026-07-07-multi-column-sort-stack.md) for the current cursor shape. Pagination model is not
IETF-specified; modern consensus across GitHub, Stripe, Slack, and Twitter v2 is cursor-based for one reason: cursors
are stable under concurrent inserts. Reverie's enrichment pipeline writes asynchronously to `manifestations`, so
offset pagination would shift the page boundary mid-scroll. Cursors are also O(log N) per page at scale; offsets
degrade as the table grows, and Reverie targets 50K+ library sizes.

Every paginated response includes an RFC 8288 `Link` header with `rel="next"` (and `rel="prev"`, `rel="first"` when
applicable), for example `Link: </api/v1/books?cursor=eyJ0eXAi...>; rel="next"`, plus a `next_cursor: string | null`
field in the JSON body. The Link header is the IETF-canonical pagination signal: it matches OPDS Atom's `<link
rel="next">` and is the shape GitHub, Stripe, and the JSON-API spec all converge on. The body field exists because
`fetch()` does not auto-parse Link headers and the frontend's react-query infinite-query helper consumes a body field
with less ceremony than a parsed Link header. The two are guaranteed to carry the same information; either is
sufficient.

`SameSite=Lax` cookies alone are insufficient per the OWASP CSRF prevention cheat sheet. Reverie adopts the OWASP
synchronizer-token pattern. On session creation in `routes/auth.rs::callback`, the backend generates a 32-byte random
token (`rand::fill`, `backend/src/auth/token.rs`), base64url-encoded, stored under `csrf_token` in the session. The
token is surfaced to the browser via the existing `GET /auth/me` JSON response (already called on mount in the
frontend's `ThemeProvider`), which carries a `csrf_token: String` field. A tower middleware layer, `csrf_required`
(`backend/src/security/csrf.rs`), mounts on every non-safe-verb (POST/PUT/PATCH/DELETE) request under `/api/v1/*`.
The middleware reads `X-CSRF-Token` from request headers; if absent it returns 428 with `type: ".../csrf-missing"`;
if present but it does not match the session value under constant-time compare (`subtle::ConstantTimeEq`), it returns
403 with `type: ".../csrf-mismatch"`. The layer wraps the matched route, so a session-authenticated mutation without
a valid token receives the CSRF problem before any route-level status is decided, including 405 for an unsupported
method and 404 for a missing row. The token rotates on privilege change (when `session_version` increments).
`POST /auth/logout` is exempt: logging out destroys the session and therefore the token, so a logged-out user has no
session to attach a token to. `SameSite=Lax`, the CSP API layer, and the Bearer token requirement for older API
surfaces all remain in place as belt-and-braces alongside the synchronizer token, which is the primary CSRF defence
for browser cookie-authed operations.

When a request targets a resource the user lacks row-level-security visibility on (`GET /api/v1/books/{id}` where the
manifestation row is filtered out), the handler returns 404 Not Found, not 403 Forbidden. This is OWASP
defence-in-depth: 403 confirms the resource exists, which is information disclosure. Under `acquire_with_rls`, the
row is invisible to the query, so the existing zero-rows to `AppError::NotFound` mapping produces the correct shape
with no special handling. `GET /api/v1/works/{id}` is an edge case: the `works` table has no row-level security, so
the handler explicitly gates on whether the user can see at least one `manifestation` for the work, returning 404
when zero are visible; this is tested explicitly for child accounts.

`PATCH` endpoints accept RFC 7396 JSON Merge Patch bodies. A missing key means "leave unchanged"; an explicit `null`
means "clear". Server-side decoding uses `serde_with::rust::double_option` for sparse-update plumbing:

```rust
#[serde(default, with = "::serde_with::rust::double_option")]
pub field: Option<Option<T>>, // None = absent, Some(None) = null/clear, Some(Some(v)) = set
```

`PATCH /api/v1/books/{id}/metadata` uses this convention, which defaults forward for any future PATCH surface. JSON
Merge Patch is the standard sparse-update format; library support exists across every client language Reverie is
likely to care about, and RFC 7396 is explicitly limited to merging object trees (no array merging), which is what
Reverie needs.

Every new or changed endpoint that accepts a PATCH exposes a `GET` at the same URI returning the same representation,
field for field, so a read-modify-write flow and any HTTP precondition layered on top of it both target one resource
instead of two shapes that can drift apart. The action-style endpoints under the enrichment review queue (accept,
reject, revert, lock/unlock) are the documented exception: they are verbs, not resource state, and have no matching
representation to read back. The pre-existing user admin surface (`PATCH /api/v1/users/{id}` and its role,
child-status, and account-status PUT verbs, whose only read-back is the paginated users list) and
`PATCH /api/v1/auth/me/theme` predate this convention and do not yet conform to it; both are tracked for retrofit
rather than exempted.

Optimistic-concurrency endpoints (shelf reorder, future metadata writes that need ETag protection) require an
`If-Match` header. Absent, the response is 428 Precondition Required with `type: ".../if-match-required"`. Mismatch is
412 Precondition Failed with `type: ".../if-match-mismatch"`. The ETag value is computed by the handler, typically a
hash of the resource state or its `updated_at`.

`Accept: application/json` is the default for the API surface. Errors emit `application/problem+json`. OPDS routes
remain on `application/atom+xml`, which is out of scope for this convention set. There is no `Accept` header parsing
yet; the API defaults to JSON unconditionally on `/api/v1/*` paths. If a future client needs content negotiation, the
handler picks it up at that point.

### Consequences

- Positive: every convention is an IETF or OWASP standard. New contributors and external integrators find every
  shape decision already grounded in a public spec; reviewer ceremony around "why this shape" collapses.
- Positive: the frontend `frontend/src/api/` client and backend handlers share one shape definition: `snake_case`,
  RFC 9457 body, RFC 8288 pagination, RFC 7396 patches. Cross-cutting drift between backend and frontend types is
  structurally bounded.
- Positive: the move from `{"error": "<msg>"}` to RFC 9457 is the only inherited-divergence-to-standard move; it is
  surgical, a single `IntoResponse` implementation plus a `test_support` helper.
- Positive: adopting the synchronizer token during the greenfield phase is cheap. Retrofitting after production
  cookie-authed mutation traffic existed would be a months-long rollout.
- Negative: the RFC 9457 envelope breaks every existing test that asserts `body["error"]`. An `assert_problem` test
  helper collapses the diff, but the change still touches several existing test files (auth, ingestion, enrichment,
  metadata, tokens).
- Negative: `next_cursor` plus the Link header duplicate the same signal; the redundancy is worth it for the
  JS-client ergonomics win and is documented as deliberate.
- Negative: the `instance` field requires a tokio task-local plus a tower middleware, roughly 30 lines of code
  including tests, an acceptable cost for RFC 9457 conformance and request-path debuggability.
- Negative: `csrf_token` adds one field to the `/auth/me` response, a small shape diff in
  `backend/src/models/user.rs`, consumed by the frontend via a module-level setter with no other API surface change.

## Pros and cons of the options

### A fixed convention set for the browser-facing JSON surface

- Positive: every convention traces to an IETF, OWASP, or W3C standard rather than a first-party invention.
- Positive: backend and frontend share one shape definition, so contract drift across the wire boundary is
  structurally bounded.
- Negative: adopting several conventions at once (RFC 9457, the synchronizer token) touches existing tests and
  existing endpoints that predate the decision.

### Keep `{"error": "<msg>"}` error envelope; defer RFC 9457

- Positive: cheaper in the near term, zero test churn; the frontend would parse a `{status, error}` pair into
  `ApiError`.
- Negative: rejected on standards-default grounds. The frontend has to grow typed error handling either way,
  because problem-slug discrimination matters for CSRF-rotation retry, role-changed retry, and similar flows.
  Building that on top of a non-standard envelope means a future adopter coming from any other RFC 9457-aware stack
  has to learn Reverie's shape, and migrating later would mean re-versioning every endpoint that used the old shape.

### Cookie + `SameSite=Lax` only (no synchronizer token)

- Positive: the cheapest CSRF story; in practice many small apps stop here.
- Negative: rejected because Reverie's threat model is the multi-user exposed instance. External contributors and
  self-hosters audit Reverie expecting OWASP-default defences, and the synchronizer token is the OWASP-blessed
  primary; not adopting it would be a deviation requiring its own ADR.

### Custom CSRF cookie (double-submit pattern)

- Positive: also an OWASP-acceptable pattern; a second cookie carries the token and the frontend reads it via JS
  and echoes it as a header.
- Negative: rejected because the synchronizer token is the OWASP "strongest" recommendation, and Reverie already has
  a server-side session store (`tower-sessions`), so the cost of the synchronizer pattern is essentially one more
  session key, whereas double-submit needs a second cookie and JS to read it.

### Offset pagination

- Positive: simpler client code, `?page=N&size=20`.
- Negative: rejected on correctness grounds. Reverie's enrichment pipeline writes asynchronously, so the row count of
  the `manifestations` table shifts mid-scroll; offset pagination would display duplicates and skip rows under that
  workload.

### Bearer-token CSRF defence only

- Positive: treats the API as if it were a public API, requiring `Authorization: Bearer <token>` on every request
  and skipping CSRF entirely.
- Negative: rejected because the browser UI uses cookie sessions for the same reason the existing `/auth/login` and
  `/auth/me` flows do (OIDC-driven login, no per-request token management on the client side). Hybrid stacks need
  browser-CSRF defence for the cookie surface and token-auth for the API client surface separately: Reverie ships
  CSRF for browser cookies, and bearer tokens exist on a separate endpoint set (`/api/tokens` issues device tokens;
  those endpoints sit behind `BasicOnly`/`Bearer` extractors that bypass the cookie session entirely and thus don't
  need CSRF, per `backend/src/auth/middleware.rs`).

### Custom problem-type host (not `reverie.example`)

- Positive: a resolvable host such as `https://reverie.unkos.dev/probs/...` would let a problem-type URI actually
  dereference.
- Negative: deferred rather than rejected outright. RFC 9457 §3.1 explicitly says the URI does not need to
  dereference at first, and nothing today depends on the URI resolving. When a canonical project URL lands, a single
  pass through `error/problems.rs` swaps the prefix, because the URIs are stable in their slugs.

## More information

The RFC 9457 envelope governs every error on the JSON API surface (`/api/v1/*`), including query-parameter
rejections: handlers extract `Result<Query<T>, QueryRejection>` (`axum_extra`) and `?`-propagate, so a malformed
query returns `application/problem+json` (`type` `.../malformed-query`, HTTP 400), never axum's plaintext 400. OPDS
query handlers, out of scope per the content-negotiation convention above, return the same `problem+json` on a
malformed query via their existing `Result<_, AppError>` path, not by this decision.

IETF specs cited: RFC 9457 (Problem Details, formerly RFC 7807), RFC 8288 (Web Linking / Link header), RFC 7396
(JSON Merge Patch), RFC 9110 §12 (content negotiation), RFC 9110 §13.1 (`If-Match`), RFC 3339 (date format), RFC
8259 (JSON). OWASP cheat sheet: Cross-Site Request Forgery Prevention.

Sibling ADR: [backend auxiliary crates](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md) (the
`axum-extra`, `serde_with`, and `subtle` dependency choices this decision relies on).

Sibling ADR: [frontend data layer dependencies](./0010-frontend-data-layer-dependencies-react-query-and-dnd-kit.md)
(the frontend dependency adoptions for the same API conventions work).
