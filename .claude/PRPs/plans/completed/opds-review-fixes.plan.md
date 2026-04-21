# Plan: OPDS Review Fixes (PR #26 follow-up)

## Summary

Address findings from the multi-agent review of PR #26 (`feat/opds-catalog`).
Two correctness bugs (auth error collapse, series pagination skip), one silent
background-task failure, five missing test cases, one misleading error variant,
a comment-hygiene pass, three helper extractions for de-duplication, and a docs
touch-up. All work lands as follow-up commits on the existing branch
`feat/opds-catalog`; **no new PR**.

## Problem → Solution

**Current state:** PR #26 implements OPDS 1.2 correctly in the happy path and
covers the security-sensitive boundaries (path traversal, RLS, XML
sanitisation). Review found:

- `BasicOnly` extractor collapses `Err(AppError::Internal)` into a 401
  challenge, hiding DB-availability failures behind a credential prompt.
- `emit_series_books` cursor predicate `(created_at, id)` does not match the
  `ORDER BY sw.position ASC NULLS LAST, …` key, so later-created books at
  higher positions are silently dropped from page 2 of any series ≥ page_size.
- `tokio::spawn(let _ = update_last_used(...))` swallows persistent
  background-write errors with zero visibility.
- `OpdsConfig::from_env` validation branches have no coverage (tests disabled
  OPDS wholesale to avoid `public_url` requirement).
- Three handler paths use `|_|` on `Cursor::parse` but have no HTTP-level
  assertion that invalid cursors return 422.
- Pagination boundary cases (empty library, exactly page_size rows, wrong
  password, disabled OPDS) have no tests.
- `CoverError::Decode(e.to_string())` is applied to DB errors, mislabelling
  them in logs.
- Plan-artifact references (`Phases D–G`, `Step 10`, `BLUEPRINT:`) leaked into
  production source comments; one comment is factually wrong (`pos_key`
  derived column).
- Three paginated handlers duplicate identical 3-line cursor-parse,
  push-cursor-predicate, and split-page blocks.
- `EPUB_MIME` is declared in both `feed.rs` and `download.rs`.
- Six `let _ = assert_shelf_owned(...).await?` bindings suppress a
  non-existent `#[must_use]` warning.
- `backend/CLAUDE.md` auth-module listing is missing `basic_only.rs`.

**Desired state:** both correctness bugs have regression tests and are fixed;
the silent background failure is logged; the five missing test cases exist;
`CoverError::Db` distinguishes DB failures from image-decode failures;
comments and duplication are cleaned; docs reflect the new module.

## Metadata

- **Complexity:** Small (~10 files, ~400 LOC + tests)
- **Source:** PR #26 review comment
  <https://github.com/unkos-dev/reverie/pull/26#issuecomment-4289686918>
