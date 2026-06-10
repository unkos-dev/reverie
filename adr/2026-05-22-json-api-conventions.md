---
status: accepted
date: 2026-05-22
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# JSON API conventions for Reverie's browser-facing REST surface

> **Path prefix note:** [`2026-06-08-api-versioning-openapi.md`](2026-06-08-api-versioning-openapi.md)
> renames the `/api/` prefix to `/api/v1/` for all endpoints in this document.
> All paths below should be read as `/api/v1/<path>` (e.g. `/api/books` → `/api/v1/books`).

## Context and Problem Statement

Step 11 of the Reverie blueprint (UNK-80) introduces the first
JSON REST surface a browser will consume: `/api/books`,
`/api/books/{id}`, `/api/works/{id}`, plus search, shelves, series,
manifest metadata, users, and persisted settings across six
sub-phases (11a–11f). Today the only browser-facing JSON paths
are auth, theme, cover, and a handful of one-shot endpoints
(ingestion + enrichment triggers, token issue); the dominant
read surface is OPDS, an Atom XML feed for e-readers. Step 11
adds a parallel JSON surface large enough that ad-hoc per-handler
choices on naming, error envelope, pagination, and CSRF would
fork the codebase against itself.

This ADR fixes the conventions before the first handler lands.
Sub-phases 11a–11f all defer to this ADR for their per-endpoint
shape. A second purpose: every convention captured here is
referenced by the frontend `src/api/` client and must hold across
the wire boundary; deciding it once avoids backend/frontend
contract drift.

The governing principle is
[`feedback_industry_standard_default`](../.claude/projects/-home-coder-reverie/memory/feedback_industry_standard_default.md):
default to IETF / OWASP / W3C standards; any deviation requires
conscious decision plus measurably better outcome plus an ADR.
This document is that ADR for every API-shape convention Step 11
adopts.

## Decision

### Field naming — `snake_case`

JSON fields use `snake_case` (e.g. `cover_url`, `next_cursor`,
`created_at`). Reverie's existing `User` struct in
`backend/src/routes/auth.rs:192-199` already emits `snake_case`
because Rust struct fields are `snake_case` by convention and
serde's default behaviour is round-trip-preserving with no
`#[serde(rename_all)]`. Matching this requires zero per-handler
configuration.

RFC 8259 (the JSON spec) is naming-agnostic; the de-facto
convention across modern public APIs is mixed (Stripe and Slack
use `snake_case`; GitHub mixes `snake_case` body fields with
`camelCase` GraphQL). Reverie picks `snake_case` for one reason
that overrides convention preference: it removes a per-struct
serde attribute that would otherwise be the load-bearing line in
every handler. The cost is one `eslint-plugin-camelcase` carve-out
on the frontend's API client surface.

### Date format — RFC 3339

Timestamps serialise as RFC 3339 strings (e.g.
`"2026-05-22T13:42:00.123Z"`). Backend uses the `time` crate
([`project_time_not_chrono`](../.claude/projects/-home-coder-reverie/memory/project_time_not_chrono.md));
`time::OffsetDateTime`'s default serde format is RFC 3339. Matches
the existing OPDS Atom feed (Atom's `<updated>` and `<published>`
use the same shape per RFC 4287 §3.3) so the two surfaces stay
consistent for any operator who reads both.

### Error envelope — RFC 7807 `application/problem+json` (CHANGED)

Errors emit `application/problem+json` per RFC 7807 with the
following body shape:

```json
{
  "type": "https://reverie.example/probs/<problem-slug>",
  "title": "<short human-readable summary>",
  "status": 422,
  "detail": "<longer explanation, may include instance-specific info>",
  "instance": "/api/books/abc-123"
}
```

- `type` is a stable URI per error variant
  (`not-found`, `unauthorized`, `forbidden`, `validation`,
  `csrf-missing`, `csrf-mismatch`, `if-match-required`,
  `if-match-mismatch`, `system-shelf-immutable`, `internal`).
  Per RFC 7807 §3.1 the URI
  identifies the problem type and does not need to dereference at
  first; we register concrete URIs as the deployment story
  matures. `reverie.example` is a placeholder host until that
  decision lands.
