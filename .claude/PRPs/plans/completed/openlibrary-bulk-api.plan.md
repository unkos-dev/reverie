# Plan: OpenLibrary Bulk API + Identified Requests

## Summary

Swap the `OpenLibrary` adapter from `/isbn/{isbn}.json` to the richer
`/api/books?bibkeys=ISBN:X&jscmd=data` endpoint, which returns humanised
records with author names, publisher, subjects, and cover URLs in one hop
(today's adapter skips authors because they arrive as opaque
`/authors/OL...` keys). At the same time, introduce a User-Agent
identification string on the shared HTTP clients so the process plays nice
with OpenLibrary's courtesy rate policy and gets the documented 3× quota
bump (1 req/s → 3 req/s). Out of scope: multi-ISBN batching across
manifestations — that requires an orchestrator-level refactor to surface
pending-queue batches to `MetadataSource` and is deferred.

## User Story

As a self-hoster with a few thousand books,
I want the OpenLibrary adapter to enrich 3× faster and include author names
out of the box,
So that bulk enrichment completes in minutes instead of hours and authors
no longer need to fall through to Google Books for attribution.

## Problem → Solution

**Current:** `OpenLibrary::lookup` hits the sparse `/isbn/{isbn}.json`
endpoint, which returns an Edition record without resolved authors (they
come as `[{key: "/authors/OL...M"}]`). The adapter explicitly skips author
emission. Requests go out anonymously, so they're capped at 1 req/s.

**Desired:** The adapter hits `/api/books?bibkeys=ISBN:{X}&jscmd=data` and
parses the humanised response shape — authors resolve inline to names,
publishers/subjects/cover URLs are included, and we stop needing a second
hop for attribution. A `User-Agent: Tome/{version} ({contact})` header is
set on the shared reqwest clients so OpenLibrary grants the identified 3
req/s quota, and the per-adapter governor bumps to match.

## Metadata

- **Complexity**: Small
- **Source PRD**: None (follow-up to Step 7 enrichment pipeline)
- **Depends On**: `feat/metadata-enrichment-pipeline` merged to `main`
- **Estimated Files**: 5-6 (0 new, 5-6 modified)

---

## Mandatory Reading

| Priority | File | Why |
|---|---|---|
| P0 | `backend/src/services/enrichment/sources/open_library.rs` | Adapter to rewrite — both URL construction and response parser |
| P0 | `backend/src/services/enrichment/http.rs` | Shared clients; User-Agent header goes here |
| P0 | `backend/src/config.rs` | New `TOME_OPERATOR_CONTACT` + derived `user_agent()` |
| P1 | `backend/src/services/enrichment/sources/mod.rs` | `MetadataSource` trait + `LookupCtx` — no shape change, just context |
| P1 | `backend/src/services/enrichment/orchestrator.rs` | `build_sources` + `run_once` — no change expected, used for wiring verification |
| P2 | `.env.example` | Document the new operator-contact knob |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| `/api/books?bibkeys=...&jscmd=data` | openlibrary.org/dev/docs/api/books | Response shape is keyed by bibkey (`{"ISBN:0385472579": {...}}`); each value has `title`, `authors: [{url, name}]`, `publishers`, `subjects`, `cover.{small,medium,large}`, `publish_date`. `jscmd=data` returns humanised view; `jscmd=details` returns raw record. |
| Rate limit policy | openlibrary.org/developers/api | Default 1 req/s anonymous; **3 req/s for identified requests** via `User-Agent: AppName (contact@example.com)`. |
| Existing adapter parser for old shape | `open_library.rs::map_isbn_response` | Authors deliberately skipped — gets fixed naturally by `/api/books` format. |

---

## Patterns to Mirror

### SHARED_CLIENT_HEADERS
```rust
// backend/src/services/enrichment/http.rs
pub fn api_client(user_agent: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(10))
        .redirect(redirect::Policy::limited(5))
        .build()
        .expect("api_client build")
}
```
The caller now threads the UA string. `cover_client` gets the same treatment.

