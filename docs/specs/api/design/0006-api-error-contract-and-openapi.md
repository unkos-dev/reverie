---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0006"
title: "API error contract and OpenAPI"
satisfies:
  - "REV-REQ-0014"
  - "REV-REQ-0015"
  - "REV-REQ-0016"
  - "REV-REQ-0017"
governed-by:
  - "REV-ADR-0011"
  - "REV-ADR-0016"
---

# API error contract and OpenAPI

This Design covers how a failure anywhere on Reverie's JSON surface becomes a wire response: the `AppError` enum and its
RFC 9457 `application/problem+json` mapping, the stable problem-type slug registry, the request-path `instance` field
and the task-local that carries it, the `ApiPath`/`ApiJson` extractors that route framework rejections into the same
envelope, the code-first OpenAPI 3.1 document generated from the handlers, the drift gate that keeps the committed
artifact honest, and the client's `ApiError` class and the `apiFetch` logic that builds one from any non-2xx response.

## Purpose and boundaries

This subject owns `AppError` and its `IntoResponse` impl (`backend/src/error/mod.rs`); the problem-type slug registry
(`backend/src/error/problems.rs`); the `instance`-field task-local and its middleware (`backend/src/error/instance.rs`);
the `ApiPath`/`ApiJson` request extractors (`backend/src/extract.rs`) and the plain `Result<Query<T>, QueryRejection>`
pattern every query-accepting handler uses instead of a third wrapper; the `ProblemDetails` DTO that is simultaneously
the runtime response body and the documented OpenAPI component schema, `ApiDoc`, `SecurityAddon`, `pilot_router`,
`router` and `spec_json` (`backend/src/openapi.rs`); the committed `backend/openapi.json` artifact and its drift test
(`backend/tests/gen_openapi.rs`); and, on the client, `ApiError` (`frontend/src/api/errors.ts`) and the Problem Details
parsing path inside `apiFetch` (`frontend/src/api/fetch.ts`).

It does not own the CSRF synchronizer-token check that raises `AppError::CsrfMissing`/`CsrfMismatch`, which belongs to
the CSRF protection subject; the `If-Match` grammar, comparison and precondition contract behind
`AppError::IfMatchRequired`/`IfMatchMismatch`/the header half of `MalformedHeader`, the Design "Conditional requests and
optimistic concurrency"; the security-header middleware, the two-class CSP, or the composite router's fallback dispatch
(including the reserved-prefix `404` and the `405` substitution that both construct an `AppError` directly), the Design
"Response security headers and CSP"; or how a request becomes a `CurrentUser` in the first place, a neighbouring subject
(`backend/src/auth/middleware.rs`). It does not own the scope/role authorisation checks that raise
`AppError::Forbidden`, the Design "Authorization axes"; the row-level-security mechanism behind the
RLS-hidden-row-to-`404` mapping, the Design "Row-level security and database context"; or any one handler's own
business-rule message inside `AppError::Validation`, which belongs to that handler's own Design.

Depends on: axum's and axum-extra's built-in rejection types (`PathRejection`, `JsonRejection`, `QueryRejection`) that
this subject's three `From` implementations and the `ApiPath`/`ApiJson` derives convert; `utoipa` and `utoipa_axum` for
the document generator and the `routes!`/`OpenApiRouter` machinery; `crate::state::AppState`; and the router assembly in
`backend/src/lib.rs`, which mounts `problem_instance_layer` on the composite router between the session layer and
`security_headers` so that every response the composite router or its fallback produces, including a reserved-prefix
typo, passes through the task-local before `AppError::into_response` or `ProblemDetails::into_response` runs.

