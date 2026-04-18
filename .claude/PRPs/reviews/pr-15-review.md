# PR Review: #15 — feat(enrichment): metadata enrichment pipeline (blueprint step 7)

**Reviewed**: 2026-04-18 (general code review)
**Re-reviewed**: 2026-04-18 (rust-reviewer)
**Remediated**: 2026-04-18
**Author**: junkovich
**Branch**: feat/metadata-enrichment-pipeline → main
**Decision**: APPROVE after remediation (CI blocker fixed, all medium findings addressed or tracked)

## Summary

Solid implementation of a complex pipeline. The security-critical paths (SSRF guard, Basic-auth timing, migration idempotency) are correct. Two review rounds (general + rust-specific) surfaced one CI blocker, two data-correctness bugs, and a set of cleanup items. All have been addressed in-branch or tracked in Linear UNK-96.

## Findings — disposition

### CRITICAL / HIGH

| ID | Issue | Status | Commit |
|----|-------|--------|--------|
| H1 | `cargo fmt --check` fails on three enrichment files | **Fixed** | `0f6210f` |
| H2 | `run_once` is ~170 lines; refactor into phase functions | **Deferred → UNK-96** | — |

### MEDIUM

| ID | Issue | Status | Commit |
|----|-------|--------|--------|
| M1-old | `get_manifestation_metadata` bypasses `acquire_with_rls` | **Fixed** | `54f9f00` |
| M1-new | `json_as_string` writes arrays/objects as raw JSON into text columns | **Fixed** | `6f90090` |
| M2-new | Silent `unwrap_or(Null)` on cache serialisation failure | **Fixed** | `ed02c9f` |
| M3-new | Unvalidated `%title%` ILIKE wildcard in hardcover adapter | **Fixed** | `ed02c9f` |
| M4-new | `google_books.rs` did not encode `+` (divergence from `open_library.rs`) | **Fixed** | `8d7da9a` |
| M5-new | `existing_pending.clone()` inside nested loop | **Deferred → UNK-96** | — |
| M2-old | `backoff()` dead code in queue.rs | **Fixed** (deleted) | `bf13b5f` |
| M3-old | `clear_field` uses dynamic SQL | **Kept as-is**: all inputs are `&'static str` from exhaustive match arms; no injection risk. The original suggestion to use `sqlx::query!` macros per branch remains valid but low-priority. |

### LOW

| ID | Issue | Status | Commit |
|----|-------|--------|--------|
| L1-old | 13 enrichment modules carry blanket `#![allow(dead_code)]` | **Fixed** (item-level allows + deletions) | `bf13b5f` |
| L2-old | `ssrf_resolver_allows_public_hostname` test passes vacuously in sandboxed CI | **Fixed** (`#[ignore]` with note) | `ed02c9f` |
| L3-old | Google Books API key appended raw to URL | **Fixed** (URL-encoded) | `bf13b5f` |
| L1-new | `pub_date` parse-failure handling diverges between pipeline vs route | **Fixed** (documented intentional divergence) | `ed02c9f` |
| L2-new | Dead `noop` function in orchestrator | **Fixed** (deleted) | `bf13b5f` |

## Post-remediation validation

| Check | Result |
|---|---|
| `cargo fmt --check` | Pass |
| `cargo clippy -- -D warnings` | Pass |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `cargo test` | 201 passed, 0 failed, 46 ignored (was 202/45 — one additional ignored is the SSRF test now gated on outbound DNS) |

## Commit trail

```text
0f6210f  style(enrichment): cargo fmt fixes for CI gate
54f9f00  fix(enrichment): gate get_manifestation_metadata behind acquire_with_rls
6f90090  fix(enrichment): reject non-scalar journal values in apply_field
8d7da9a  refactor(enrichment): share query_encode helper across adapters
bf13b5f  refactor(enrichment): remove blanket dead_code allows + dead backoff()
ed02c9f  fix(enrichment): post-review small fixes
```

## Follow-up tracked in Linear

- **UNK-96** — Refactor `enrichment::orchestrator::run_once` into phase functions; includes `existing_pending.clone()` perf as a sub-task.

## Files reviewed

| File | Change |
|---|---|
| `backend/migrations/20260417000001_add_enrichment_pipeline.up.sql` | Added |
| `backend/migrations/20260417000001_add_enrichment_pipeline.down.sql` | Added |
| `backend/migrations/20260417000002_grant_field_locks_select_ingestion.{up,down}.sql` | Added |
| `backend/src/services/enrichment/http.rs` | Added |
| `backend/src/services/enrichment/cover_download.rs` | Added |
| `backend/src/services/enrichment/value_hash.rs` | Added |
| `backend/src/services/enrichment/confidence.rs` | Added |
| `backend/src/services/enrichment/policy.rs` | Added |
| `backend/src/services/enrichment/lookup_key.rs` | Added |
| `backend/src/services/enrichment/field_lock.rs` | Added |
| `backend/src/services/enrichment/cache.rs` | Added |
| `backend/src/services/enrichment/queue.rs` | Added |
| `backend/src/services/enrichment/dry_run.rs` | Added |
| `backend/src/services/enrichment/orchestrator.rs` | Added |
| `backend/src/services/enrichment/sources/{mod,open_library,google_books,hardcover}.rs` | Added |
| `backend/src/routes/enrichment.rs` | Added |
| `backend/src/routes/metadata.rs` | Modified |
| `backend/src/auth/middleware.rs` | Modified |
| `backend/src/config.rs` | Modified |
| `backend/src/db.rs` | Modified |
| `backend/src/test_support.rs` | Modified |
| `backend/src/models/work.rs` | Modified |
| `backend/src/services/ingestion/orchestrator.rs` | Modified |
| `backend/src/services/metadata/{draft,extractor,inversion}.rs` | Modified |