- `title` and `status` mirror the HTTP status reason phrase and
  numeric code.
- `detail` is the caller-visible message — what was previously
  the singular `"error"` field.
- `instance` is the request path (RFC 7807 §3.1 makes this
  optional but recommended; we always include it for debuggability).
- Content-Type is `application/problem+json`, not
  `application/json`. This signals to RFC 7807-aware clients that
  the body is a Problem Details document and not a domain object
  with an `error` field — important for `fetch().then(res =>
res.json())` flows that branch on shape.

This is a **CHANGE** from the inherited shape
`{"error": "<msg>"}` that today exists across
`backend/src/error.rs` and every handler emitting bespoke error
bodies. Migration is Task 1b of Sub-phase 11a: rewrite
`AppError::IntoResponse` to centralise the new envelope; audit
`rg '"error":' backend/src` to find any handler bypassing it.

Rationale for the migration: RFC 7807 is the only IETF-blessed
JSON error envelope. Problem-type URIs allow the frontend to
discriminate errors by stable identifier instead of brittle
status-code + string-prefix sniffing. The current
`{"error": "<msg>"}` shape carries the same information but is
non-standard, costs Reverie a roundtrip every time a non-Reverie
client integrates, and makes deviations (e.g. validation errors
that need structured `field`/`message` pairs) carry the same
top-level shape as catastrophic failures.

The migration is well-scoped: `AppError` already centralises the
mapping. Test churn is limited to assertions on `body["error"]`
which a `test_support::assert_problem(response, type_slug,
status)` helper collapses into one line per assertion.

#### `instance` field plumbing

`instance` is the request path. `AppError` is a value, not a
request-coupled construct, so the path is captured by a tiny
`problem_instance_layer` tower middleware in
`backend/src/error/instance.rs` that stores the request path into
a `tokio::task_local!` slot on request entry. `AppError::into_response`
reads from that task-local. The middleware mounts on the outermost
composite router inside `build_router_with_session_store` — wrapping
matched API routes AND the composite fallback — so that
reserved-prefix typos (`/api/__nope__`, `/auth/__nope__`) emitted
by `composite_fallback` carry the `instance` field too. The slot is
`None` outside an HTTP request (e.g. unit tests calling
`AppError::Validation(...).into_response()` directly), in which
case `instance` is omitted from the body — RFC 7807 §3.1 permits
omission.

### Null shape — `Option<T>::None` → JSON `null` (NEVER `skip_serializing_if`)

Nullable fields serialise as `null`, never omitted. TypeScript
consumers read `field: T | null` (always present, sometimes null)
not `field?: T` (sometimes absent, sometimes the value). The
distinction matters: `field?: T` collapses "field absent" and
"field present with `undefined`" into the same shape, which
breaks JSON Merge Patch (RFC 7396) semantics in 11c where
`{"field": null}` means "clear" and `{}` means "leave
unchanged". The existing `User` struct in `backend/src/models/`
already follows this pattern (no `skip_serializing_if` anywhere).

### Pagination model — opaque base64url cursor

`/api/books` and every list endpoint paginates with cursors, not
offsets. Cursors are opaque base64url-encoded payloads carrying
the sort key + tiebreaker (e.g. `(created_at, id)` for
`sort=recent`). Sub-phase 11a Task 4 introduces a tagged
`CursorKey` enum (variant tag `r`/`t`/`a` for `Recent` / `Title`
/ `Author`); the OPDS path keeps using only `Recent` and is
forward-compatible via the tag byte.

Pagination model is not IETF-specified; modern consensus across
GitHub, Stripe, Slack, and Twitter v2 is cursor-based for one
reason: cursors are stable under concurrent inserts. Reverie's
enrichment pipeline writes asynchronously to `manifestations`,
so offset pagination would shift the page boundary mid-scroll.
Cursors are also O(log N) per page at scale; offsets degrade as
the table grows, and the blueprint targets 50K+ library sizes.