Depended on by: effectively every handler in `backend/src/routes/`. Of the handler-shaped functions there, all but three
return `Result<impl IntoResponse, AppError>` (or a bare infallible success type): `health::health` (a fixed `200` with
no error path), `health::ready` (`Result<&'static str, ProblemDetails>`, building the DTO directly rather than through
`AppError` — see Structure), and `spa::try_dist_file` (owned by "Response security headers and CSP", not an application
route). It is also depended on by `backend/src/authz_matrix.rs` (Design "Authorization axes"), which parses
`spec_json()` in-process to build its deny-by-default grid, and by most of the backend test suite through
`test_support::assert_problem` (the `tokens`, `enrichment`, `ingestion`, and `suggest` route test modules assert on
their error responses their own way instead). On the client, `ApiError` is depended on throughout the SPA: every page
and mutation hook that branches on `err instanceof ApiError`, the library cell-editing and metadata-dialog surfaces that
layer `isIfMatchMismatch`/`isIfMatchRequired` on top of it (owned by "Conditional requests and optimistic concurrency"),
and the 401 recovery funnel in `frontend/src/lib/query/client.ts` (the Sessions subject).

## Structure

- `backend/src/error/mod.rs` declares `AppError`, a `#[non_exhaustive]` `thiserror::Error` enum, and its `IntoResponse`
  impl. The impl first computes an optional `WWW-Authenticate` challenge for the three variants that carry one
  (`Unauthorized`, `InvalidCredential`, `BasicAuthRequired`), then matches every variant by name — no catch-all arm — to
  a `(StatusCode, slug, title, detail)` tuple, and finally builds one `crate::openapi::ProblemDetails` from that tuple
  and attaches the challenge header if present. Because the match has no wildcard, a variant added without its own arm
  fails to compile inside this module, even though `#[non_exhaustive]` means a downstream crate could not write such a
  match itself. Three `From` implementations route framework rejections into the same type:
  `From<axum_extra::extract::QueryRejection>` (`MalformedQuery`), `From<axum::extract::rejection::PathRejection>`
  (`MalformedPath` for a decode failure, `Internal` for any other rejection kind — a routing or extractor-ordering bug,
  not a caller error), and `From<axum::extract::rejection::JsonRejection>` (`InvalidRequestBody`, matching the rejection
  subclass to a fixed status and a fixed, non-leaking detail sentence rather than axum's own message).
- `backend/src/error/problems.rs` is a flat `const` registry, one per `AppError` variant, plus `PROBLEM_BASE` (the
  placeholder host `https://reverie.example/probs`) and `problem_type(slug)`, which concatenates the two. The slugs are
  the load-bearing, stable part of the resulting URI; the host is a placeholder value.
- `backend/src/error/instance.rs` holds a `tokio::task_local!` `CURRENT_URI: String` and two functions:
  `problem_instance_layer`, a tower middleware that stores `req.uri().path()` into the task-local for the scope of
  `next.run(req)`, and `current_request_uri()`, which reads it back (`None` outside the scope, including any unit test
  that calls `.into_response()` directly).
- `backend/src/extract.rs` declares `ApiPath<T>` and `ApiJson<T>` as one-line `#[derive(FromRequestParts)]` /
  `#[derive(FromRequest)]` wrappers, each `#[from_request(via(...), rejection(AppError))]` over the corresponding axum
  extractor. The derive (from axum-extra) needs only the `From<PathRejection>`/`From<JsonRejection>` implementations
  this subject already supplies; the wrapper types add no logic of their own. Every path-parameter and JSON-body
  extraction under `backend/src/routes/` goes through one of these two wrappers; no handler declares a bare
  `axum::extract::Path` or a bare `axum::Json` as a request extractor. Query-string extraction has no equivalent
  wrapper: every query-accepting handler instead declares `params: Result<Query<T>, QueryRejection>` from
  `axum_extra::extract` and `?`-propagates it, relying on the `From<QueryRejection>` impl directly.
- `backend/src/openapi.rs` is the document generator. `SecurityAddon` (a `utoipa::Modify` impl) registers the four
  security schemes (`session_cookie`, `opds_basic`, `device_token_bearer`, `oidc_jwt_bearer`); `ApiDoc` carries the
  document metadata, the document-level `security(("session_cookie" = []))` default, the component-schema list, and the
  tag descriptions. `ProblemDetails` is the shared runtime-and-schema struct: `AppError::into_response` and
  `health::ready` both construct it directly, and its own `IntoResponse` impl sets the `application/problem+json`
  content type and fills `instance` from `current_request_uri()` when the caller left it `None`. `pilot_router()` seeds
  an `OpenApiRouter` with `ApiDoc::openapi()` and merges the `OpenApiRouter` each documented module exposes (`health`,
  `library`, `suggest`, `series`, `dashboard`, `shelves`, `users`, `settings`, `tokens`, `metadata`, `reading`,
  `enrichment`, `ingestion`, `auth`, `preferences`, and the OPDS cover-download dual mount). `router()` takes the
  axum-router half for `crate::build_router` to mount; `spec_json()` merges the OPDS feed routes' own `OpenApiRouter` on
  top of `pilot_router()` first — documenting them unconditionally even though their runtime mount is gated on the
  `opds.enabled` setting — then serialises the merged document as pretty-printed JSON with a trailing newline.
- Each documented handler carries its own `#[utoipa::path(...)]` annotation naming its path, tag, per-operation
  `security` requirement, and a hand-chosen `responses(...)` list. A handler is wired into `pilot_router()` through the
  `routes!(handler, ...)` macro, which resolves a compiler-generated `__path_<handler>` module the annotation produces;
  a handler passed to `routes!` without the annotation fails to compile; there is no way to register a route through
  this mechanism and leave it undocumented.
- `backend/tests/gen_openapi.rs` is the drift gate: `spec_json()` must render byte-identical to the committed
  `backend/openapi.json` (`REGEN=1 cargo test --test gen_openapi` rewrites it), plus five narrower assertions — every
  `ProblemDetails`-referencing response declares `application/problem+json` as its only content type, the document
  reports `openapi: "3.1.x"` and covers `/health`/`/health/ready`, the four security schemes and the document-level
  deny-by-default are present, and a handful of library, series and dashboard routes and their DTO schemas are
  registered.
- `frontend/src/api/errors.ts` declares `ApiError extends Error` (`status`, `type`, `title`, `detail`, and a
  `problemSlug` computed property that takes the last path segment of `type`) plus two free functions,
  `isIfMatchMismatch`/`isIfMatchRequired`, that test `status` and `problemSlug` together — built entirely on
  `ApiError`'s public surface, with no knowledge of the `If-Match` mechanism they name (owned by "Conditional requests
  and optimistic concurrency").
- `frontend/src/api/fetch.ts` owns `apiFetch`'s Problem Details half: a tolerant `ProblemDetailsSchema` (every field
  optional, parsed with Zod via `safeParse`), `peekProblem` (clones the response, gates on the `Content-Type` header,
  parses), `problemFromResponse`/`problemToApiError` (assemble the `ApiError`, falling back to `response.statusText` and
  empty strings for fields a non-conforming body omits), and `decodeSuccess` (the shared 2xx/error decoder used by both
  the first attempt and the CSRF-retry attempt, described by "CSRF protection"). CSRF token injection and retry, and
  `ETag` capture/replay, live in the same file but are owned by their own Designs; this Design covers only the
  request/response mechanics every code path shares regardless of which of those two other concerns fired.

