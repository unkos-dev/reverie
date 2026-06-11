# Implementation Report

**Plan**: `.claude/PRPs/plans/completed/unk-374-bounded-list-queries.plan.md`
**Source Issue**: UNK-374
**Branch**: `fix/unk-374-bounded-list-queries`
**Date**: 2026-06-11
**Status**: COMPLETE (DB-backed test suite runs in CI per the repo's CI-first rule)

---

## Summary

All five ADR-7 "no unbounded queries" offenders are bounded by construction:
keyset pagination for `GET /api/v1/shelves` (mixed-direction OR-expansion),
the items page of `GET /api/v1/shelves/{id}` (manifestation_id tiebreaker),
and both OPDS navigation feeds (new `NameCursor`, `rel=next` follows the feed
kind per RFC 5005); a defensive `LIMIT 500` on `GET /api/v1/users` (ADR-7
small-ceiling exception). One migration adds four keyset-supporting btree
indexes. The frontend clients walk cursors internally so page components are
untouched.

## Assessment vs Reality

| Metric     | Predicted   | Actual      | Reasoning |
| ---------- | ----------- | ----------- | --------- |
| Complexity | MEDIUM-HIGH | MEDIUM-HIGH + infra detour | Code matched the plan; an unplanned dev-infra outage (below) consumed the largest share of wall-clock |
| Confidence | 8/10        | held        | Every pattern had a working exemplar; no design pivots |

## Deviations from Plan

1. **`.sqlx` regeneration could not run end-to-end.** The dev-LXC sync agent
   (Mutagen) had been dead since ~Jun 7, leaving the LXC's Postgres several
   migrations behind; the sanctioned migrate path compiles the crate *online*
   against that DB, so stale schema ⇒ build failure ⇒ no migration — a
   bootstrap deadlock. Sync was restored, but the DB remains behind. The
   committed cache is the union of main's CI-green entries plus the new-query
   entries emitted by the partial prepare runs (their tables are untouched by
   the stale-window migrations), minus the three orphaned entries of replaced
   queries. CI's `cargo sqlx prepare --check` is the authoritative verdict.
2. **Users-cap overflow test skipped** (per plan Task 5 note): seeding 501
   users per test run is disproportionate; the cap is a static `LIMIT` bound
   reviewed in code.
3. **`ShelfWithItems` model deleted** rather than kept: review finding D2
   moved the envelope route-local, leaving the model orphaned.

## Validation Results

| Check | Result | Details |
| ----- | ------ | ------- |
| `cargo fmt --check` | ✅ | |
| `cargo clippy --workspace --all-targets -- -D warnings` (offline) | ✅ | |
| `SQLX_OFFLINE=true cargo check --tests` (cold) | ✅ | |
| Doc-lint (broken intra-doc links) | ✅ | module-doc links need absolute paths (known gotcha) |
| `REGEN=1 cargo test --test gen_openapi` | ✅ | artifact regenerated |
| Frontend `eslint --max-warnings 0` / `tsc -b` / vitest | ✅ | 302 tests pass (12 in shelves client) |
| `#[sqlx::test]` suite + `prepare --check` | ⏭ CI | CI-first rule |

## Follow-up (infra, outside this repo)

The dev-LXC migrate service should build with `SQLX_OFFLINE=true` — the
committed cache exists precisely so builds don't depend on a current DB, and
an online build deadlocks whenever the dev DB is behind a schema-affecting
commit. Also: nothing monitors the sync agent's liveness; it was down four
days unnoticed.