### Pagination signaling — RFC 8288 `Link` header + body `next_cursor`

Every paginated response includes:

1. An RFC 8288 `Link` header with `rel="next"` (and `rel="prev"`,
   `rel="first"` when applicable):

   ```http
   Link: </api/books?cursor=eyJ0eXAi...>; rel="next"
   ```

2. A `next_cursor: string | null` field in the JSON body.

The Link header is the IETF-canonical pagination signal: it
matches OPDS Atom's `<link rel="next">` and is the shape GitHub,
Stripe, and the JSON-API spec all converge on. We add the body
field because `fetch()` does not auto-parse Link headers and the
frontend's react-query infinite-query helper consumes a body
field with less ceremony than a parsed Link header. The two are
guaranteed to carry the same information; either is sufficient.

### CSRF defense — OWASP synchronizer token pattern (CHANGED)

`SameSite=Lax` cookies alone are insufficient per the OWASP CSRF
prevention cheat sheet (cited as P0 reading in the Sub-phase 11a
plan). Reverie adopts the OWASP synchronizer-token pattern:

- On session creation in `routes/auth.rs::callback`, generate a
  32-byte random token (`rand::fill(&mut bytes)` per
  `auth/token.rs:50-56`), base64url-encoded, stored under
  `csrf_token` in the session.