### CONFIG_DERIVATION
```rust
// backend/src/config.rs
impl Config {
    pub fn user_agent(&self) -> String {
        match self.operator_contact.as_deref() {
            Some(contact) => format!("Tome/{} ({contact})", env!("CARGO_PKG_VERSION")),
            None => format!("Tome/{} (unidentified)", env!("CARGO_PKG_VERSION")),
        }
    }
}
```
Log a single warning at startup when `operator_contact` is unset — the
queue still works, just at the 1 req/s tier.

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/src/services/enrichment/http.rs` | UPDATE | Add `user_agent: &str` param to `api_client` + `cover_client` builders |
| `backend/src/services/enrichment/sources/open_library.rs` | UPDATE | Switch URL to `/api/books?bibkeys=ISBN:{X}&jscmd=data`; rewrite `map_isbn_response` for the keyed-by-bibkey response; bump `limiter()` quota to 3 req/s; rename `map_isbn_response` → `map_api_books_response` |
| `backend/src/services/enrichment/sources/google_books.rs` | UPDATE | No shape change; accept the new User-Agent (propagated via shared client) |
| `backend/src/services/enrichment/sources/hardcover.rs` | UPDATE | Same as Google Books |
| `backend/src/services/enrichment/orchestrator.rs` | UPDATE | `api_client()` call now takes the UA string from `config.user_agent()`; `fan_out_for_dry_run` too |
| `backend/src/config.rs` | UPDATE | Add `operator_contact: Option<String>` parsed from `TOME_OPERATOR_CONTACT`; add `user_agent()` derivation |
| `backend/src/main.rs` | UPDATE (minor) | Startup warning when `operator_contact` is `None` |
| `.env.example` | UPDATE | Document `TOME_OPERATOR_CONTACT` |

---

## Step-by-Step Tasks

### Task 1: Config — add `operator_contact` + `user_agent()`
- **ACTION**: Add `operator_contact: Option<String>` to `Config`; parse from `TOME_OPERATOR_CONTACT` (optional); add `user_agent()` method.
- **MIRROR**: Existing optional env-var patterns in `Config::from_env` (e.g. `googlebooks_api_key`).
- **VALIDATE**: Extend existing config unit tests to cover the new field's default + explicit value.

### Task 2: `http.rs` — thread User-Agent through both clients
- **ACTION**: Add `user_agent: &str` parameter to `api_client` and `cover_client`. Set via `reqwest::ClientBuilder::user_agent`.
- **MIRROR**: Existing builder chain in `http.rs`.
- **GOTCHA**: Existing call sites in `orchestrator.rs` + `cover_download.rs` tests need updating; the test-only default of `reqwest::Client::new()` can remain untouched.
- **VALIDATE**: `cargo check --tests`.

### Task 3: `OpenLibrary` adapter — switch to `/api/books?jscmd=data`
- **ACTION**: Rewrite the ISBN URL to `{base}/api/books?bibkeys=ISBN:{isbn}&jscmd=data&format=json`. Leave the `/search.json` path alone (it remains the right endpoint for title/author fallback).
- **IMPLEMENT**:
  - `map_api_books_response(body: &Value, isbn_key: &str) -> Vec<SourceResult>` — look up the keyed sub-object and pull `title`, `authors[0..].name` → `creators` field, `publishers[0].name` → `publisher`, `publish_date`, `subjects[0..].name`, `cover.large` / `.medium` / `.small` → `cover_url`, plus ISBN-13/10 from `identifiers.isbn_13` / `identifiers.isbn_10`.
  - Replace the deliberate-skip comment for authors; emit a `creators` `SourceResult`.
  - Bump `limiter()` to `RateLimiter::direct(Quota::per_second(NonZeroU32::new(3).unwrap()))`.
- **MIRROR**: Existing `map_search_response` for pattern; the new function is a strict superset.
- **VALIDATE**: `cargo test services::enrichment::sources::open_library` green.

### Task 4: Orchestrator — pass UA when building clients
- **ACTION**: `run_once` and `fan_out_for_dry_run` now compute `let ua = config.user_agent();` and call `api_client(&ua)`.
- **VALIDATE**: `cargo check`.

### Task 5: Main — startup warning when unset
- **ACTION**: In `main.rs`, after config load, if `config.operator_contact.is_none()` emit `tracing::warn!("TOME_OPERATOR_CONTACT unset — OpenLibrary requests will run at the 1 req/s anonymous tier")`.
- **VALIDATE**: Visual on startup.

### Task 6: Wiremock tests — update fixtures to `/api/books` shape
- **ACTION**: In `open_library.rs::tests`, the ISBN test fixtures currently match `path("/isbn/{isbn}.json")`. Update to `path("/api/books")` and the response body to the keyed-by-bibkey shape: `{"ISBN:9780441172719": {"title": "Dune", "authors": [{"name": "Frank Herbert", "url": "..."}], ...}}`.
- **ACTION (Phase D integration tests)**: Update `services::enrichment::orchestrator::tests::mock_openlibrary_isbn` to match the new path + body.
- **VALIDATE**: `cargo test services::enrichment` green; Phase D integration tests still pass under `--ignored`.

### Task 7: `.env.example` + user-facing docs
- **ACTION**: Document `TOME_OPERATOR_CONTACT=you@example.com` with a note explaining the 3× rate-limit benefit.
- **MIRROR**: Existing env-var doc comments.

---

## NOT Building

- Multi-ISBN batching (passing 2-100 bibkeys per request). Requires a
  batch method on `MetadataSource` + a queue-level batcher; that's a
  separate plan. The single-ISBN-per-call shape remains, but the per-call
  payload is now much richer.
- A local OpenLibrary dump mirror. Discussed and deferred.
- Changes to Google Books or Hardcover endpoints — they stay as-is.

---

## Testing Strategy

### Unit Tests

| Test | Input | Expected |
|---|---|---|
| `config::user_agent_with_contact` | `TOME_OPERATOR_CONTACT=a@b.c` | `Tome/{version} (a@b.c)` |
| `config::user_agent_without_contact` | unset | `Tome/{version} (unidentified)` |
| `open_library::map_api_books_response_happy` | keyed body with all fields | returns title, creators, publisher, pub_date, subjects, cover_url, isbn_10, isbn_13 SourceResults |
| `open_library::map_api_books_response_missing_key` | `{}` | returns `Vec::new()` (treated as miss) |
| `open_library::map_api_books_response_partial` | body with only title | returns only the title SourceResult |

### Integration Tests (wiremock)

Update the existing 4 wiremock tests in `open_library.rs`:
- 200 happy (new shape)
- 404 → treated as miss (returns empty list, not error)
- 429 with Retry-After → `SourceError::RateLimited`
- 500 → `SourceError::Http(500)`

Phase D orchestrator integration tests in `services::enrichment::orchestrator::tests` need their `mock_openlibrary_isbn` helper updated; scenarios otherwise unchanged.

### Edge Cases Checklist

- [ ] Missing `/api/books` response key → return empty (clean miss).
- [ ] Author with URL but no `name` field → skip silently.
- [ ] `cover` field present but all sizes empty strings → skip `cover_url` emission.
- [ ] `publish_date` in raw "June 1, 2003" form → pass through (pub_date normaliser in `value_hash.rs` already handles leading YYYY-MM-DD prefix only; non-ISO values stage rather than apply).

---

## Validation Commands

```bash
cd backend
cargo fmt --check
cargo clippy --all-targets -- -D warnings   # new code only; 2 pre-existing errors remain OOS
cargo test services::enrichment
cargo test --bin tome-api                   # full non-ignored suite
DATABASE_URL_INGESTION="postgres://tome_ingestion:tome_ingestion@tome-postgres:5432/tome_dev" \
  DATABASE_URL="postgres://tome_app:tome_app@tome-postgres:5432/tome_dev" \
  cargo test --bin tome-api -- --ignored    # Phase D regression
