# Feature: Bounded list queries — cap or paginate all unbounded lists (UNK-374)

## Summary

Five `LIMIT`-less queries violate the ADR-7 "no unbounded queries" contract
(`adr/2026-06-08-keyset-pagination-list-contract.md`). This plan brings each to
compliance: keyset pagination for `GET /api/v1/shelves`, shelf items in
`GET /api/v1/shelves/{id}`, and the two OPDS navigation feeds (authors, series);
a defensive hard `LIMIT` for `GET /api/v1/users` (justified small natural
ceiling — household instance). All patterns mirror the compliant
`GET /api/v1/books` + OPDS acquisition-feed implementations. Frontend API
clients absorb the wire change by walking cursors to assemble full lists, so
page components stay untouched.

## User Story

As an operator of a multi-user exposed Reverie instance
I want every list endpoint bounded by construction
So that no single request can scale its response with library/user data size and exhaust server resources.

## Problem Statement

Five queries return unbounded row sets that grow with user data:

| #   | Endpoint                                          | Handler                                                         | Query (no LIMIT)                                                         |
| --- | ------------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------ |
| 1   | `GET /api/v1/shelves`                             | `list_shelves`, `backend/src/routes/shelves/mod.rs:124`         | `FROM shelves WHERE user_id=$1 ORDER BY is_system DESC, name ASC`        |
| 2   | `GET /api/v1/shelves/{id}` items                  | `get_shelf_with_items`, `shelves/mod.rs:412` (items query :434) | `FROM shelf_items WHERE shelf_id=$1 ORDER BY position ASC, added_at ASC` |
| 3   | `GET /api/v1/users`                               | `list_users`, `backend/src/routes/users/mod.rs:92`              | `FROM users ORDER BY created_at ASC`                                     |
| 4   | `GET /opds/library/authors` (+ shelf-scoped twin) | `emit_authors`, `backend/src/routes/opds/library.rs:475`        | `FROM authors a WHERE EXISTS(…) ORDER BY a.sort_name ASC`                |
| 5   | `GET /opds/library/series` (+ shelf-scoped twin)  | `emit_series`, `opds/library.rs:623`                            | `FROM series s WHERE EXISTS(…) ORDER BY s.sort_name ASC`                 |

ADR-7 names all five as non-compliant. `backend/CLAUDE.md` "No unbounded
queries" invariant forbids them; the code predates the rule.

## Solution Statement

- **Keyset pagination** (the ADR-7 default) for offenders 1, 2, 4, 5 — each
  gains a cursor type following the established encode/parse shape
  (base64url-unpadded over pipe-delimited fields), `LIMIT page_size + 1`
  sentinel fetch, `split_page`-style overflow detection, and a continuation
  affordance (`next_cursor` JSON field + RFC 8288 `Link` header for JSON;
  Atom `rel="next"` link for OPDS).
- **Defensive hard cap** for offender 3 (`list_users`) — ADR-7's justified
  exception: a household instance's user table has a genuinely small natural
  ceiling. Mirrors the `emit_series_books` cap pattern
  (`opds/library.rs:683–710`). No wire-shape change.
- **Frontend**: `listShelves()` and `getShelf()` walk cursors internally and
  return the same fully-assembled shapes — page components unchanged.
- **Indexes**: one migration adds keyset-supporting btree indexes (ADR-7 +
  CLAUDE.md indexing discipline: "Index … keyset sort keys").

## Metadata

| Field            | Value                                                                                                                             |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Type             | BUG_FIX (contract compliance)                                                                                                     |
| Complexity       | MEDIUM-HIGH (5 endpoints, 2 wire-contract changes, OPDS feed semantics, mixed-direction keyset)                                   |
| Systems Affected | routes/shelves, routes/users, routes/opds (library, feed, cursor), routes/cursor, migrations, frontend/src/api, docs/openapi.json |
| Dependencies     | none new — sqlx 0.8 QueryBuilder, base64ct, utoipa (all in tree)                                                                  |
| Estimated Tasks  | 11                                                                                                                                |
| Linear           | UNK-374 (branch `fix/unk-374-bounded-list-queries`)                                                                               |

---

## UX Design

