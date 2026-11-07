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
  - "REV-ADR-0069"
---

# Conditional requests and optimistic concurrency

This Design covers how Reverie protects a read-modify-write cycle against a lost update: the strong entity-tag grammar
and comparison, the shared hash-based module that produces a tag, the `If-Match` precondition contract every protected
endpoint applies in the same relative order, and the client's ownership of validated values and their matching tag.

## Purpose and boundaries

This subject owns the RFC 9110 §8.8.3 strong entity-tag grammar and §13.1.2 strong comparison as Reverie implements
them; the shared hash-based tag constructor and `If-Match` parser in `backend/src/routes/etag.rs` (`hash_etag`,
`StrongEntityTag`, `parse_if_match`, `if_match_mismatch`); the precondition contract common to every protected endpoint
(missing header returns `428`, a mismatched tag returns `412`, a malformed or policy-refused header is rejected before
either) and the point in each handler where that contract is evaluated relative to the existence check and the request
body's own semantic validation; and the explicit client baselines in `frontend/src/api/books.ts`, `reading.ts` and
`fetch.ts`.

It does not own the business logic of any endpoint that uses this contract: what fields
`PATCH /api/v1/books/{id}/reading` may change and its transition-stamp rules (the Design "Reading state"), the per-field
apply/journal mechanics `PATCH /api/v1/books/{id}/metadata` runs once its precondition holds (the Design "Metadata
review and editing"), or shelf CRUD and its handler-enforced ownership predicate (the Design "Shelves"), whose responses
carry no entity-tag. It does not own the content of any one endpoint's dedicated hash-input struct beyond the contract
those structs must satisfy. It does not own the RFC 9457 Problem Details envelope the resulting errors render into, or
the status-code selection rules that assign `400`/`412`/`428` to a failure class (Design "API error contract and
OpenAPI").

Depends on: RFC 9110 §8.8.3 (`entity-tag` grammar), §13.1.1 (`If-Match`), and §13.1.2 (strong comparison), and RFC 6585
§3 (`428 Precondition Required`), all fixed for Reverie's surface by REV-ADR-0011; `axum::http::HeaderMap` and
`HeaderValue` for header access; each consuming endpoint's own row lock, which is what makes the tag comparison
race-free against a concurrent writer (owned by that endpoint's own subject, not restated here).

Depended on by: `PATCH /api/v1/books/{id}/reading` (the Design "Reading state"), `PATCH /api/v1/books/{id}/metadata`
(the Design "Metadata review and editing"), and `PUT /api/v1/settings` (`backend/src/routes/settings/mod.rs`) on the
server side; the metadata edit dialog and the library table's cell-editing surface (the Library table cell editing and
undo subject) on the client side, both of which read a captured tag through `apiFetch`.

## Structure

### The entity-tag mechanism

Reverie produces entity-tags in one place, a shared hash-based module. Only the endpoints that use it emit a tag or
accept `If-Match`; shelf endpoints do neither.

`backend/src/routes/etag.rs` is the shared mechanism, consumed by `backend/src/routes/reading.rs`,
`backend/src/routes/metadata.rs` and `backend/src/routes/settings/mod.rs`. `hash_etag<T: Serialize>` serialises `state`
to JSON, SHA-256s the bytes, truncates the digest to 16 bytes, base64url-encodes it without padding, and wraps the
result in double quotes. Reading hashes all editable state through the fixed-order `ReadingEtagFields`, including notes
and timestamps. Its all-null default gives an absent row the same validator as matched reading GET.

Metadata hashes `MetadataRevision`, containing the work identity, manifestation identity and both aggregate revisions of
zero or greater. The data-model triggers advance revisions for canonical and provenance-relevant changes, including
changes away and back. `load_book_metadata` returns `MetadataSnapshot`, pairing complete `BookMetadata` values with
those owner revisions. Matched GET assembles that snapshot in a read-only REPEATABLE READ RLS transaction. PATCH
compares the tag under its manifestation and work locks and loads the post-write snapshot before commit. Its response
retains per-field `fields` acknowledgements and adds complete `values`; GET serves the same values type directly.

List rows carry `metadata_etag` and `reading_etag`. Their projected values, revisions, relation batches and complete
reading hash inputs come from one read-only REPEATABLE READ transaction. The list keeps its compact projection and omits
reading notes from the body.

`settings/mod.rs` hashes `SettingsEtagFields`, a private struct holding only the settings row's `revision`, which every
write to the row increments, so the tag changes exactly when any setting does; both `GET` and `PUT /api/v1/settings`
emit it. The `GET` body is a function of that row alone, so one tag never covers two different bodies. Process state
that changes without a revision, the last successful live reload, is served separately by the admin-only
`GET /api/v1/settings/reload-status`, which carries no `ETag`.

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

### The precondition contract's evaluation order

Three handlers apply this contract: `update_book_metadata`, `patch_reading`, and `put_settings`. These are the complete
set of production call sites of `AppError::IfMatchRequired` and `AppError::IfMatchMismatch`. All three follow the same
relative order: `CurrentUser::require_scope` and the endpoint's role check run first; parsing the `If-Match` header and
rejecting it as absent (`428`) or malformed/policy-refused (`400`) runs second, before any database work.

For metadata and reading, the request then resolves whether the target manifestation exists and is visible to the
caller, returning `404` when it does not. `update_book_metadata` uses a `SELECT ... FOR UPDATE` whose locked row the
entity-tag comparison then reads. `patch_reading` uses a separate, unlocked `SELECT` against `manifestations`, then
locks a `reading_state` row `FOR UPDATE` for the comparison after an `INSERT ... ON CONFLICT DO NOTHING` has seeded that
row. The row this handler locks does not itself answer the existence question. `put_settings` has no existence step,
since the settings row is a singleton: after `require_scope`, `require_admin` and the header parse, it opens a
transaction and locks the row `FOR UPDATE` through `services::settings::lock_revision`. The update commits under that
same lock.

The entity-tag comparison against the locked row runs next, returning `412` on a mismatch. Semantic body validation runs
only once the precondition holds: an empty patch, an out-of-range rating, or settings field validation and the
provider-key registry check. RFC 9110 §13.2.1 places precondition evaluation after the normal request checks and before
the request content is processed, so a caller holding a stale representation learns that first and refetches.

### Client editing baselines

`apiFetchWithHeaders` returns a decoded body and response headers without retaining entity-tags. Metadata and reading
adapters validate the complete body and require the matching strong response ETag before publishing one snapshot. List
rows carry `metadata_etag` and `reading_etag` from the same database snapshot as their values.

Table commits capture the displayed row's tag, input and source query key. Dialog drafts retain the snapshot from which
their fields were seeded, including through background refresh and field errors. Every write sends that explicit
baseline. The transport's CSRF retry retains the same body and precondition. A 412 header alone never replaces a draft
baseline or permits another save. Only a matched reset or new editing decision adopts fresh values and their tag.

Settings retain each tag with its validated values. `apiFetchWithHeaders` exposes the decoded body and response headers
without caching settings tags. `getSettings` and `updateSettings` validate the body and require a non-empty response
`ETag` before returning one object containing both. The React Query settings snapshot holds that object; a failed read
or write leaves the previous values and tag together. `useSettingsEditor` sends the starting snapshot's tag explicitly
on a save, and a reconciled retry uses the tag from the validated fresh snapshot. A `412` supplies no replacement
snapshot: the editor reads current settings before comparing concurrent changes or retrying.

Each area retains its base snapshot with its draft from the first edit. Shared cache updates, including a previous
area's pending save, do not replace that base. Field rendering, dirty comparisons and writes use the retained snapshot.
Only the editor's own validated reconciliation read replaces its draft base; clearing the draft releases it so the
pristine editor follows the shared snapshot.

`frontend/src/api/errors.ts` exposes `isIfMatchMismatch` and `isIfMatchRequired`, both scoped to the shared module's two
problem-type slugs (`if-match-mismatch`, `if-match-required`) and their statuses (`412`, `428`); its doc comment treats
a `428` the same as a `412` for recovery purposes: reload a matched representation before a new editing decision.

## Interfaces and dependencies

Exact metadata Undo uses the same strong aggregate precondition as PATCH. It requires `If-Match` before database writes
and compares it under edition and work locks. Selected-field eligibility is an additional decision, independent of
aggregate freshness. Operation resolution supplies current complete metadata and its matching ETag without replaying
writes. It cannot authorise an ordinary save from retained draft values.

- `hash_etag<T: Serialize>(state: &T) -> Result<HeaderValue, AppError>` and
  `parse_if_match(headers: &HeaderMap) -> Result<Option<StrongEntityTag>, AppError>` build and read a tag;
  `StrongEntityTag::matches(&self, current: &HeaderValue) -> bool` compares one, and
  `if_match_mismatch(current_etag: &HeaderValue) -> Response` builds the `412` response for a stale one — all four in
  `backend/src/routes/etag.rs`, the shared mechanism's public surface.
- `PATCH /api/v1/books/{id}/reading`, `PATCH /api/v1/books/{id}/metadata`, and `PUT /api/v1/settings` are the three HTTP
  operations this contract protects; each declares its `If-Match` requirement and the `400`/`412`/`428` response set in
  its own `#[utoipa::path]` block, which this Design does not restate (Design "API error contract and OpenAPI" owns the
  generated shape).
- Metadata and reading adapters expose matched snapshots and require explicit write baselines. `isIfMatchMismatch` and
  `isIfMatchRequired` in `frontend/src/api/errors.ts` identify precondition refusals.

## Data and state

Entity-tags are derived inside the request transaction. Metadata revisions and field stamps live on their owning rows;
tags themselves are not stored. Writes compare under their resource locks, while matched metadata and list reads share
an explicit database snapshot:

| Endpoint family | Constructor | Hash/comparison input |
| --------------- | ----------- | --------------------- |
| `GET`/`PATCH .../metadata` and list rows | `MetadataRevision::etag` | Work and manifestation identities and aggregate revisions |
| `GET`/`PATCH .../reading` | `hash_etag` | `ReadingEtagFields<'a>` (private, distinct from wire type `ReadingState`) |
| `GET`/`PUT /api/v1/settings` | `hash_etag` | `SettingsEtagFields` (the row's `revision` only) |

No shelf endpoint emits an `ETag` or reads an `If-Match` header.

`ETAG_HASH_BYTES` (16, i.e. 128 bits of the SHA-256 digest) is the only adjustable value in the hashing path;
`etag.rs`'s own comment records the rationale as collision resistance sufficient for concurrency checks between a
handful of concurrent editors, traded against header compactness, not a cryptographic integrity guarantee against a
hostile party (the server always recomputes the tag from its own row rather than trusting a client-supplied hash as
data).

Client editing snapshots live in Query and the owning draft. A retained dialog draft owns its initial snapshot until an
explicit matched reset. The Library table-editing owner holds at most ten field operations in memory. Leaving Library or
table editing clears them; filtering, sorting, loading, empty results and the details drawer preserve them.

## Runtime behaviour

A metadata save carries a client request UUID and its editing baseline. A stale aggregate precondition returns 412
before request-reuse checking. After a lost acknowledgement, unchanged dialog input reuses its original request UUID,
patch and baseline. A committed resend therefore receives 412, then resolves the original request before conflict
feedback. Changed input starts a fresh UUID against the retained baseline. The table resolves uncertain saves through
Query and exposes Reload, followed by a new edit. Neither flow automatically replays a save or infers success from equal
values. A saved resolution adopts complete values and the matching ETag; table resolutions retain eligible operations.

Undo sends one operation and its retained aggregate validator. After 412, Query resolves that same operation and retries
once only when the server confirms it eligible and unlocked. The retry uses the matched response's ETag. A restored
resolution confirms an already committed Undo. Lock or unresolved recovery retains the entry; unavailable or conflicting
operations leave the local collection. No refusal executes the next entry.

**A metadata or reading `PATCH` with a malformed `If-Match`:**

A caller sends `If-Match: *`, a weak tag, or the header twice, to either the metadata or reading `PATCH`.
`parse_if_match` fails before any database work — the wildcard is checked first (so it reports "the `*` wildcard is not
accepted" rather than "not quoted"), and a repeated header instance is rejected before either value is read. The handler
propagates the `Err` via `?`, so the request never reaches `acquire_with_rls`: no existence check, no row lock, and no
semantic body validation runs (a malformed body would itself be rejected by the request's own extractor before the
handler body executes at all). The response is `400`, problem type `.../malformed-header`.

## Failure and recovery

- Missing `If-Match` returns 428 before database work. A stale supplied precondition returns 412 with the current tag.
  Ordinary saves retain input and require a matched read before a new editing decision; they never automatically retry.
- Missing or malformed acknowledgement values or tags leave the edit uncertain. Metadata resolution uses committed
  request evidence through Query retry and reconnect. Reading recovery uses a matched read. A failed read retains input.
- Undo refuses locked fields with 409 and retains the entry until its original expiry. A 422 `undo-unavailable` or 409
  `undo-conflict` removes it. Expiry removes entries quietly, without extending their deadlines.

## Security and operations

This mechanism is a data-integrity control against a lost update, not an authorisation boundary. On all three protected
endpoints, the scope/role check (`require_scope`, and `require_not_child` or `require_admin` where present) and the
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

The shared module's `400` conforms to the status REV-ADR-0011's own status-code-selection rule assigns to a
syntactically refused header form. Every malformed or policy-refused form is refused as satisfying nothing on all three
endpoints, so no caller can turn a malformed `If-Match` into a successful write it should not have been able to make.
Reverie has no operational surface specific to this subject: it has no service to run, restart, or scale, and no
credential of its own.
