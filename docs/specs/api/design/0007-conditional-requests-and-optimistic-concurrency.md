---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0007"
title: "Conditional requests and optimistic concurrency"
satisfies:
  - "REV-REQ-0018"
  - "REV-REQ-0019"
  - "REV-REQ-0020"
governed-by:
  - "REV-ADR-0011"
---

# Conditional requests and optimistic concurrency

This Design covers how Reverie protects a read-modify-write cycle against a lost update: the strong entity-tag grammar
and comparison, the two independent mechanisms that produce a tag (a shared hash-based module and the shelves module's
own timestamp-based one), the `If-Match` precondition contract every protected endpoint applies in the same relative
order, and the client's capture and replay of the tag between a resource's GET/PATCH pair.

## Purpose and boundaries

This subject owns the RFC 9110 §8.8.3 strong entity-tag grammar and §13.1.2 strong comparison as Reverie implements
them; the shared hash-based tag constructor and `If-Match` parser in `backend/src/routes/etag.rs` (`hash_etag`,
`StrongEntityTag`, `parse_if_match`, `if_match_mismatch`); the shelves module's separate, `updated_at`-derived tag
constructor and parser in `backend/src/routes/shelves/mod.rs`; the precondition contract common to every protected
endpoint (missing header returns `428`, a mismatched tag returns `412`, a malformed or policy-refused header is rejected
before either) and the point in each handler where that contract is evaluated relative to the existence check and the
request body's own semantic validation; and the client-side capture and replay in `frontend/src/api/etags.ts` and
`frontend/src/api/fetch.ts`.

