# Implementation Report: EPUB Structural Validation and Auto-Repair

## Summary

Implemented a 5-layer pure Rust EPUB validation pipeline (ZIP → container → OPF → XHTML → cover)
with auto-repair, integrated into the ingestion orchestrator between file copy and DB insert.
Files that pass or are repaired proceed normally; files with non-critical issues are marked
`degraded`; irrecoverable files are quarantined.

## Assessment vs Reality

| Metric | Predicted (Plan) | Actual |
|---|---|---|
| Complexity | Large | Large |
| Confidence | High | High |
| Files Changed | 12 new + 3 modified | 10 new + 3 modified |

(Migration files count as 2, epub module files as 7 — plan counted 12 new total which is correct
when split: 2 migrations + 6 epub/* + services/mod.rs = 9 new; orchestrator.rs + Cargo.toml = 2 modified)

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 1 | Add migration files | Complete | `20260415000001_epub_validation.{up,down}.sql` |
| 2 | Add Cargo dependencies | Complete | `encoding_rs`, `image`, `quick-xml`, `zip` |
| 3 | `epub/mod.rs` — core types + entry point | Complete | |
| 4 | `zip_layer.rs` — ZIP integrity | Complete | |
| 5 | `container_layer.rs` — container.xml parse | Complete | |
| 6 | `opf_layer.rs` — OPF parse, spine, accessibility | Complete | |
| 7 | `xhtml_layer.rs` — XHTML encoding + parse | Complete | Deviated — see below |
| 8 | `cover_layer.rs` — cover image decode | Complete | |
| 9 | `repair.rs` — atomic ZIP repackage | Complete | |
| 10 | `services/mod.rs` update | Complete | |
| 11 | Orchestrator integration (Step 4.5) | Complete | |
| 12 | Migration verify | Pending (DB not accessible) | Run `sqlx migrate run` manually |
| 13 | Unit tests | Complete | 14 tests written, all pass |
| 14 | Accessibility metadata end-to-end | Complete | Flows through OPF → report → DB |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| Static Analysis (`cargo check`) | Pass | Zero errors |
| Clippy (`-D warnings`) | Pass | Zero warnings |
| Unit Tests | Pass | 14 tests written, 14 pass |
| Full Test Suite | Pass | 69 tests, 0 failures, 0 regressions |
| Build | Pass | `cargo test` (debug) |
| Integration | N/A | DB tests marked `#[ignore]`, require running postgres |
| Edge Cases | Pass | All checklist items covered by unit tests |

## Files Changed

| File | Action | Notes |
|---|---|---|
| `backend/migrations/20260415000001_epub_validation.up.sql` | CREATED | sqlx:disable-transaction pragma, IF NOT EXISTS guard |
| `backend/migrations/20260415000001_epub_validation.down.sql` | CREATED | Documents enum removal caveat |
| `backend/Cargo.toml` | UPDATED | +4 deps: encoding_rs, image, quick-xml, zip |
| `backend/src/services/epub/mod.rs` | CREATED | All shared types + `validate_and_repair` entry point |
| `backend/src/services/epub/zip_layer.rs` | CREATED | Layer 1: ZIP integrity + 3 unit tests |
| `backend/src/services/epub/container_layer.rs` | CREATED | Layer 2: container.xml + 2 unit tests |
| `backend/src/services/epub/opf_layer.rs` | CREATED | Layer 3: OPF parse + accessibility + 2 unit tests |
| `backend/src/services/epub/xhtml_layer.rs` | CREATED | Layer 4: XHTML encoding + XML parse + 2 unit tests |
| `backend/src/services/epub/cover_layer.rs` | CREATED | Layer 5: cover image decode + 2 unit tests |
| `backend/src/services/epub/repair.rs` | CREATED | Atomic ZIP repackage + 3 unit tests |
| `backend/src/services/mod.rs` | UPDATED | +`pub mod epub;` |
| `backend/src/services/ingestion/orchestrator.rs` | UPDATED | Step 4.5 + updated CTE INSERT |

## Deviations from Plan

1. **`detect_declared_encoding` uses `from_utf8_lossy` not `from_utf8`** — The plan used
   `str::from_utf8(...).ok()?` on the first 200 bytes. When test bytes contained a non-UTF-8
   character within those 200 bytes, the scan returned `None` and the encoding wasn't detected.
   Fixed with `String::from_utf8_lossy` so non-ASCII content bytes don't poison the ASCII-range
   XML declaration scan. This is a correctness improvement.

2. **Clippy collapsible_if fixes in opf_layer.rs and repair.rs** — The plan's code had nested
   `if` blocks where clippy (edition 2024) required `let ... && let ...` chains. Applied the
   lint-correct form throughout. No semantic change.

3. **`map_or(false, ...)` → `is_some_and(...)` in repair.rs** — Clippy lint fix.

## Issues Encountered

- `detect_declared_encoding` returned `None` for any bytes where non-UTF-8 content appeared
  within the first 200 bytes, even if the XML declaration was pure ASCII. Root cause: `str::from_utf8`
  rejects the entire slice if any byte is invalid. Fix: `from_utf8_lossy`. Found via failing test.

## Tests Written

| Test File | Tests | Coverage |
|---|---|---|
| `zip_layer.rs` | 3 | path traversal, corrupt ZIP, clean ZIP |
| `container_layer.rs` | 2 | missing container (repaired), valid container |
| `opf_layer.rs` | 2 | broken spine ref, EPUB3 accessibility metadata |
| `xhtml_layer.rs` | 2 | Latin-1 declared (repaired), non-UTF-8 no decl (degraded) |
| `cover_layer.rs` | 2 | missing cover file, undecodable cover |
| `repair.rs` | 3 | container.xml regenerated, mimetype first+stored, OPF spine rewrite |

## Next Steps

- [ ] Run `sqlx migrate run` against dev DB to apply migration
- [ ] Code review via `/code-review`
- [ ] Create PR via `/prp-pr`