### Before State

```text
Client ──GET /api/v1/shelves──────────► [Shelf, Shelf, …unbounded…]      (array)
Client ──GET /api/v1/shelves/{id}─────► {…, items: […unbounded…]}        (envelope, unbounded items)
Client ──GET /api/v1/users────────────► [User, …unbounded…]              (array)
OPDS   ──GET /opds/library/authors────► <feed> …every author… </feed>    (no rel=next)
OPDS   ──GET /opds/library/series─────► <feed> …every series… </feed>    (no rel=next)

PAIN_POINT: response size scales with data volume; resource-exhaustion surface
            on the multi-user threat model. ADR-7 contract violated.
```

### After State

```text
Client ──GET /api/v1/shelves──────────► {items: [≤N], next_cursor}       + Link: rel="next"
        ──GET …?cursor=X──────────────► {items: [≤N], next_cursor: null}
Client ──GET /api/v1/shelves/{id}─────► {…, items: [≤N], next_cursor}    + ETag (unchanged)
        ──GET …?cursor=X──────────────► {…, items: [≤N], next_cursor: null}
Client ──GET /api/v1/users────────────► [User, … ≤500]                   (hard cap, shape unchanged)
OPDS   ──GET /opds/library/authors────► <feed> ≤N entries + rel="next" </feed>
OPDS   ──GET /opds/library/series─────► <feed> ≤N entries + rel="next" </feed>

VALUE_ADD: every response bounded by construction; N = opds.page_size (default 50).
Frontend api clients walk cursors → page components see identical data shapes.
```

### Interaction Changes

| Location                   | Before           | After                                                | User Impact                                                   |
| -------------------------- | ---------------- | ---------------------------------------------------- | ------------------------------------------------------------- |
| `GET /api/v1/shelves`      | bare `[Shelf]`   | `{items, next_cursor}` envelope                      | **BREAKING wire change** — frontend client updated in same PR |
| `GET /api/v1/shelves/{id}` | `items` complete | `items` paged + `next_cursor` field, `?cursor` param | **BREAKING wire change** — frontend client updated in same PR |
| `GET /api/v1/users`        | unbounded array  | array capped at 500, documented                      | None for realistic instances                                  |
| OPDS authors/series nav    | full feed        | paged feed + `rel="next"`                            | OPDS clients follow standard Atom paging (RFC 5005)           |

Pre-v0.1.0, no API stability promise — breaking wire changes are acceptable
when frontend ships in lockstep (same PR).

---

## Mandatory Reading

| Priority | File                                                        | Lines                              | Why Read This                                                                                            |
| -------- | ----------------------------------------------------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------- |
| P0       | `adr/2026-06-08-keyset-pagination-list-contract.md`         | all                                | The governing contract — tiebreakers, sentinel, exception rules                                          |
| P0       | `backend/src/routes/library/mod.rs`                         | 92–160, 215–290, 388–515           | The compliant JSON pattern: params, sentinel fetch, `split_page`, next_cursor, Link header, utoipa shape |
| P0       | `backend/src/routes/cursor.rs`                              | all                                | JSON cursor encode/parse + error taxonomy to mirror                                                      |
| P0       | `backend/src/routes/opds/cursor.rs`                         | all                                | OPDS cursor module to extend with a name-keyed cursor                                                    |
| P1       | `backend/src/routes/opds/library.rs`                        | 352–360, 452–470, 475–525, 623–710 | OPDS keyset predicate, rel=next emission, both offenders, the `emit_series_books` cap exemplar           |
| P1       | `backend/src/routes/opds/feed.rs`                           | ~355–370                           | `add_next_link` + the `debug_assert_eq!(kind, Acquisition)` to relax                                     |
| P1       | `backend/src/routes/shelves/mod.rs`                         | 1–17, 115–160, 400–467             | Offenders 1+2, module RLS docstring, ETag contract on shelf detail                                       |
| P1       | `backend/src/routes/users/mod.rs`                           | 58–115                             | Offender 3                                                                                               |
| P1       | `backend/src/routes/library/tests.rs`                       | 23–43, 75–101, 494–542             | Test pattern: `server_with_page_size`, fixtures, pagination-walk test                                    |
| P2       | `backend/src/routes/opds/tests.rs`                          | 717–771                            | OPDS rel=next walk test pattern                                                                          |
| P2       | `frontend/src/api/shelves.ts` + `frontend/src/api/users.ts` | all                                | Zod schemas + clients to update                                                                          |
| P2       | `backend/migrations/20260526000000_initial_schema.up.sql`   | 305–311, 512–513, 552–610          | shelf_items DDL/PK + existing index inventory                                                            |