It does not own the business logic of any endpoint that uses this contract: what fields
`PATCH /api/v1/books/{id}/reading` may change and its transition-stamp rules (Design "Reading state"), the per-field
apply/journal mechanics `PATCH /api/v1/books/{id}/metadata` runs once its precondition holds (Design "Metadata review
and editing"), or shelf CRUD and its handler-enforced ownership predicate (Design "Shelves"). It does not own the
content of any one endpoint's dedicated hash-input struct beyond the contract those structs must satisfy. It does not
own the RFC 9457 Problem Details envelope the resulting errors render into, or the status-code selection rules that
assign `400`/`412`/`428` to a failure class (Design "API error contract and OpenAPI").

Depends on: RFC 9110 §8.8.3 (`entity-tag` grammar), §13.1.1 (`If-Match`), and §13.1.2 (strong comparison), and RFC 6585
§3 (`428 Precondition Required`), all fixed for Reverie's surface by REV-ADR-0011; `axum::http::HeaderMap` and
`HeaderValue` for header access; each consuming endpoint's own row lock, which is what makes the tag comparison
race-free against a concurrent writer (owned by that endpoint's own Design, not restated here).

Depended on by: `PATCH /api/v1/books/{id}/reading` (Design "Reading state"), `PATCH /api/v1/books/{id}/metadata` (Design
"Metadata review and editing"), and `PUT /api/v1/shelves/{id}/items` (Design "Shelves") on the server side; the metadata
edit dialog and the library table's cell-editing surface (Design "Library table cell editing and undo") on the client
side, both of which read a captured tag through `apiFetch` rather than handling `If-Match` themselves.

## Structure

### Two independent entity-tag mechanisms

Reverie has two independent ways of producing and checking an entity-tag: a shared hash-based module and the shelves
module's own timestamp-based one.

`backend/src/routes/etag.rs` is the shared mechanism, consumed by `backend/src/routes/reading.rs` and
`backend/src/routes/metadata.rs`. `hash_etag<T: Serialize>` serialises `state` to JSON, SHA-256s the bytes, truncates
the digest to 16 bytes, base64url-encodes it without padding, and wraps the result in double quotes. Its doc comment
states the contract every caller must satisfy: `state` must be a dedicated struct with a fixed field order covering at
least every field the paired `PATCH` can modify, never the raw `updated_at` column, so the tag changes exactly when the
covered representation does and never leaks a timestamp. `reading.rs` satisfies this with `ReadingEtagFields`, a private
struct distinct from the wire type `ReadingState` (built to borrow `notes` as `&str` rather than clone it).
`metadata.rs` satisfies it differently: `load_book_metadata` returns the same `BookMetadata` struct that both
`get_book_metadata` serves as the response body and `update_book_metadata` feeds into `hash_etag`, a deliberate choice
recorded on `load_book_metadata`'s own doc comment ("Sharing this assembly means the precondition check, the post-write
`ETag`, and the matched `GET` all hash the identical representation through one code path rather than three
independently maintained ones") rather than an accidental byproduct of unrelated reuse.

`parse_if_match` reads the request's `If-Match` header and returns `Ok(None)` when absent, `Ok(Some(StrongEntityTag))`
for one well-formed strong tag, or `Err(AppError::MalformedHeader)`. It rejects more than one `If-Match` header instance
before reading either value (the list form expressed as repeated field instances). For the single value it then parses,
`parse_strong_entity_tag` trims RFC 9110 §5.6.3 `OWS` (space and horizontal tab only) from the edges, operating on bytes
rather than `&str` because the grammar's `obs-text` range (`%x80`-`%xFF`) is not valid UTF-8; `str::trim`'s Unicode
whitespace handling would both mishandle `obs-text` and strip octets the grammar treats as legitimate content. It
rejects the `*` wildcard before the quoting check, so a wildcard gets its own message rather than "not quoted"; rejects
a `W/` weak-validator prefix; rejects a value with no matching surrounding quotes; and rejects an inner `DQUOTE` (a
comma-separated list's boundary, or garbage) before the general `etagc` grammar check. The resulting `StrongEntityTag`
is the only way past this function, so any value reaching a handler is already known to satisfy the grammar and carry no
weak prefix. Its `matches` method compares the stored quoted wire form against a `HeaderValue`'s bytes with octet
equality, no normalisation or case-folding. `if_match_mismatch` builds the `412` response for a stale tag and inserts
the resource's current `ETag` into it, so a caller can recover in one round trip instead of issuing a follow-up `GET`.

`backend/src/routes/shelves/mod.rs` carries its own private `etag_header` and `parse_if_match`, not this module's. A
shelf's entity-tag is `shelves.updated_at` itself, RFC 3339-formatted with the exact options chrono's `Serialize` impl
uses for the response body (so a client echoing the body's `updated_at` field produces byte-identical `If-Match`
content) and quoted, rather than a hash of any wider representation; this exact match holds because chrono, Reverie's
first-party datetime crate, serialises `DateTime<Utc>` to that same RFC 3339 spelling by default, with no per-field
formatting attribute to keep in step. Its `parse_if_match` strips the quoting and parses the inner bytes as an RFC 3339
timestamp with `chrono::DateTime::parse_from_rfc3339`. Three explicit checks run in sequence: a weak (`W/`) prefix is
rejected first; the value must then strip a matching pair of surrounding double quotes, so the `*` wildcard and any
unquoted or mismatched-quote value are rejected at this second, explicit check before either reaches the parser; only
what survives both checks is handed to the RFC 3339 parser, so a comma-separated list or already-quoted garbage is
rejected at this third stage because it fails to parse, not through a further explicit form check. Unlike the shared
module, this function reads the header with `HeaderMap::get`, which returns only the first field value, so a request
carrying `If-Match` twice is read as if only the first instance were present rather than rejected as a list form. The
comparison itself is `DateTime<Utc>` value equality (`row.ts != if_match`) against the timestamp locked under
`FOR UPDATE`, not an octet comparison of the header's raw bytes. A weak, wildcard, or list-form `If-Match` on this
endpoint is rejected as `AppError::Validation`, HTTP `422`, rather than the shared module's `400`.

### The precondition contract's evaluation order

Three handlers apply this contract: `reorder_shelf_items`, `update_book_metadata`, and `patch_reading` — the complete
set of production call sites of `AppError::IfMatchRequired` and `AppError::IfMatchMismatch`. All three follow the same
relative order, checked directly in each handler's body: `CurrentUser::require_scope` (and `require_not_child` where the
endpoint has one) runs first; parsing the `If-Match` header and rejecting it as absent (`428`) or
malformed/policy-refused (`400` via the shared module, `422` via shelves' own parser — see Failure and recovery) runs
second, before any database work; the request then resolves whether the target manifestation or shelf exists and is
visible to the caller, returning `404` at this point when it does not, after the header has already been accepted as
well-formed. `update_book_metadata` and `reorder_shelf_items` answer that question with the same `SELECT ... FOR UPDATE`
whose locked row the entity-tag comparison then reads; `patch_reading` answers it with a separate, unlocked `SELECT`
against `manifestations`, then locks a `reading_state` row `FOR UPDATE` for the comparison only after an
`INSERT ... ON CONFLICT DO NOTHING` has already seeded that row, so the row this handler locks does not itself answer
the existence question. The entity-tag comparison against the row each handler locks runs next, returning `412` on a
mismatch; and the endpoint's own semantic body validation — an empty patch, an out-of-range rating, a partial shelf
reorder — runs last, only once the precondition has held. Each PATCH handler's own doc comment states the rationale for
placing body validation last: RFC 9110 §13.2.1 places precondition evaluation after the normal request checks and before
the request content is processed, so a caller holding a stale representation learns that first and refetches, rather
than being sent to fix a body a concurrent write may already have made irrelevant.

### The client's capture and replay

`frontend/src/api/etags.ts` holds a module-level `Map<string, string>` keyed by resource identity, with two key
families: `reading:{id}` and `metadata:{id}`, resolved from a request path by `etagKeyForPath`. It is the client
counterpart to exactly the two `hash_etag`-based endpoint families above; the shelf reorder `PUT`, whose `If-Match`
value derives from `shelves.updated_at`, resolves to no key and is never touched by this cache (see below).
`frontend/src/api/fetch.ts`'s `apiFetch` calls `captureEtag` on every response it receives, for every status code: when
the response carries an `ETag` header and its path resolves to a key, `rememberEtag` overwrites that key's value.
`sendRequest`, the single-attempt request builder `apiFetch` calls, auto-echoes a cached tag as `If-Match` only for a
`PATCH` request whose caller did not already set that header, so a caller-supplied header always wins over the cache and
a shelf `PUT` is never eligible regardless of its path.

The shelf reorder's client side is a different pattern by design: `frontend/src/api/shelves.ts`'s `buildEtag` quotes a
caller-supplied `updatedAt` string, and `reorderShelfItems` takes an `ifMatch` parameter the caller must source from its
own cached shelf detail, rather than from a shared cache this module writes. `frontend/src/api/errors.ts` exposes
`isIfMatchMismatch` and `isIfMatchRequired`, both scoped to the shared module's two problem-type slugs
(`if-match-mismatch`, `if-match-required`) and their statuses (`412`, `428`); its doc comment treats a `428` the same as
a `412` for recovery purposes, since the remedy — reload, then retry — is identical, and notes `428` is reachable when
an ETag-priming fetch loses a race against a very fast concurrent commit.

## Interfaces and dependencies

- `hash_etag<T: Serialize>(state: &T) -> Result<HeaderValue, AppError>` and
  `parse_if_match(headers: &HeaderMap) -> Result<Option<StrongEntityTag>, AppError>` build and read a tag;
  `StrongEntityTag::matches(&self, current: &HeaderValue) -> bool` compares one, and
  `if_match_mismatch(current_etag: &HeaderValue) -> Response` builds the `412` response for a stale one — all four in
  `backend/src/routes/etag.rs`, the shared mechanism's public surface.
- `etag_header(updated_at: DateTime<Utc>) -> Result<HeaderValue, AppError>` and a module-private `parse_if_match`
  (`backend/src/routes/shelves/mod.rs`) are the shelves-only equivalents; neither is exported outside that module.
- `PATCH /api/v1/books/{id}/reading`, `PATCH /api/v1/books/{id}/metadata`, and `PUT /api/v1/shelves/{id}/items` are the
  three HTTP operations this contract protects; each declares its `If-Match` requirement and the `400`/`412`/
  `422`/`428` response set in its own `#[utoipa::path]` block, which this Design does not restate (Design "API error
  contract and OpenAPI" owns the generated shape).
- `etagKeyForPath`, `rememberEtag`, `getRememberedEtag` (`frontend/src/api/etags.ts`) and the `captureEtag`/ auto-echo
  logic inside `apiFetch`/`sendRequest` (`frontend/src/api/fetch.ts`) are the client's capture-and-replay surface for
  the two shared-module resource families. `buildEtag` and the `ifMatch` parameter on `reorderShelfItems`
  (`frontend/src/api/shelves.ts`) are the parallel, caller-driven surface for shelves.
  `isIfMatchMismatch`/`isIfMatchRequired` (`frontend/src/api/errors.ts`) are the typed checks callers use to branch on
  the two precondition failures.

## Data and state

No entity-tag is stored. Every server-side tag is derived on demand, inside the request's own transaction, from the
database row the request already locked for its own write:

| Endpoint family | Constructor | Hash/comparison input |
| --------------- | ----------- | --------------------- |
| `GET`/`PATCH .../metadata` | `hash_etag` | `BookMetadata` (also the wire response body) |
| `GET`/`PATCH .../reading` | `hash_etag` | `ReadingEtagFields<'a>` (private, distinct from wire type `ReadingState`) |
| Shelf detail read, create/rename, item mutations | `etag_header` | `shelves.updated_at` directly, not hashed |

`GET /api/v1/shelves` (the shelf list) emits no `ETag` at all: there is no single `updated_at` for a page of shelves,
and the list carries no precondition-protected write of its own.

`ETAG_HASH_BYTES` (16, i.e. 128 bits of the SHA-256 digest) is the only adjustable value in the hashing path;
`etag.rs`'s own comment records the rationale as collision resistance sufficient for concurrency checks between a
handful of concurrent editors, traded against header compactness, not a cryptographic integrity guarantee against a
hostile party (the server always recomputes the tag from its own row rather than trusting a client-supplied hash as
data).

On the client, `etags.ts`'s cache is the only persisted (page-lifetime) state this subject owns. It has exactly one
writer, `captureEtag`, called from `apiFetch` after every response regardless of status, and one reader, the
`PATCH`-only branch of `sendRequest`. It holds no more than two live keys per manifestation id at a time (the `reading:`
and `metadata:` families) and nothing for shelves. Nothing mirrors it to `localStorage` or any other longer-lived store,
so a fresh page load starts with an empty cache; the next `GET` of either resource re-seeds its key.

## Runtime behaviour

**A metadata `PATCH` riding a stale `ETag`:**

1. The edit dialog opens by calling `getBookMetadata`, whose `GET` response carries an `ETag`; `captureEtag` stores it
   under `metadata:{id}`.
2. A second client's `PATCH` commits first, changing the manifestation's editable fields.
3. The dialog submits its own `PATCH`. `sendRequest` finds no caller-set `If-Match`, resolves `metadata:{id}`, and
   echoes the now-stale cached tag as `If-Match`.
4. `update_book_metadata` checks `require_scope`/`require_not_child`, then `parse_if_match` (well-formed, so parsing
   succeeds), then acquires an RLS transaction and row-locks the manifestation and its work — the point at which a
   missing or RLS-hidden manifestation would answer `404`, ahead of the tag comparison.
5. `load_book_metadata` reads the current `BookMetadata` under that same lock; `hash_etag(&current)` recomputes its tag,
   so no concurrent writer can slip a change in between this comparison and the lock. `StrongEntityTag::matches` finds
   the caller's cached tag does not match the fresh one; the handler returns `if_match_mismatch(&current_etag)` — `412`,
   carrying the fresh tag — without ever inspecting the request body.
6. `apiFetch` calls `captureEtag` on this `412` response, overwriting `metadata:{id}` with the fresh tag. The caller's
   own recovery path (checked with `isIfMatchMismatch`) reloads and retries; the retried `PATCH` carries the tag the
   cache just refreshed, with no follow-up `GET` needed.

**A metadata or reading `PATCH` with a malformed `If-Match`, contrasted with the same case on a shelf reorder:**

1. A caller sends `If-Match: *`, a weak tag, or the header twice, to either the metadata or reading `PATCH`.
   `parse_if_match` fails before any database work — the wildcard is checked first (so it reports "the `*` wildcard is
   not accepted" rather than "not quoted"), and a repeated header instance is rejected before either value is read. The
   handler propagates the `Err` via `?`, so the request never reaches `acquire_with_rls`: no existence check, no row
   lock, and no semantic body validation runs (a malformed body would itself be rejected by the request's own extractor
   before the handler body executes at all). The response is `400`, problem type `.../malformed-header`.
2. The same three inputs sent to `PUT /api/v1/shelves/{id}/items` meet a different parser and a different result. A weak
   prefix and, separately, the quoting check are rejected explicitly before parsing runs; the wildcard and any unquoted
   value fail that quoting check, while a comma-separated list, correctly quoted, is refused only because it does not
   parse as an RFC 3339 timestamp. All three become `AppError::Validation`, so the response is `422`, not `400`. A
   header sent twice is not rejected at all: `HeaderMap::get` reads only the first instance, so the second value is
   silently ignored rather than triggering the list-form rejection the shared module applies. In every case the
   malformed or ignorable-second header is resolved before the shelf's existence/ownership row lock runs, matching the
   evaluation order described in Structure; the difference is only which parser runs and what status it reports.

## Failure and recovery

- **Missing `If-Match`.** All three protected endpoints return `AppError::IfMatchRequired` (`428`,
  `.../if-match-required`) when the header is absent, checked before any database work. The client treats this the same
  as a `412`: `isIfMatchRequired` exists alongside `isIfMatchMismatch` precisely because the two share a recovery path
  (reload, then retry), and the frontend's own doc names the mechanism by which a genuine caller can reach `428` in
  practice — an ETag-priming fetch losing a race against a very fast concurrent commit that already advanced the
  resource past the tag the priming fetch was about to return.
- **Stale `If-Match`.** `AppError::IfMatchMismatch` (`412`) from `update_book_metadata` or `patch_reading` carries the
  resource's current `ETag` on the response, via `if_match_mismatch`, so `captureEtag` refreshes the client cache from
  the failure response itself and a retry needs no extra round trip. `reorder_shelf_items` returns the same
  `AppError::IfMatchMismatch` variant directly on a mismatch, without attaching an `ETag` header to that response; its
  own `#[utoipa::path]` `412` entry documents no response header, unlike the metadata and reading entries, and the
  frontend module doc for shelves states the caller should refetch the shelf detail to recover rather than expecting the
  error response to carry it.
- **Malformed, weak, wildcard, or list-form `If-Match`.** Rejected as satisfying nothing, on all three endpoints, so a
  caller can never use one of these forms to bypass the freshness check. The status code and problem type differ by
  which parser runs: `400`/`.../malformed-header` from the shared module (`update_book_metadata`, `patch_reading`);
  `422`/`.../validation` from `reorder_shelf_items`'s own parser. A repeated `If-Match` header instance is explicitly
  rejected as a list form by the shared module; the shelves parser has no equivalent check and instead silently reads
  only the first instance, which is not a precondition bypass (the value still has to match the locked row) but is a
  materially different observable response — no error at all for a repeated header, versus `400` from the shared module
  for the same input.
- **`hash_etag` or `etag_header`'s own encoding failure.** Both return `AppError::Internal` (`500`) if serialising the
  hash input or formatting the header value fails. Neither is reachable in ordinary operation: every hash input type is
  a plain, always-serialisable struct, and the base64url/RFC 3339 output these functions produce is always valid
  `HeaderValue` byte content.

## Security and operations

This mechanism is a data-integrity control against a lost update, not an authorisation boundary. On every one of the
three protected endpoints, the scope/role check (`require_scope`, and `require_not_child` where present) and the
row-level-security or ownership check both run independently of the `If-Match` comparison, and both run before it (see
Structure's evaluation-order description) — a caller who lacks read access to a resource is refused with
`401`/`403`/`404` before the entity-tag comparison ever executes, so the `412` response's echoed current tag never
reaches a caller who could not already read the resource through its own `GET`. Rejecting the `*` wildcard, weak
validators, and entity-tag lists is a deliberate policy choice on top of the RFC 9110 grammar: `etag.rs`'s own doc
comment states the wildcard is refused because "accepting the wildcard would let a caller opt out of the freshness check
it exists to enforce" — accepting any of these forms as satisfying `If-Match` would let a caller skip the comparison
this subject exists to perform, not gain unauthorised access. The byte-level, obs-text-aware grammar parsing in the
shared module exists to avoid rejecting a well-formed opaque tag, not as a defence against a crafted header; `http`'s
own header-value construction already refuses the C0 control characters and DEL that the parser tests exercise
defensively for inputs a `HeaderValue` cannot itself represent in production.

The shared module and the shelves module diverge on exactly two points described above (status code for a
malformed/weak/wildcard header, and whether a repeated header instance is rejected). The shared module's `400` conforms
to the status REV-ADR-0011's own status-code-selection rule assigns to a syntactically refused header form; only the
shelves module's `422` diverges from that rule. Neither divergence changes what a caller can accomplish: every malformed
or policy-refused form is still refused as satisfying nothing on all three endpoints, so no caller can turn a malformed
`If-Match` into a successful write it should not have been able to make. Reverie has no operational surface specific to
this subject: it has no service to run, restart, or scale, and no credential of its own.