- The token is surfaced to the browser via the existing
  `GET /auth/me` JSON response (already called on mount in the
  frontend's `ThemeProvider`). A new `csrf_token: String` field
  joins the body.
- A tower middleware layer `csrf_required` mounts on every
  non-safe-verb (POST/PUT/PATCH/DELETE) request under `/api/*`.
  The middleware reads `X-CSRF-Token` from request headers; if
  absent → 428 with `type: ".../csrf-missing"`; if present but
  does not match the session value under constant-time compare
  (`subtle::ConstantTimeEq`) → 403 with `type: ".../csrf-mismatch"`.
- Token rotates on privilege change (when `session_version`
  increments).
- `POST /auth/logout` is exempt — logging out destroys the
  session and therefore the token; a logged-out user has no
  session to attach a token to.

This is a **CHANGE** from the implicit-only defenses Reverie
relied on previously (`SameSite=Lax` + CSP API layer + Bearer
token requirement for older API surfaces). Both prior defenses
remain in place as belt-and-braces; the synchronizer token is
the new primary CSRF defense for browser cookie-authed
operations.

Rationale for the migration: the OWASP cheat sheet is explicit
that `SameSite=Lax` is necessary but not sufficient (it does not
block top-level GET CSRF that returns sensitive state, and it
does not block CSRF when a cookie is set with `SameSite=None`
for any reason during the session). The synchronizer token is
the OWASP-blessed primary defense. Adopting it during the
greenfield phase (before any production browser-cookie-authed
mutation traffic exists) is cheaper than retrofitting later.

#### Order-of-operations note

The middleware turns on **after** the frontend reads the token.
Specifically: Phase 1 of Task 1c ships token issuance plus the
`csrf_token` field on `/auth/me` plus the frontend reader; Phase
2 enables the middleware. Reversing this order would break
existing cookie-authed mutating endpoints (`POST /api/enrichment/*`,
`POST /api/tokens`) in dev between merge of token-gen and merge
of frontend reader.

### Existence-not-leaked — 404 (not 403) when RLS hides a row

When a request targets a resource the user lacks RLS visibility
on (`GET /api/books/{id}` where the manifestation row is filtered
out), the handler returns 404 Not Found, not 403 Forbidden. OWASP
defense-in-depth: 403 confirms the resource exists, which is
information disclosure. Backend implementation: under
`acquire_with_rls`, the row is invisible to the query, so the
existing zero-rows → `AppError::NotFound` mapping produces the
correct shape with no special handling.

Edge case in 11a Task 7 (`GET /api/works/{id}`): the `works`
table has no RLS, so the handler must explicitly gate on whether
the user can see ≥1 `manifestation` for the work, returning 404
when zero are visible. Tested explicitly for child accounts.

### Mutating-verb body shape — RFC 7396 (JSON Merge Patch)

`PATCH` endpoints accept RFC 7396 JSON Merge Patch bodies. A
missing key means "leave unchanged"; an explicit `null` means
"clear". Server-side decoding uses `serde_with::rust::double_option`
for sparse-update plumbing:

```rust
#[serde(default, with = "::serde_with::rust::double_option")]
pub field: Option<Option<T>>, // None = absent, Some(None) = null/clear, Some(Some(v)) = set
```

11c is the first sub-phase to ship a PATCH endpoint
(`PATCH /api/books/{id}/metadata`); the convention defaults
forward for any future PATCH surface.

JSON Merge Patch is the standard sparse-update format; library
support exists across every client language we are likely to
care about. RFC 7396 is explicitly limited to merging object
trees (no array merging), which is what we need.

### HTTP precondition — RFC 9110 §13.1 `If-Match` / 412 / 428

Optimistic-concurrency endpoints (shelf reorder in 11d, future
metadata writes that need ETag protection) require an `If-Match`
header. Absent → 428 Precondition Required with
`type: ".../if-match-required"`. Mismatch → 412 Precondition
Failed with `type: ".../if-match-mismatch"`. The ETag value is
computed by the handler (typically a hash of the resource state
or its `updated_at`).

11a does not ship a precondition-protected endpoint; the
convention is fixed here so 11d's shelf-reorder can implement it
without re-litigating.

### Content negotiation — RFC 9110 §12

`Accept: application/json` is the default for the API surface.
Errors emit `application/problem+json`. OPDS routes remain on
`application/atom+xml` (unchanged — OPDS is out of scope for
Step 11). No `Accept` header parsing yet; we default to JSON
unconditionally on `/api/*` paths. If a future client needs
content negotiation, the handler picks it up at that point.

## Consequences

- **Good** — every convention is an IETF or OWASP standard. New
  contributors and external integrators find every shape decision
  already grounded in a public spec; reviewer ceremony around
  "why this shape" collapses.
- **Good** — frontend `src/api/` client and backend handlers
  share one shape definition: snake_case, RFC 7807 body, RFC 8288
  pagination, RFC 7396 patches. Cross-cutting drift between
  backend and frontend types is structurally bounded.
- **Good** — the migration from `{"error": "<msg>"}` to RFC 7807
  is the only inherited-divergence-to-standard move; it is
  surgical (single `IntoResponse` impl + a `test_support` helper).
- **Good** — adopting the synchronizer token during the
  greenfield phase is cheap. Retrofitting after production
  cookie-authed mutation traffic existed would be a months-long
  rollout.
- **Bad** — RFC 7807 migration breaks every existing test that
  asserts `body["error"]`. The `assert_problem` helper collapses
  the diff but the PR still touches several existing test files
  (auth, ingestion, enrichment, metadata, tokens). Sub-phase 11a's
  test plan accounts for this.
- **Bad** — `next_cursor` plus the Link header duplicate the same
  signal. Worth the redundancy for the JS-client ergonomics win;
  documented as deliberate.
- **Bad** — `instance` field requires a tokio task-local + a
  tower middleware. ~30 LOC including tests; acceptable cost for
  RFC 7807 conformance and request-path debuggability.
- **Neutral** — `csrf_token` adds one field to the `/auth/me`
  response (12-line shape diff in `backend/src/models/user.rs`).
  Frontend consumes it via a module-level setter; no API surface
  change beyond the field.

## Alternatives Considered

### Keep `{"error": "<msg>"}` error envelope; defer RFC 7807

Cheaper: zero test churn. Frontend would parse a `{status,
error}` pair into `ApiError`.

Rejected on standards-default grounds. The frontend has to grow
typed error handling either way (problem-slug discrimination
matters for CSRF rotation retry, role-changed retry, etc.).
Building that on top of a non-standard envelope means a future
adopter coming from any other RFC 7807-aware stack has to learn
Reverie's shape. Migrating later means re-versioning every
endpoint that uses the old shape. Cheaper to do it once, now.

### Cookie + `SameSite=Lax` only (no synchronizer token)

Cheapest CSRF story. OWASP guidance is "necessary but not
sufficient"; in practice many small apps stop here.

Rejected because Reverie's threat model is the
"multi-user exposed instance"
per
[`project_open_source_security_stance`](../.claude/projects/-home-coder-reverie/memory/project_open_source_security_stance.md).
External contributors and self-hosters audit Reverie expecting
OWASP-default defenses. Synchronizer token is the OWASP-blessed
primary; not adopting it is a deviation that would require its
own ADR. We adopt the standard.

### Custom CSRF cookie (double-submit pattern)

OWASP also lists double-submit as an acceptable pattern. A second
cookie carries the token; the frontend reads it via JS and echoes
it as a header.

Rejected on the basis that the synchronizer token is the OWASP
"strongest" recommendation, and Reverie already has a
server-side session store (`tower-sessions`) — the cost of
synchronizer is essentially "one more session key", whereas
double-submit needs a second cookie and JS to read it. We pick
the OWASP-strongest option since the cost difference is
negligible.

### Offset pagination

Simpler client code: `?page=N&size=20`.

Rejected on correctness grounds — Reverie's enrichment pipeline
writes asynchronously, so the row count of the `manifestations`
table shifts mid-scroll. Offset pagination would display
duplicates and skip rows under that workload. Cursor is the
correct answer.

### Bearer-token CSRF defense only

Treat the API as if it were a public API: require `Authorization:
Bearer <token>` on every request, skip CSRF entirely.

Rejected — the browser UI uses cookie sessions for the same
reason the existing `/auth/login` and `/auth/me` flows do
(OIDC-driven login, no per-request token management on the
client side). Hybrid stacks need browser-CSRF defense for the
cookie surface AND token-auth for the API client surface. We
ship CSRF for browser cookies; bearer tokens exist on a separate
endpoint set (`/api/tokens` issues device tokens; those endpoints
sit behind `BasicOnly`/`Bearer` extractors that bypass the cookie
session entirely and thus don't need CSRF — verified in
[`backend/src/auth/middleware.rs`](../backend/src/auth/middleware.rs)).

### Custom problem-type host (not `reverie.example`)

Pick `https://reverie.unkos.dev/probs/...` or
`https://reverie.example.com/...`.

Deferred. RFC 7807 §3.1 explicitly says the URI does not need to
dereference at first; we use `reverie.example` as a placeholder
because nothing today depends on the URI resolving. When the OSS
release lands a canonical project URL, a single sed pass through
`error/problems.rs` swaps the prefix; the URIs are stable in
their slugs.

## More Information

- Parent (security stance):
  [`project_open_source_security_stance.md`](../.claude/projects/-home-coder-reverie/memory/project_open_source_security_stance.md)
  — threat model is the multi-user exposed instance.
- Industry-standard default principle:
  [`feedback_industry_standard_default.md`](../.claude/projects/-home-coder-reverie/memory/feedback_industry_standard_default.md).
- IETF specs cited: RFC 7807 (Problem Details),
  RFC 8288 (Web Linking / Link header), RFC 7396 (JSON Merge
  Patch), RFC 9110 §12 (Content negotiation), RFC 9110 §13.1
  (`If-Match`), RFC 3339 (date format), RFC 8259 (JSON).
- OWASP cheat sheet: Cross-Site Request Forgery Prevention.
- Implementation plan ingest:
  `.claude/PRPs/plans/library-ui.plan.md` (Sub-phase 11a Tasks
  1, 1b, 1c).
- Linear: [UNK-80](https://linear.app/unkos/issue/UNK-80).