## Interfaces and dependencies

- The wire contract: every error response is `application/problem+json` with `type` (`{PROBLEM_BASE}/{slug}`), `title`,
  `status`, `detail`, and an optional `instance`. `type` and `title` are stable per problem class; `detail` and
  `instance` vary per occurrence. Nullable/omitted-field conventions for success bodies follow the same wire conventions
  but are not this subject's concern beyond `instance`, which is the one field this subject computes rather than a
  handler.
- `crate::openapi::ProblemDetails` is the single point of coupling between the error module and the OpenAPI module: the
  module doc on `backend/src/openapi.rs` names this "intentional... the resulting openapi↔error circularity is the point
  — one struct owns the RFC 9457 wire shape, so the spec and the bytes on the wire cannot drift." Splitting the struct
  into a documentation-only copy and a runtime-only copy would reopen exactly the drift this design avoids.
- `backend/openapi.json` is consumed downstream by the docs site's generated API reference (linked in More information)
  and, in-process rather than from the file, by `authz_matrix.rs`'s completeness sweep — both read the same generated
  shape, never a hand-maintained one.
- Per-operation `responses(...)` documentation is authored by hand at each `#[utoipa::path]` site and is not derived
  from `AppError`'s full variant set: `POST /api/v1/tokens`, for example, documents `401`/`403`/`422` but not the
  `400`/`413`/`415` statuses its own `ApiJson<CreateTokenRequest>` extractor could also raise for a malformed request
  body. The drift gate checks that whatever `ProblemDetails` responses a handler does declare use
  `application/problem+json` and nothing else as their content type; it does not check that every status an endpoint can
  actually return is declared.