```

Manual smoke (optional, requires real network):

```bash
# Start the stack, ingest one EPUB with a known ISBN, trigger enrichment
TOME_OPERATOR_CONTACT=you@example.com cargo run --bin tome-api
# In another shell, observe tracing for "hardcover disabled" / OpenLibrary
# requests hitting /api/books — the rate-limit governor should now allow
# bursts of 3 per second rather than 5 per minute.
```

---

## Acceptance Criteria

- [ ] OpenLibrary `/api/books?bibkeys=ISBN:X&jscmd=data` URL is used for ISBN lookups.
- [ ] Authors are emitted as `creators` `SourceResult` from OpenLibrary (not skipped).
- [ ] `User-Agent` header is set on both `api_client` and `cover_client` via `config.user_agent()`.
- [ ] `TOME_OPERATOR_CONTACT` is documented in `.env.example` and emits a startup warning when unset.
- [ ] OpenLibrary governor is at 3 req/s (identified) not 1 req/s.
- [ ] All existing OpenLibrary wiremock tests pass against the new shape.
- [ ] Phase D integration tests still pass with the updated mocks.
- [ ] `cargo fmt --check` clean; `cargo clippy -D warnings` has no new errors.

---

## Branching

```bash
git switch main
git pull
git switch -c feat/openlibrary-bulk-api
```

Target: single PR. Expected diff size: ~300-400 LOC changed across ~6 files.

## Estimated Effort

~1 day: ~4 h code + ~2 h test fixture updates + ~2 h validation / manual smoke.
