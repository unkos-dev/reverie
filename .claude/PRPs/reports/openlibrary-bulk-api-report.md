# Implementation Report: OpenLibrary Bulk API + Identified Requests

## Summary

Replaced the OpenLibrary ISBN adapter's `/isbn/{isbn}.json` endpoint with the
humanised `/api/books?bibkeys=ISBN:X&jscmd=data&format=json` endpoint so the
single-hop response carries author names, publishers, subjects, and cover URLs
inline (Hash-keyed by bibkey). Threaded a new `User-Agent` string —
`Tome/{version} ({contact})` — through both `api_client` and `cover_client`
builders, sourced from the new optional `TOME_OPERATOR_CONTACT` env var.
Bumped the OpenLibrary per-module governor from `5 req/min` to `3 req/s`
(OpenLibrary's identified-request tier). Emits a startup warning when the
operator contact is unset so the default quota tier is discoverable.

Implementation was folded onto `feat/metadata-enrichment-pipeline` (branch
option 3) rather than stacked on a fresh branch off `main`, at the user's
direction.

## Assessment vs Reality

| Metric | Predicted (Plan) | Actual |
|---|---|---|
| Complexity | Small | Small |
| Confidence | (unstated) | High |
| Files Changed | 5-6 modified | 9 modified (4 extra Config struct literals) |
| LOC | 300-400 | ~280 |

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 1 | Config: `operator_contact` + `user_agent()` | Complete | Two new unit tests |
| 2 | `http.rs`: thread UA through `api_client`/`cover_client` | Complete | |
| 3 | OpenLibrary adapter switch to `/api/books?jscmd=data` | Complete | 5 new pure-parser tests |
| 4 | Orchestrator: pass UA to client factories | Complete | `run_once` + `fan_out_for_dry_run` |
| 5 | main.rs startup warning | Complete | |
| 6 | Update wiremock fixtures to `/api/books` shape | Complete | Phase D helper `mock_openlibrary_isbn` rewraps legacy bodies automatically |
| 7 | `.env.example` docs | Complete | |
| 8 | Validation | Complete | fmt clean, clippy clean (minus 2 pre-existing OOS errors), 197/197 unit pass, 14/15 ignored integration pass |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| Static Analysis (`cargo fmt --check`) | Pass | exit 0 |
| Static Analysis (`cargo clippy -D warnings`) | Pass for new code | 2 pre-existing errors (`db.rs` await-holding-lock, `cleanup.rs` cloned_ref_to_slice_refs) were documented as out-of-scope in the plan |
| Unit Tests | Pass | 197 pass, 0 fail |
| Integration Tests (`--ignored`) | Pass for new code | 14 pass, 1 fail (`field_lock::lock_unlock_roundtrip` — pre-existing Phase D RLS issue on `manifestations`, documented in session memory 3850, unrelated to this change) |
| Build | Pass | |

## Files Changed

| File | Action | Summary |
|---|---|---|
| `backend/src/config.rs` | UPDATE | `operator_contact: Option<String>`, `user_agent()`, 2 new tests |
| `backend/src/services/enrichment/http.rs` | UPDATE | `api_client(&str)` and `cover_client(usize, u64, &str)` take UA |
| `backend/src/services/enrichment/sources/open_library.rs` | UPDATE | URL → `/api/books?bibkeys=ISBN:X&jscmd=data&format=json`; `map_api_books_response(body, bibkey)`; bumped limiter to 3 req/s; rewired wiremock tests; 5 new pure-parser tests |
| `backend/src/services/enrichment/orchestrator.rs` | UPDATE | `api_client(&config.user_agent())` in `run_once` + `fan_out_for_dry_run`; rewrote `mock_openlibrary_isbn` helper to wrap legacy bodies into the new bibkey-keyed shape |
| `backend/src/services/enrichment/queue.rs` | UPDATE | Added `operator_contact: None` to test Config literal |
| `backend/src/services/enrichment/cover_download.rs` | UPDATE | Pass test UA to `cover_client` |
| `backend/src/services/ingestion/orchestrator.rs` | UPDATE | Added `operator_contact: None` to test Config literal |
| `backend/src/test_support.rs` | UPDATE | Added `operator_contact: None` to shared test Config |
| `backend/src/main.rs` | UPDATE | Startup `tracing::warn!` when `operator_contact` is `None` |
| `.env.example` | UPDATE | Documented `TOME_OPERATOR_CONTACT` |

## Deviations from Plan

- **Branch strategy**: Plan said branch from `main` after the enrichment PR
  merges. User chose option 3 (fold into `feat/metadata-enrichment-pipeline`).
  No code impact, but widens that branch's PR scope.
- **Additional Config literals**: Plan listed 5-6 files; finding the new
  `operator_contact` field needed in 4 additional test-support Config literals
  (`test_support.rs`, `queue.rs`, `ingestion/orchestrator.rs`,
  `enrichment/orchestrator.rs`). Mechanical only — each took one extra
  `operator_contact: None` line.
- **Phase D mock shape bridging**: Instead of rewriting each Phase D
  orchestrator test body, the `mock_openlibrary_isbn` helper was extended with
  a `normalise_api_books_entry` translator that lifts legacy
  `publishers: ["Ace"]` strings into `{name: "Ace"}` objects and wraps the
  whole body under the `ISBN:{isbn}` bibkey. Keeps the per-test bodies
  compact and matches the adapter's new consumption shape.

## Issues Encountered

- Initial clippy flagged a `collapsible_if` inside the new
  `normalise_api_books_entry` helper; resolved by merging into a single
  `if let ... && let ... && let ...` chain.
- Clippy's 2 pre-existing errors (`await-holding-lock` in `db.rs:40`,
  `cloned_ref_to_slice_refs` in `cleanup.rs:114`) remain untouched per the
  plan's explicit OOS note.