- **Branch:** `feat/opds-catalog` (already checked out; PR #26 open)
- **Depends on:** none — all findings are against code already on this branch
- **Tier:** Moderate (two correctness fixes + regression tests)

---

## Patterns to Mirror

| Pattern | Source | Used in tasks |
|---|---|---|
| `match verify_basic` arm split | `backend/src/auth/middleware.rs:103-128` (`CurrentUser::from_request_parts` propagates `Err(?)` correctly) | Task 1 |
| Single-page emit without cursor | `backend/src/routes/opds/library.rs` — `emit_search` | Task 2 |
| `if let Err(e) = … tracing::warn!(…)` background-task logging | `backend/src/services/enrichment/orchestrator.rs:498-510` | Task 3 |
| `#[sqlx::test(migrations = "./migrations")]` with `app_pool_for`/`ingestion_pool_for` | `backend/src/routes/opds/tests.rs` (existing tests) | Tasks 5–10 |
| Config `with_env` test harness | `backend/src/config.rs` (existing tests) | Tasks 6–9 |
| `thiserror` variant addition | `backend/src/services/covers/error.rs` | Task 11 |
| Helper fn alongside `push_scope` | `backend/src/routes/opds/scope.rs` | Task 15 |

---

## Files to Change

| File | Action | Purpose |
|---|---|---|
| `backend/src/auth/basic_only.rs` | UPDATE | Split `_ =>` arm so `Err(Internal)` propagates |
| `backend/src/auth/middleware.rs` | UPDATE | Log `update_last_used` failures in spawned task |
| `backend/src/routes/opds/library.rs` | UPDATE | Single-page `emit_series_books`; extract `parse_cursor`/`push_cursor_predicate`/`split_page`; delete false `pos_key` comment |
| `backend/src/routes/opds/mod.rs` | UPDATE | Remove `Phase D–G` / `Step 10` plan references |
| `backend/src/routes/opds/shelves.rs` | UPDATE | Strip `BLUEPRINT:` prefix; drop `let _ =` bindings |
| `backend/src/routes/opds/feed.rs` | UPDATE | Remove `// <id>` / `// <title>` / `// <author>` label comments |
| `backend/src/routes/opds/download.rs` | UPDATE | Import `EPUB_MIME` from `feed`; drop `release DB tx` and `collapse dashes` comments |
| `backend/src/routes/opds/tests.rs` | UPDATE | Add 5 regression tests |
| `backend/src/services/covers/error.rs` | UPDATE | Add `CoverError::Db(String)` variant |
| `backend/src/services/covers/mod.rs` | UPDATE | Map sqlx errors to `CoverError::Db` |
| `backend/src/config.rs` | UPDATE | Add 4 `OpdsConfig::from_env` validation tests |
| `backend/CLAUDE.md` | UPDATE | Add `basic_only.rs` to auth tree |

---

## NOT Building (Scope Limits — with rationale)

| Item | Why deferred |
|---|---|
| Convert `OpdsConfig` to `enum { Disabled, Enabled { … } }` | Good modeling instinct, but the churn (main.rs + every `public_url.as_ref()` handler + test literals) is disproportionate to the runtime risk. Startup validation already rejects `enabled=true` + missing `public_url`. Revisit as a standalone refactor. |
| `FeedBuilder::expect()` → `Result` plumbing | Writes target `Cursor<Vec<u8>>`; `quick-xml` cannot fail on that sink. Silent-failure-hunter's own note: "structurally unreachable today." Defensive reshaping of `finish()` is not worth the churn. |
| `compute_hex_sha256` `let _ = write!` → `.expect` | `fmt::Write` on `String` is infallible by construction. The "future type change could silently corrupt" scenario is defensive against a bug nobody has written. |
| `|_|` on `Cursor::parse` → full `CursorError` in log | Raw cursor is in the access log already and `CursorError` variants are covered by unit tests; context loss is low-severity. |
| `covers.rs` inline closures → named handlers | Style consistency with other modules, not a correctness issue. |
| `opensearch.rs` inline ownership check → reuse `assert_shelf_owned` | Minor duplication; worth doing only if `assert_shelf_owned` is promoted to `pub(super)` for other reasons. |
| `DeviceToken` custom `Debug` impl to redact hash | Argon2 hashes are one-way; the current `#[serde(skip)]` prevents JSON leaks. Custom `Debug` is over-engineering for this trust model. |

These deferrals are intentional — do not silently pick them up.

---

## Step-by-Step Tasks

Each task follows the repo TDD rule (`CLAUDE.md` §Hard Rules 5): write the
failing test first, confirm red, implement, confirm green. Run
`cargo check -p reverie-api` after every edit; run targeted `cargo test` after
each task. Final Level-3 run comes at the end.

### Phase A — Correctness fixes (block merge)

#### Task 1: UPDATE `backend/src/auth/basic_only.rs` — propagate `Err(Internal)`

- **TEST FIRST**: add a `#[sqlx::test]` to `backend/src/routes/opds/tests.rs`
  that drops the `users` table (or similar DB-break) after server construction,
  then issues a Basic-authenticated OPDS request and asserts
  `response.status_code() == INTERNAL_SERVER_ERROR` (500), NOT 401. The
  simpler variant: close the pool before the request. Inspect `verify_basic`
  error paths in `middleware.rs:74,78` to pick a reproducible break.
- **ACTION**: replace the wildcard arm:
  ```rust
  match verify_basic(state, parts).await {
      Ok(Some(user)) => Ok(BasicOnly(user)),
      Ok(None) | Err(AppError::Unauthorized) => Err(AppError::BasicAuthRequired {
          realm: state.config.opds.realm.clone(),
      }),
      Err(other) => Err(other),
  }
  ```
- **VALIDATE**: new test passes; all existing `tests::unauthenticated_*` /
  `revoked_token_rejected` still green.

#### Task 2: UPDATE `backend/src/routes/opds/library.rs` — single-page `emit_series_books`

- **TEST FIRST**: add a test that inserts a series with N > page_size
  manifestations where `created_at` is NOT monotonic with `position`
  (e.g. pos=1,c=2020 / pos=2,c=2022 / … / pos=(page_size+1),c=2023). Hit
  `/opds/library/series/{id}` and assert the response contains every
  manifestation's URN (`urn:reverie:manifestation:{uuid}`). With today's bug
  this fails: the last few high-position rows are dropped on page 2.
- **ACTION**: remove cursor support from `emit_series_books`:
  drop the `cursor` parameter, the `push_cursor_predicate` block, and the
  `add_next_link` call; emit the whole series in one page (cap at a generous
  limit, e.g. `page_size * 10`, so we don't accidentally OOM on a pathological
  10,000-book series; ORDER BY stays the same). Update the false comment at
  `library.rs:540-543` to describe the actual ordering (`NULLS LAST` via
  PostgreSQL syntax; no `pos_key` derived column).
- **MIRROR**: `emit_search` in `library.rs` — same single-page shape.
- **VALIDATE**: new test green; existing series tests still green.

#### Task 3: UPDATE `backend/src/auth/middleware.rs` — log `update_last_used` failures

- **ACTION**: change `middleware.rs:92-94`:
  ```rust
  tokio::spawn(async move {
      if let Err(e) = device_token::update_last_used(&pool, token_id).await {
          tracing::warn!(
              error = %e,
              %token_id,
              "device_token: update_last_used failed (non-fatal)"
          );
      }
  });
  ```
- **TEST**: no new unit test (the warn is diagnostic; side-effect-only). If a
  test is easy, verify the error branch compiles cleanly against
  `AppError`/`sqlx::Error`.
- **VALIDATE**: `cargo check` + `cargo clippy --all-targets -- -D warnings`.

### Phase B — Missing tests

#### Task 4: UPDATE `backend/src/routes/opds/tests.rs` — invalid cursor returns 422

- **ACTION**: new `#[sqlx::test]` that GETs
  `/opds/library/new?cursor=!!!not-base64url!!!` with a valid `BasicOnly`
  header and asserts `StatusCode::UNPROCESSABLE_ENTITY`. Mirror on
  `/opds/library/authors/{id}?cursor=…` and
  `/opds/shelves/{shelf_id}/new?cursor=…` — one test per handler, or a single
  parameterised test that walks all three paths.
- **VALIDATE**: `cargo test --lib opds::tests::invalid_cursor`.

#### Task 5: UPDATE `backend/src/routes/opds/tests.rs` — empty library feed has no next link

- **ACTION**: `#[sqlx::test]` that builds the OPDS server with an empty
  `manifestations` table, GETs `/opds/library/new`, asserts 200, asserts body
  contains zero `<entry>` elements, and asserts body does NOT contain
  `rel="next"`.
- **VALIDATE**: `cargo test --lib opds::tests::empty_library`.

#### Task 6: UPDATE `backend/src/routes/opds/tests.rs` — exactly `page_size` rows has no next link

- **ACTION**: insert exactly `page_size` (50) manifestations, GET
  `/opds/library/new`, assert 200, assert 50 `<entry>` elements, assert no
  `rel="next"`. This is the off-by-one sentinel for `has_more = rows.len() >
  page_size`.
- **VALIDATE**: `cargo test --lib opds::tests::exact_page_size`.

#### Task 7: UPDATE `backend/src/routes/opds/tests.rs` — wrong password returns challenge

- **ACTION**: `#[sqlx::test]` that constructs a Basic header with a valid user
  UUID but a wrong token plaintext, GETs `/opds`, asserts
  `StatusCode::UNAUTHORIZED` AND that `WWW-Authenticate` header is present and
  starts with `Basic `. Distinct from the existing
  `unauthenticated_returns_challenge` (missing header) and
  `revoked_token_rejected` (revoked token).
- **VALIDATE**: `cargo test --lib opds::tests::wrong_password`.

#### Task 8: UPDATE `backend/src/routes/opds/tests.rs` — disabled OPDS returns 404

- **ACTION**: plain `#[tokio::test]` (no DB) using the existing `test_server()`
  helper which builds with `opds.enabled=false`; GET `/opds` and assert
  `StatusCode::NOT_FOUND`. Covers the `router_enabled()` gate contract.
- **VALIDATE**: `cargo test --lib opds::tests::opds_disabled`.

#### Task 9: UPDATE `backend/src/config.rs` — `OpdsConfig::from_env` validation tests

- **ACTION**: four new tests using the existing `with_env` harness:
  1. `opds_enabled_without_public_url_errors`
  2. `opds_page_size_out_of_range_errors` (value `0` and `501`)
  3. `opds_realm_with_double_quote_errors`
  4. `opds_enabled_with_valid_public_url_parses` (happy path)
- **VALIDATE**: `cargo test --lib config::`.

### Phase C — Error hygiene

#### Task 10: UPDATE `backend/src/services/covers/error.rs` — add `CoverError::Db(String)`

- **TEST FIRST**: if a unit test exists for `CoverError`, extend it; otherwise
  skip — this is a variant addition verified by the typechecker.
- **ACTION**: add `#[error("db: {0}")] Db(String)` alongside existing variants.
- **VALIDATE**: `cargo check`.

#### Task 11: UPDATE `backend/src/services/covers/mod.rs` — remap DB errors

- **ACTION**: the two `map_err(|e| CoverError::Decode(e.to_string()))` sites
  at `covers/mod.rs:53-62` (`acquire_with_rls` and the `query_as` call) become
  `map_err(|e| CoverError::Db(format!("covers: {e}")))`. Any other sqlx errors
  in the module: audit and remap similarly.
- **VALIDATE**: `cargo test --lib covers`; no existing test should regress.

### Phase D — Comment and simplification cleanup

#### Task 12: UPDATE `backend/src/routes/opds/library.rs` — remove false `pos_key` comment

- **ACTION**: delete the 4-line `Series ordering … derived column pos_key …`
  comment at `library.rs:540-543`. Replace with a one-liner if any comment is
  needed at all: `// Series order: position ASC NULLS LAST, then newest
  first.` — or remove entirely if the SQL is self-evident.
- **VALIDATE**: `cargo check`.

#### Task 13: UPDATE `backend/src/routes/opds/mod.rs` — strip plan references

- **ACTION**: delete `// Filled in during Phases D–G.` at `mod.rs:14`. At
  `mod.rs:46`, remove the phrase `Step 10 ` from the `CurrentUser` doc
  comment; the sentence still makes sense without it.
- **VALIDATE**: `cargo check`.

#### Task 14: UPDATE `backend/src/routes/opds/shelves.rs` — strip BLUEPRINT, drop `let _ =`

- **ACTION** (a): `shelves.rs:4` — replace `BLUEPRINT: cross-user access
  returns 404, not 403` with `returns 404 for foreign shelves to avoid leaking
  shelf existence`.
- **ACTION** (b): on lines 81, 102, 122, 145, 165, 188, change
  `let _ = assert_shelf_owned(...).await?;` to
  `assert_shelf_owned(...).await?;`.
- **VALIDATE**: `cargo test --lib opds::shelves`.

#### Task 15: UPDATE `backend/src/routes/opds/feed.rs` and `download.rs` — drop restatement comments

- **ACTION** (feed.rs): remove the inline `// <id>`, `// <title>`,
  `// <updated>`, `// <author>/<name> per creator.`, `// <dc:identifier> …`,
  `// Stable URN per manifestation for client bookmarks.` comments. Keep the
  module-level `//!` and `///` docs (those carry the XML 1.0 Char rationale
  and RFC 4287 citation — they're load-bearing).
- **ACTION** (download.rs): remove `// release DB transaction before I/O` from
  the `drop(tx)` line; remove `// collapse multiple dashes` from the
  `ascii_fallback` helper.
- **VALIDATE**: `cargo check`.

#### Task 16: UPDATE `backend/src/routes/opds/library.rs` — extract three helpers

- **ACTION**: add private helpers alongside the existing code (near `Scope` /
  `push_scope` feels right, but keep them in `library.rs` unless they're used
  from `shelves.rs` too):
  ```rust
  fn parse_cursor(raw: Option<String>) -> Result<Option<super::cursor::Cursor>, AppError> {
      raw.as_deref()
          .map(super::cursor::Cursor::parse)
          .transpose()
          .map_err(|_| AppError::Validation("invalid cursor".into()))
  }

  fn push_cursor_predicate(
      qb: &mut QueryBuilder<'_, Postgres>,
      cursor: &Option<super::cursor::Cursor>,
  ) {
      if let Some(c) = cursor {
          qb.push(" AND (m.created_at, m.id) < (");
          qb.push_bind(c.created_at);
          qb.push(", ");
          qb.push_bind(c.id);
          qb.push(")");
      }
  }

  fn split_page<'a>(
      rows: &'a [sqlx::postgres::PgRow],
      page_size: i64,
  ) -> (&'a [sqlx::postgres::PgRow], bool) {
      let has_more = rows.len() as i64 > page_size;
      let page_rows = if has_more { &rows[..page_size as usize] } else { rows };
      (page_rows, has_more)
  }
  ```
  Replace the three duplicated blocks at L224/383/534 (parse), L241/402/557
  (predicate), and L257/418/573 (split) with calls. After Task 2,
  `emit_series_books` no longer paginates — only two callers remain for
  `push_cursor_predicate` and `split_page`. Still worth the helper if the
  other two sites are identical.
- **VALIDATE**: `cargo test --lib opds`.

#### Task 17: UPDATE `backend/src/routes/opds/download.rs` — import `EPUB_MIME`

- **ACTION**: delete `pub const EPUB_MIME: &str = "application/epub+zip";` at
  `download.rs:26`. Add `use super::feed::EPUB_MIME;` (or fully-qualify
  inline). Verify no external caller imports `download::EPUB_MIME`.
- **VALIDATE**: `cargo check`; `rg 'download::EPUB_MIME'` returns nothing.

### Phase E — Docs

#### Task 18: UPDATE `backend/CLAUDE.md` — list `basic_only.rs`

- **ACTION**: in the `auth/` block under "Project Structure", add a line:
  ```text
  │   │   ├── basic_only.rs # BasicOnly extractor (OPDS Basic-only auth)
  ```
  Alphabetise if the existing order is alphabetical (it appears to be:
  backend, middleware, oidc, token).
- **VALIDATE**: visual check.

---

## Validation Commands

Run from `backend/`. All commands must return exit 0.

### Level 1: Static analysis
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

### Level 2: Targeted tests
```bash
cargo test --lib opds
cargo test --lib auth
cargo test --lib config
cargo test --lib covers
```

### Level 3: Full suite + release build
```bash
cargo test
cargo build --release
```

---

## Acceptance Criteria

- [ ] Both Critical findings have passing regression tests: `BasicOnly`
      propagates `Err(Internal)` as 500 (Task 1); series of >page_size books
      with non-monotonic `created_at` renders every entry (Task 2).
- [ ] Five new tests exist and pass: invalid-cursor 422, empty-library
      no-next, exact-page no-next, wrong-password challenge, OPDS-disabled 404
      (Tasks 4–8).
- [ ] Four new config validation tests pass (Task 9).
- [ ] `CoverError::Db` variant exists and replaces `Decode` for DB errors in
      `covers/mod.rs` (Tasks 10–11).
- [ ] `update_last_used` failures produce a `tracing::warn!` log line
      (Task 3).
- [ ] No `pos_key`, `Phase D–G`, `Step 10`, or `BLUEPRINT:` references in
      production Rust source (Tasks 12–14).
- [ ] `// <id>`, `// <title>`, `// <author>…`, `// release DB transaction…`,
      `// collapse multiple dashes` comments removed (Task 15).
- [ ] Three duplicated cursor/pagination blocks replaced with helpers
      (Task 16).
- [ ] `EPUB_MIME` has a single definition (Task 17).
- [ ] `backend/CLAUDE.md` lists `basic_only.rs` under `auth/` (Task 18).
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
      `cargo test`, and `cargo build --release` all pass.
- [ ] Changes land as follow-up commits on `feat/opds-catalog`; **no new PR**.

---

## Completion Checklist

- [ ] All 18 tasks completed.
- [ ] Level 1, 2, 3 validation passes.
- [ ] All 11 acceptance criteria verified.
- [ ] Commits follow Conventional Commits (scopes: `opds`, `auth`, `config`,
      `covers`, `docs`).
- [ ] PR #26 on GitHub receives the follow-up commits; PR description
      optionally updated to note review fixes applied.
- [ ] No regression in pre-existing OPDS, auth, covers, or config tests.

---

## Commit Strategy

Commit by topic, not by task, so the history is reviewable:

1. `fix(auth): propagate internal errors through BasicOnly extractor` — Task 1
2. `fix(opds): emit complete series feed without cursor pagination` — Task 2
3. `fix(auth): log device-token last_used update failures` — Task 3
4. `test(opds): cover invalid cursor, pagination bounds, auth edges` —
   Tasks 4–8
5. `test(config): cover OpdsConfig validation branches` — Task 9
6. `refactor(covers): distinguish DB errors from decode errors` — Tasks 10–11
7. `refactor(opds): extract pagination helpers and remove stale comments` —
   Tasks 12–17
8. `docs(backend): list basic_only.rs in CLAUDE.md auth tree` — Task 18