- On the client, `apiFetch` (`frontend/src/api/fetch.ts`) is the sole interface between the SPA and `fetch()`, and
  `ApiError` is the sole typed error surface every caller under `frontend/src/` branches on.

## Data and state

- `CURRENT_URI` is the only shared mutable state this subject owns: one `tokio::task_local!` slot, written once per
  request by `problem_instance_layer` before `next.run()`, read at most once per response by
  `ProblemDetails::into_response` when the caller left `instance: None`. It has exactly one writer and its scope ends
  when the middleware's future resolves, so it never crosses requests on a shared connection or leaks into a background
  task spawned off a request.
- The `problems.rs` constants and `SecurityAddon`'s scheme registrations are compile-time data; changing either is a
  code change, not a runtime configuration.
- `backend/openapi.json` is the one artifact this subject persists to disk. It is regenerated by
  `REGEN=1 cargo test --test gen_openapi` (or the repository's `just rust::regen` aggregate) and is otherwise read-only
  at runtime — nothing in the request path re-reads or re-derives it; `spec_json()` always renders fresh from the
  handlers.
- `openapi.rs::API_VERSION` is a fixed `"0.1.0"` constant surfaced as the document's `info.version`, independent of the
  crate's own release-managed version, so a release does not by itself force a spec regeneration.
- On the client, `ApiError` instances are ephemeral: each is constructed and thrown per failed request and carries no
  state beyond its four fields. `apiFetch` itself holds no module-level state; the CSRF token cache and the `ETag` map
  live in their own modules, owned by their own Designs.

## Runtime behaviour

**A malformed path parameter**, for example `GET /api/v1/works/not-a-uuid`:

1. Routing matches `/api/v1/works/{id}` and axum begins extracting `ApiPath<Uuid>` before the handler body runs.
2. The inner `axum::extract::Path<Uuid>` extraction fails to parse `"not-a-uuid"` as a `Uuid`, producing a
   `PathRejection::FailedToDeserializePathParams` whose `status()` is `400`.
3. The `ApiPath` derive converts that rejection through `AppError`'s `From<PathRejection>` impl, which matches the `400`
   case to `AppError::MalformedPath("a path parameter has the wrong shape")` — a fixed sentence, never the rejection's
   own text, which can embed decoder internals.
4. `AppError::into_response` builds a `ProblemDetails` with `type` ending in `/malformed-path`, `status: 400`,
   `title: "Malformed Path Parameter"`.
5. Because `problem_instance_layer` wraps the whole composite router, the task-local already holds
   `/api/v1/works/not-a-uuid` when this response's body is serialised, so `instance` carries that path even though the
   handler itself never ran.

**A request whose existence must not leak**, `GET /api/v1/works/{id}` for a well-formed but non-existent or fully
RLS-hidden id (`work_detail` in `backend/src/routes/library/mod.rs`):

1. `ApiPath<Uuid>` succeeds, so this time the handler runs. It first fetches every `manifestations` row for the work
   under the caller's `acquire_with_rls` transaction (Design "Row-level security and database context").
2. If that fetch returns zero rows — because the work id does not exist at all, or because every manifestation of a real
   work is hidden from this caller — the handler returns `AppError::NotFound` immediately, before ever querying the
   `works` table itself.
3. A caller therefore cannot distinguish "no such work" from "a work I am not allowed to see" from the response alone:
   both collapse to the same `404` `not-found` Problem Details body, the OWASP existence-not-leaked choice this
   subject's governing ADR records.

**A JSON body the extractor rejects**, `POST /api/v1/tokens` with a body that is not valid JSON:

1. `ApiJson<CreateTokenRequest>`'s `FromRequest` delegates to `axum::Json`, which fails to parse the bytes and produces
   `JsonRejection::JsonSyntaxError`.
2. `AppError`'s `From<JsonRejection>` impl matches that variant to status `400` and the fixed detail sentence
   `"the request body is not valid JSON"`, wrapped in `AppError::InvalidRequestBody { status, detail }`.
3. The response carries `type` ending in `/invalid-request-body`, `status: 400`. A body that is valid JSON but the wrong
   shape instead hits `JsonRejection::JsonDataError` and answers `422`; a missing or wrong `Content-Type` answers `415`;
   a body over the size limit answers `413` through the same variant's catch-all arm, which preserves axum's own
   `other.status()` but still substitutes the fixed detail sentence "the request body could not be read".

**The `instance` field on an unmatched reserved-prefix path**, `GET /api/v1/__definitely_not_a_route__` (the regression
test `unmatched_api_route_returns_problem_with_instance` in `backend/src/lib.rs`):

1. `problem_instance_layer`, mounted outside the whole composite router (routes, SPA assets, and the fallback together),
   stores the path into the task-local before `next.run()`.
2. No registered route matches, so the request reaches `composite_fallback` (Design "Response security headers and
   CSP"), which recognises `/api` as a reserved prefix and answers with `AppError::NotFound`'s response rather than the
   single-page application's `index.html`.
3. `AppError::into_response` leaves `ProblemDetails.instance` as `None`; that struct's own `IntoResponse` impl fills it
   from `current_request_uri()`, which is still in scope because the fallback ran inside the same middleware stack as an
   ordinary matched route. The body's `instance` reads `/api/v1/__definitely_not_a_route__`.

**A non-2xx response reaching the client**, for example a `412` from a metadata `PATCH` whose `If-Match` is stale:

1. `apiFetch`'s `decodeSuccess` sees `!response.ok` and calls `problemFromResponse`, which calls `peekProblem`.
2. `peekProblem` clones the response (a `Response` body is single-shot), checks the `Content-Type` header includes
   `application/problem+json` or `application/json`, and parses the body through the tolerant `ProblemDetailsSchema`.
3. `problemToApiError` builds `new ApiError(412, type, title, detail)`, falling back to `response.statusText` for
   `title` and `""` for `detail` if any field was absent — the schema tolerates a partial body rather than throwing.
4. The caller sees a thrown `ApiError` with `status === 412` regardless of whether the body was well-formed Problem
   Details, and `isIfMatchMismatch` (owned by "Conditional requests and optimistic concurrency") can then test
   `problemSlug` on it.

**A response whose body is not JSON-shaped at all**, for example a reverse proxy's HTML error page on an apparent
non-2xx: `peekProblem`'s content-type gate fails immediately (`ct` matches neither content type), so it returns `null`
without attempting to parse the body, and `problemToApiError` falls back entirely to `response.statusText` and an empty
`detail`, with `type: null`. The thrown `ApiError` still carries the real HTTP `status`, so status-based branches (the
`401` redirect funnel, the `412` conflict check) keep working; only slug-based branches see `null`.

## Failure and recovery

- Every `AppError` variant serialises as `application/problem+json` with a stable `type` slug drawn from `problems.rs`:
  the exhaustive, wildcard-free match in `AppError::into_response` guarantees this for the enum itself
  (`internal_returns_500_without_leaking_details` and the sibling per-variant tests in `error/mod.rs` pin the shape for
  every variant). The one producer of a `ProblemDetails` body outside `AppError` is `health::ready`'s `503`, which
  deliberately uses RFC 9457 §4.2.1's `about:blank` type with no registered slug, because the semantics are fully
  carried by the status code; this is outside the stated obligation's scope (it names `AppError` variants), not a
  counterexample to it.
- A query, path, or JSON-body decode failure maps to the status its rejection class implies rather than one flattened
  code: `MalformedQuery` and `MalformedPath` are always `400`; `InvalidRequestBody` follows `JsonRejection`'s own
  subclass (`400`/`413`/`415`/`422`), verified per subclass in `error/mod.rs`'s test module.
- The `instance` field reflects the request path exactly when the request passed through `problem_instance_layer` (every
  request the composite router or its fallback handles) and is omitted otherwise (a unit test calling `.into_response()`
  directly, per `instance_omitted_outside_request`).
- Every non-2xx `apiFetch` response surfaces as an `ApiError` carrying the response's real HTTP status, whatever the
  body shape: a conforming Problem Details body, a partial one, or a non-JSON one (the fallback paths above) all still
  produce an `ApiError` whose `.status` matches `response.status`.
- A malformed *2xx* body is a distinct failure this subject also owns: `decodeSuccess`'s `JSON.parse` on a nominally
  successful response can throw a bare `SyntaxError` (a proxy returning an HTML gateway-error page under a `200`, for
  instance); `apiFetch` wraps that into an `ApiError` with `type: null`, `title: "Malformed JSON response"`, and a
  `detail` carrying the request URL, status, and an up-to-80-character body preview, rather than letting the raw
  `SyntaxError` propagate with no request context.
- A `PathRejection` kind other than a plain decode failure (any kind whose `status()` is not `400` — a routing or
  extractor-ordering mistake, not a value the caller sent) maps to `AppError::Internal` and a `500`, never to
  `malformed-path`: `path_rejection_other_kind_maps_to_internal` pins this so a server-side extractor bug is never
  reported back to the caller as if their input were at fault.

## Security and operations

`AppError::Internal` never lets the wrapped error's `Display` text reach the response body: the cause is
`tracing::error!`-logged with full context, and the client-visible `detail` is the fixed string "An internal error
occurred.", independent of what the wrapped `anyhow::Error` says (`internal_returns_500_without_leaking_details`
constructs an error whose message names a database connection string and asserts neither word appears in the body). The
same discipline applies to every other fixed-sentence variant (`InvalidRequestBody`, `MalformedPath`): the caller's own
bytes may be echoed back (`MalformedQuery`'s detail is built from the rejection's `Display`, which is the caller's own
query string plus the failing field name), but no server-side state ever is.

`Unauthorized` and `InvalidCredential` share an identical body (same slug, title, detail) and differ only in the
`WWW-Authenticate` challenge (`Bearer` versus `Bearer error="invalid_token"`), and `InvalidCredential`'s body never says
*why* a presented credential was rejected — expired, forged, or naming an unknown identity are all the same response.
This denies an attacker a working oracle for probing which failure mode applies. `BasicAuthRequired` carries a third,
OPDS-specific challenge (`Basic realm="...", charset="UTF-8"`) alongside the same RFC 9457 body; e-reader clients using
OPDS act on the status and challenge header alone and are documented to ignore the JSON body entirely. Because OPDS
success responses are unversioned Atom XML while its failures still render through this subject's envelope, a decode
failure or an explicit `AppError` on any OPDS route answers `application/problem+json` just like `/api/v1/*` does, an
explicit in-scope carve-out despite the OPDS content type otherwise being out of the JSON API conventions.

`SecurityAddon`'s document-level `security(("session_cookie" = []))` default is a documentation-time fail-safe, not a
runtime gate: an operation wired through `pilot_router` that omits its own per-operation `security` annotation documents
as requiring authentication by default rather than silently documenting as public. Runtime enforcement of that
requirement is the neighbouring auth middleware's job, and the completeness backstop that every real operation actually
declares and enforces a scope is `authz_matrix.rs`'s sweep (Design "Authorization axes"); this subject supplies the
document the sweep parses and nothing more.

This subject has no independent operational surface of its own to run, restart, or scale. Its one operational action is
regenerating `backend/openapi.json` after a doc-comment or handler-surface change, via `just rust::regen` or
`REGEN=1 cargo test --test gen_openapi` directly; the CI-mirrored drift test is the only gate on that artifact going
stale.

## More information

- [API reference](../../../../website/src/content/docs/reference/index.md): the generated-reference landing page
  rendered from `backend/openapi.json`.
- [Backend README](../../../../backend/README.md): orientation, including the error module's place in the directory
  tree.