External docs: none required — contract, cursor modules, and OPDS feed
semantics are all first-party. (RFC 5005 `rel="next"` on Atom feeds is the
only external touchpoint; `feed.rs` already implements link emission.)

---

## Patterns to Mirror

**SENTINEL FETCH + SPLIT** (`backend/src/routes/library/mod.rs:215–216, 506–515`):

```rust
qb.push(" LIMIT ");
qb.push_bind(page_size + 1);
// …
fn split_page(rows: &[sqlx::postgres::PgRow], page_size: i64) -> (&[sqlx::postgres::PgRow], bool) {
    let page_size_usize = usize::try_from(page_size).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_size_usize;
    let page_rows = if has_more { &rows[..page_size_usize] } else { rows };
    (page_rows, has_more)
}
```

**NEXT_CURSOR + LINK HEADER** (`library/mod.rs:267–287`):

```rust
let next_cursor = match page_rows.last() {
    Some(last) if has_more => { /* encode, map err → AppError::Internal w/ tracing::warn! */ }
    _ => None,
};
let mut headers = HeaderMap::new();
if let Some(ref nc) = next_cursor {
    let next_url = build_next_url(&uri, nc);
    if let Ok(value) = HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
        headers.insert(LINK, value);
    }
}
```

**CURSOR ENCODE/PARSE** (`backend/src/routes/opds/cursor.rs:67–93`): base64url-unpadded
over `<field>|<field>` payload; `CursorError` enum variants per failure mode;
malformed cursor → `AppError::Validation` → 422.

**MIXED-DIRECTION KEYSET PREDICATE** — single tuple `<`/`>` only works when all
sort directions match. Books' Author sort shows the OR-expansion shape
(`library/mod.rs:388–485`). For `(is_system DESC, name ASC, id ASC)`:

```sql
AND (is_system < $1
     OR (is_system = $1 AND (name, id) > ($2, $3)))
```

**JUSTIFIED-CAP COMMENT** (`opds/library.rs:685–688`) — the cap must carry its
justification inline:

```rust
// Cap at 10× the configured page size — a series of this size is
// pathological, and a single oversized feed beats silently dropping …
let series_limit = i64::from(state.config.opds.page_size) * 10;
```

**OPDS REL=NEXT** (`opds/library.rs:452–470`): encode cursor from last row,
`fb.add_next_link(&format!("{self_path}?cursor={next_cursor}"))`.

**PAGINATION WALK TEST** (`library/tests.rs:494–542`): page_size=2 via
`server_with_page_size`, seed 3 rows, assert page 1 has Link + next_cursor,
page 2 has neither.

---

## Files to Change

| File                                                                              | Action | Justification                                                                                                       |
| --------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------- |
| `backend/migrations/2026MMDDHHMMSS_keyset_list_indexes.up.sql` (+`.down.sql`)     | CREATE | btree indexes for new keyset scans                                                                                  |
| `backend/src/routes/cursor.rs`                                                    | UPDATE | add `ShelfCursor` + `ShelfItemCursor` (encode/parse, same error taxonomy)                                           |
| `backend/src/routes/shelves/mod.rs`                                               | UPDATE | paginate `list_shelves` + shelf-items query; new envelope; utoipa                                                   |
| `backend/src/models/shelf.rs`                                                     | UPDATE | `ShelfWithItems` gains `next_cursor`; new `ShelfListResponse` lives in routes (mirror `BookListResponse` placement) |
| `backend/src/routes/users/mod.rs`                                                 | UPDATE | hard cap + doc note                                                                                                 |
| `backend/src/routes/opds/cursor.rs`                                               | UPDATE | add name-keyed cursor (`NameCursor { sort_name, id }`)                                                              |
| `backend/src/routes/opds/feed.rs`                                                 | UPDATE | allow `add_next_link` on Navigation feeds (relax debug_assert)                                                      |
| `backend/src/routes/opds/library.rs`                                              | UPDATE | keyset + LIMIT in `emit_authors`/`emit_series`; stop discarding `_cursor`                                           |
| `backend/src/routes/shelves/tests…` / `users` / `opds/tests.rs` / `library` tests | UPDATE | pagination-walk + cap tests per endpoint                                                                            |
| `frontend/src/api/shelves.ts`                                                     | UPDATE | envelope schemas; cursor-walk in `listShelves`/`getShelf`                                                           |
| `docs/openapi.json`                                                               | REGEN  | response-shape changes (`REGEN=1 cargo test --test gen_openapi`)                                                    |
| `backend/.sqlx/`                                                                  | REGEN  | query text changes (`cargo sqlx prepare -- --tests` — CI-first caveat below)                                        |