- `field_lock::lock_unlock_roundtrip` still fails under `--ignored` with
  an RLS violation on `manifestations`. Same failure recorded against Phase D
  (session memory #3850). Unrelated to the OpenLibrary/UA work.

## Tests Written

| Test File | New Tests | Coverage |
|---|---|---|
| `backend/src/config.rs` | `user_agent_with_contact_embeds_identifier`, `user_agent_without_contact_reports_unidentified` | UA formatting both paths |
| `backend/src/services/enrichment/sources/open_library.rs` | `map_api_books_response_happy_emits_full_field_set`, `_missing_key_is_clean_miss`, `_partial_returns_only_present_fields`, `_skips_author_without_name`, `_skips_empty_cover_urls`, `isbn_happy_path_hits_api_books_and_maps_fields`, `isbn_missing_key_is_clean_empty` | Pure parser edge cases + wiremock happy path + 200-with-empty-map miss |

Existing OpenLibrary wiremock tests (`isbn_404_is_clean_empty`,
`isbn_429_maps_to_rate_limited_with_retry_after`,
`isbn_500_maps_to_http_error`) were kept and rewired to match the new URL.

## Acceptance Criteria

- [x] `/api/books?bibkeys=ISBN:X&jscmd=data` URL used for ISBN lookups.
- [x] Authors emitted as `creators` `SourceResult` from OpenLibrary.
- [x] `User-Agent` set on both `api_client` and `cover_client` via `config.user_agent()`.
- [x] `TOME_OPERATOR_CONTACT` documented in `.env.example` and emits startup warning when unset.
- [x] OpenLibrary governor at 3 req/s (was 5 req/min).
- [x] Existing OpenLibrary wiremock tests pass against the new shape.
- [x] Phase D orchestrator integration tests pass with the updated mock helper.
- [x] `cargo fmt --check` clean; `cargo clippy -D warnings` adds no new errors.

## Next Steps

- [ ] Code review via `/code-review` — recommended before PR.
- [ ] Commit on `feat/metadata-enrichment-pipeline`.
- [ ] Open PR for the full enrichment branch (Phase B + C + D + OpenLibrary bulk API).