## NOT Building (Scope Limits)

- **No total-count fields** — ADR-7: separate approximate query if ever needed; not requested.
- **No offset pagination** — forbidden by contract.
- **No `/api/v1/users` keyset envelope** — cap is the chosen exception; revisit only if multi-tenant ever becomes a target.
- **No frontend infinite-scroll/paged UI** — clients walk cursors to assemble full lists; UI pagination is a product decision for later.
- **No touch to already-compliant lists** — `/api/v1/books`, OPDS acquisition feeds, `emit_series_books` cap.
- **No new config knob** — all paginated endpoints keep sharing `opds.page_size` (status quo for `/api/v1/books`; introducing a second knob is a separate decision).
- **No RLS changes** — shelves/users handlers stay on grant-gated pool + explicit ownership predicates per module docstring (`shelves/mod.rs:1–17`).

---

## Step-by-Step Tasks

TDD per repo rule: each endpoint task writes its failing pagination test first,
then implements. DB-backed tests are CI-first — locally validate with
`cargo fmt --check`, `clippy`, `SQLX_OFFLINE=true cargo check --tests`, drift
tests; push for the full `#[sqlx::test]` suite.

### Task 1: CREATE migration `keyset_list_indexes`

- **ACTION**: new sqlx migration (up/down) adding:
  - `idx_shelves_user_keyset ON shelves (user_id, is_system DESC, name ASC, id ASC)`
  - `idx_shelf_items_shelf_keyset ON shelf_items (shelf_id, position ASC, added_at ASC, manifestation_id ASC)`
  - `idx_authors_sort_name_id ON authors (sort_name ASC, id ASC)`
  - `idx_series_sort_name_id ON series (sort_name ASC, id ASC)`
  - (none for `users` — capped endpoint, `ORDER BY created_at` on a ≤500-row table needs no index)
- **MIRROR**: existing index DDL style in `migrations/20260526000000_initial_schema.up.sql:552+`
- **GOTCHA**: review with `database-reviewer` agent before commit (CLAUDE.md indexing discipline); shelves/shelf_items are write-light, index cost acceptable
- **VALIDATE**: migration applies in CI (`#[sqlx::test]` runs every migration); `cargo sqlx prepare` against compose cluster if reachable

### Task 2: UPDATE `backend/src/routes/cursor.rs` — JSON cursors for shelves

- **ACTION**: add `ShelfCursor { is_system: bool, name: String, id: Uuid }` and `ShelfItemCursor { position: i32, added_at: OffsetDateTime, manifestation_id: Uuid }` with `encode()/parse()` + unit tests (round-trip, malformed, each error variant)
- **MIRROR**: `opds/cursor.rs:20–93` struct shape; reuse this module's `CursorError` taxonomy and base64ct calls. Tag-prefix payloads (`sh|…`, `si|…`) so cursors are not cross-endpoint replayable
- **GOTCHA**: `name` is user-controlled text and may contain `|` — escape or length-prefix the name field (books' Title cursor handles this; copy its approach exactly — check `cursor.rs:155–198` for the delimiter strategy before inventing one)
- **TESTS FIRST**: round-trip + malformed-input unit tests in same file
- **VALIDATE**: `SQLX_OFFLINE=true cargo test -p reverie-api cursor` (pure unit, no DB)

### Task 3: UPDATE `list_shelves` — keyset pagination

- **ACTION**: add `Query<ShelfListParams { cursor: Option<String> }>`, envelope `ShelfListResponse { items: Vec<Shelf>, next_cursor: Option<String> }`, sentinel fetch `LIMIT page_size + 1`, mixed-direction predicate `(is_system < $c) OR (is_system = $c AND (name, id) > ($n, $i))`, Link header, utoipa update (`body = ShelfListResponse`, 422 response for bad cursor, Link header doc)
- **MIRROR**: `library/mod.rs:148–290` end-to-end; `page_size` from `state.config.opds.page_size`
- **GOTCHA**: current query is `sqlx::query!` macro; keyset predicate needs `QueryBuilder` (dynamic) — mirror books' QueryBuilder usage; item_count subquery stays per-row (bounded by page_size, not N+1-by-loop)
- **TESTS FIRST**: pagination-walk test (page_size=2 via `server_with_page_size` analog, seed 3 shelves with names forcing order, system shelf boundary case: walk must cross is_system DESC → name ASC transition without dropping/duplicating), malformed-cursor → 422
- **VALIDATE**: `SQLX_OFFLINE=true cargo check --tests`; full test in CI

### Task 4: UPDATE `get_shelf_with_items` — paginate items

- **ACTION**: add `?cursor` query param; items query gains keyset predicate `(position, added_at, manifestation_id) > ($p, $a, $m)` (all-ASC → single tuple compare) + `LIMIT page_size + 1`; `ShelfWithItems` gains `next_cursor: Option<String>`; utoipa update
- **MIRROR**: Task 3 + `models/shelf.rs:63–79`
- **GOTCHA**: ETag/If-Match contract (`shelves/mod.rs:400–411`) is on shelf `updated_at` — unchanged by item paging; keep header emission identical. PK `(shelf_id, manifestation_id)` is the only unique key — `manifestation_id` MUST be the final tiebreaker (position and added_at are both non-unique, migration analysis confirmed)
- **TESTS FIRST**: walk test (seed page_size+1 items, two pages, terminates), reorder flow still green (existing PUT items tests untouched)
- **VALIDATE**: as Task 3

### Task 5: UPDATE `list_users` — defensive cap

- **ACTION**: add `LIMIT 500` via const `MAX_LISTED_USERS: i64 = 500` with justification comment (household-instance ceiling per ADR-7 exception; multi-hundred-user instance is out of design scope); utoipa 200 description documents the cap; response stays `[UserResponse]`
- **MIRROR**: `emit_series_books` cap comment style (`opds/library.rs:685–688`)
- **TESTS FIRST**: seed `MAX_LISTED_USERS + 1`? — 501 user inserts per test is heavy; instead make the cap a fn parameter… NO — keep simple: test with a small seeded count asserting order + a unit-level assertion that the SQL contains LIMIT is brittle. Pragmatic choice: integration test seeds 5 users, asserts all 5 + ordering (existing behavior); cap correctness covered by the query's static `LIMIT 500` + code review. Note this deviation in PR body (501-row seed rejected as test-suite cost)
- **VALIDATE**: as Task 3

### Task 6: UPDATE `backend/src/routes/opds/cursor.rs` — name-keyed cursor

- **ACTION**: add `NameCursor { sort_name: String, id: Uuid }` encode/parse with distinct tag prefix; unit tests
- **MIRROR**: existing `Cursor` in same file; same `CursorError` reuse
- **GOTCHA**: same `|`-in-name escaping concern as Task 2 — copy the books Title-cursor delimiter strategy
- **VALIDATE**: pure unit tests, no DB

### Task 7: UPDATE `feed.rs` + `emit_authors`/`emit_series` — paginate nav feeds

- **ACTION**: relax `add_next_link`'s `debug_assert_eq!(kind, Acquisition)` (`feed.rs:~363`) to allow Navigation (RFC 5005 Atom paging is feed-kind-agnostic; adjust the assert message/comment accordingly); in both emitters: parse incoming `cursor` (param already plumbed, currently discarded — delete the "load fully per plan decision" comments), add keyset predicate `(sort_name, id) > ($s, $i)` + `LIMIT page_size + 1`, emit `rel="next"` with `NameCursor`
- **MIRROR**: `emit_new` (`opds/library.rs:352–360, 452–470`)
- **GOTCHA**: both emitters serve library-scoped AND shelf-scoped twins (`self_parent` carries the path) — the next link must preserve scope, which `self_path` formatting already does. QueryBuilder + RLS transaction usage stays as-is (`acquire_with_rls`)
- **TESTS FIRST**: nav-feed walk test mirroring `opds/tests.rs:717–771` — seed page_size+1 authors (distinct names), walk rel=next until exhaustion, assert each author exactly once; repeat for series; shelf-scoped variant smoke
- **VALIDATE**: as Task 3

### Task 8: UPDATE `frontend/src/api/shelves.ts` — absorb wire change

- **ACTION**: `ShelfListResponseSchema = z.object({ items: z.array(ShelfSchema), next_cursor: z.string().nullable() })`; `listShelves` loops pages (`?cursor=`) accumulating `items` until `next_cursor` null, returns `Shelf[]` (signature unchanged); `ShelfWithItemsSchema` gains `next_cursor`; `getShelf` walks item pages, returns assembled `ShelfWithItems` with full `items` (and `next_cursor` stripped/ignored). Page components (`ShelvesListPage.tsx`, `ShelfDetailPage.tsx`, `UsersPage.tsx`) untouched
- **GOTCHA**: bound the walk loop (e.g. max 100 iterations, throw on overrun) — never trust a server loop condition unboundedly; pass `signal` through every fetch for query cancellation
- **TESTS**: existing frontend test setup — extend api client tests if present (check `frontend/src` for shelves api tests; if none exist, vitest unit with mocked fetch walking 2 pages)
- **VALIDATE**: `cd frontend && npm run lint && npm run type-check && npm test`

### Task 9: Regenerate artifacts

- **ACTION**: `cd backend && REGEN=1 SQLX_OFFLINE=true cargo test --test gen_openapi`; regen `.sqlx` cache (`cargo sqlx prepare -- --tests` — needs reachable cluster; if unreachable, hand-author entries is NOT acceptable at this diff size → run in CI-pushed state or via reverie-dev LXC psql access)
- **GOTCHA**: config schema NOT affected (no config struct change) — `gen_config_ref` regen unnecessary
- **VALIDATE**: `cargo test --test gen_openapi` (drift gate green), `cargo sqlx prepare --check -- --tests`

### Task 10: Full local validation + docs touch

- **ACTION**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, doc-lint (`RUSTDOCFLAGS=-D rustdoc::broken_intra_doc_links cargo doc --no-deps --workspace`), `typos`; update Starlight API-reference narrative if it describes the three JSON endpoints' shapes (check `docs/` for shelves/users pages — rationale-in-user-docs rule: pagination semantics + the users cap reasoning belong in user-facing docs)
- **VALIDATE**: all exit 0

### Task 11: PR

- **ACTION**: push, PR title `fix(api): bound all list queries — keyset pagination + users cap`, body carries `Closes UNK-374`, security-review note (resource-exhaustion surface closed; cursor inputs validated → 422; no new authz paths), breaking-wire-change note with the lockstep frontend update, the Task 5 test-depth deviation note
- **VALIDATE**: CI green; bot triage

---

## Testing Strategy

| Test                                                             | Validates                                                                                  |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `shelves` pagination walk (page_size=2, 3 shelves incl. system)  | envelope, Link header, mixed-direction keyset crosses is_system→name boundary, termination |
| shelf-items walk (page_size+1 items)                             | tuple keyset, ETag unchanged, termination                                                  |
| malformed cursor → 422 (each new cursor consumer)                | cursor error path                                                                          |
| users: order + shape (cap statically reviewed — see Task 5 note) | no regression                                                                              |
| OPDS authors/series rel=next walk, each row exactly once         | nav-feed paging, no drop/dup at boundary                                                   |
| OPDS shelf-scoped nav twin smoke                                 | scope preserved in next link                                                               |
| frontend: client walk assembles 2 mocked pages                   | loop + bound                                                                               |
| existing suites stay green                                       | no regression (reorder, RLS, 401/403)                                                      |

Edge cases: page boundary exactly at is_system flip; two shelf items sharing
`(position, added_at)` (forces manifestation_id tiebreaker — seed explicitly);
empty list (next_cursor null, no Link); cursor for deleted row (keyset is
tolerant by design — documents itself); author names containing `|`.

Seeding gotcha: tests creating works must use distinct vocabulary
(pg_trgm find_or_create >0.6 collapse — CLAUDE.md). Shelf/user/author/series
names have no trigram dedup but keep names order-deterministic (ASCII, no
locale-sensitive collation surprises).

## Validation Commands

```bash
cargo fmt --all -- --check
```

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```bash
SQLX_OFFLINE=true cargo check --tests
```

```bash
cargo test --test gen_openapi
```

```bash
cd frontend && npm run lint && npm run type-check && npm test
```

Full `#[sqlx::test]` suite: CI (push) — CI-first rule, `backend/CLAUDE.md`.

## Acceptance Criteria

- [ ] All five offender queries carry `LIMIT` bound by construction
- [ ] Keyset endpoints: stable total order incl. unique tiebreaker; no row dropped/duplicated at page boundaries (walk tests prove)
- [ ] Malformed cursors → 422 ProblemDetails
- [ ] `docs/openapi.json` regenerated, drift gate green
- [ ] Frontend lockstep: zero behavior change in page components
- [ ] ADR-7's non-compliant list is empty after merge (no ADR edit needed — it describes the state at decision time; optionally note in PR)
- [ ] PR body: `Closes UNK-374`, security-review answer, breaking-change + test-depth notes

## Risks and Mitigations

| Risk                                                                         | Likelihood | Impact              | Mitigation                                                                                              |
| ---------------------------------------------------------------------------- | ---------- | ------------------- | ------------------------------------------------------------------------------------------------------- |
| Mixed-direction keyset predicate subtly wrong (is_system DESC + name ASC)    | MED        | HIGH (dropped rows) | boundary-crossing walk test seeded to span the flip; mirror books' Author OR-expansion                  |
| `\|` in user-controlled name breaks cursor parse                             | MED        | MED                 | copy books Title-cursor delimiter strategy (Task 2 gotcha); malformed-cursor tests with hostile names   |
| OPDS clients ignore rel=next on navigation feeds → see only first 50 authors | LOW-MED    | MED                 | RFC 5005 standard paging; acceptable per ADR-7 (bounded beats unbounded); page_size configurable to 500 |
| Frontend walk loop on buggy server cursor → infinite                         | LOW        | MED                 | hard iteration bound + throw (Task 8)                                                                   |
| `.sqlx` cache regen needs reachable cluster                                  | MED        | LOW (process)       | reverie-dev LXC (`reverie-dev psql`) or CI round-trip; never hand-author at this size                   |
| Index bloat on write paths                                                   | LOW        | LOW                 | shelves/shelf_items write-light; database-reviewer pass (Task 1)                                        |

## Notes

- **Web research skipped deliberately**: governing contract (ADR-7), both cursor
  modules, and feed-building are first-party; no external library/version
  questions. RFC 5005 paging is already implemented by `feed.rs`.
- **Why users gets a cap, not keyset**: ADR-7 exception requires a "genuinely
  small natural ceiling" — a household/self-hosted instance's user count
  qualifies; shelves/items/authors/series scale with library size and do not.
  Issue text endorses exactly this split.
- **Single shared page-size knob** (`opds.page_size`) is the status quo for
  `/api/v1/books`; this plan extends rather than revisits it.
- **Possible PR split** if review size is a concern: PR-A JSON endpoints
  (Tasks 1–5, 8 partial), PR-B OPDS (6, 7). Default: one PR — shared cursor
  groundwork and one regen cycle.

**Confidence Score**: 8/10 — every pattern has a working in-repo exemplar with
walk tests to copy; deductions for mixed-direction keyset subtlety and the
two lockstep wire changes.
