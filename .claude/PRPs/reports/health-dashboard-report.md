# Implementation Report

**Plan**: `.claude/PRPs/plans/health-dashboard.plan.md`
**Linear**: UNK-81 (Step 12, v0.1.0 milestone)
**Branch**: `feature/unk-81-step-12-library-health-dashboard`
**Date**: 2026-06-05
**Status**: COMPLETE (browser validation partial — see below)

---

## Summary

Added an admin-only library health dashboard: two read-only Axum JSON endpoints
(`GET /api/dashboard/stats`, `GET /api/dashboard/activity`) that aggregate
library-wide operator metrics over existing tables, plus a React
`/admin/dashboard` page rendering them with existing shadcn/ui primitives. No
schema change, no new write paths. Mirrors the Step 11 library-API + admin-page
patterns (`require_admin()` gate, `acquire_with_rls()` pool path, Zod-at-boundary
client, `useAuthMe()` redirect guard).

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning |
| ---------- | --------- | ------ | --------- |
| Complexity | MEDIUM    | MEDIUM | Matched. Pure aggregation over existing tables; the only surprises were lint/doc gates, not logic. |
| Confidence | HIGH      | HIGH   | Pool/RLS decision verified correct — admin RLS context yields global manifestation visibility (proved by `stats_distinct_works_vs_manifestations` returning non-zero totals on seeded data). |

**Deviations from the literal plan (all plan-sanctioned options, not scope changes):**

- **Query C `ended_at` (bug fixed before it shipped).** The plan's reference SQL
  used bare `MAX(completed_at)`, which ignores NULLs and would report a non-null
  end time for an in-flight batch that already has some completed jobs —
  contradicting the documented "null while in-flight" contract. Implemented as
  `CASE WHEN <any queued/running> THEN NULL ELSE MAX(completed_at) END`. Covered
  by `activity_in_progress_sums_to_total`.
- **Coverage bars use the shadcn `Progress` primitive (Task 11 primary option),
  not a plain `<div>` bar.** The plan's fallback plain-div needs an inline
  `style={{ width }}`, which the repo's `no-restricted-syntax` ESLint rule bans
  outside `components/ui/**`. `Progress` is the canonical primitive, lives in the
  lint-exempt dir, and added no new dependency (the unified `radix-ui` package was
  already present). One-token lint-compliance tweak to the generated
  `progress.tsx` (`String(...)` around a number in a template literal) to satisfy
  the repo's stricter-than-shadcn `restrict-template-expressions` floor.
- **Task 0 / Task 12 (admin nav link): DESCOPED.** `/admin/users` is reached by
  URL only today — no rendered `<Link>`/nav host exists anywhere in the SPA. Per
  the plan's Task 0 scope guard, no nav shell was invented; `/admin/dashboard` is
  URL-only for this MVP, matching the existing admin UX.
- **Task 12b (API docs): WAIVED.** No per-endpoint API-doc convention exists in
  the Starlight `docs/` site (no page references `/api/*`). No new doc pattern
  started, per the plan's waiver branch.

---

## Tasks Completed

| #   | Task | File | Status |
| --- | ---- | ---- | ------ |
| 0   | Nav pre-flight (URL-only → Task 12 descoped) | — | ✅ |
| 1   | Backend integration tests (TDD, red first) | `backend/src/routes/dashboard/tests.rs` | ✅ |
| 2   | Handlers + DTOs | `backend/src/routes/dashboard/mod.rs` | ✅ |
| 3   | Register module + merge router | `backend/src/routes/mod.rs`, `backend/src/lib.rs` | ✅ |
| 4   | `.sqlx` offline cache (5 query files) | `backend/.sqlx/*` | ✅ |
| 5   | Backend tests green + fmt + clippy | — | ✅ |
| 6   | Zod API client | `frontend/src/api/dashboard.ts` | ✅ |
| 7   | `dashboard` query-key family | `frontend/src/lib/query/keys.ts` | ✅ |
| 8   | Page tests (TDD, red first) | `frontend/src/pages/admin/DashboardPage.test.tsx` | ✅ |
| 9   | Dashboard page | `frontend/src/pages/admin/DashboardPage.tsx` | ✅ |
| 10  | Route module + wiring | `frontend/src/routes/dashboard.tsx`, `production.ts`, `main.tsx` | ✅ |
| 11  | `Progress` primitive (shadcn CLI) | `frontend/src/components/ui/progress.tsx` | ✅ |
| 12  | Admin nav link | — | DESCOPED (URL-only) |
| 12b | API documentation decision | — | WAIVED (no convention) |
| 13  | Full backend + frontend suites | — | ✅ |
| 14  | Browser validation | — | PARTIAL (backend down) |

---

## Validation Results

| Check | Result | Details |
| ----- | ------ | ------- |
| Backend tests (dashboard) | ✅ | 7 passed |
| Backend tests (full suite) | ✅ | all pass (see run log) |
| `cargo fmt --check` | ✅ | clean |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | ✅ | 0 warnings |
| `cargo sqlx prepare --check -- --tests` | ✅ | cache current |
| Doc-lint (`-D rustdoc::broken_intra_doc_links`) | ✅ | clean (fixed a broken `//!` method link with explicit path) |
| Frontend lint (`--max-warnings 0`) | ✅ | 0 |
| Frontend type-check (`tsc -b`) | ✅ | clean |
| Frontend tests | ✅ | 281 passed (incl. 5 DashboardPage) |
| Frontend build | ✅ | `dashboard` lazy chunk emitted (12.1 kB) |
| Browser validation | ⚠️ PARTIAL | Route mounts & renders without JS errors; full data + admin-redirect live check blocked by absent backend/OIDC session — deferred to Level 6 manual. |

---

## Tests Written

| Test File | Test Cases |
| --------- | ---------- |
| `backend/src/routes/dashboard/tests.rs` | `stats_distinct_works_vs_manifestations` (2 manifs / 1 work → works==1, clean_non_epub==1), `stats_empty_library_returns_zeros`, `stats_endpoint_rejects_non_admin` (403), `stats_endpoint_requires_auth` (401), `activity_endpoint_admin_lists_batches`, `activity_in_progress_sums_to_total` (in-flight invariant + null ended_at), `activity_limit_is_clamped` (0 + huge) |
| `frontend/src/pages/admin/DashboardPage.test.tsx` | renders heading + book total, non-admin redirect, activity batch row, clean-bucket footnote, empty-library no-divide-by-zero |

---

## Security (CLAUDE.md Hard Rule 6 — touches auth + response shape)

Both endpoints gate with `require_admin()` **before** any DB access — 403 for
non-admins, 401 unauthenticated (via the `CurrentUser` extractor). The only
request parameter (`?limit`) is clamped to `1..=100` before binding to `$1`; no
user input is interpolated into SQL. Responses carry only aggregate counts and
byte totals — no per-user rows, file paths, or PII. RFC 7807 error bodies reuse
the existing `AppError` `IntoResponse` (internal cause never leaked). **Stands up
to security review: yes** — admin-gated, parameterless-except-clamped-limit,
read-only aggregate over non-sensitive counts. Verified by the 403/401 tests.

---

## Next Steps

- [ ] Push branch + open PR with `Closes UNK-81` and the security answer.
- [ ] Level 6 manual validation against a running backend + admin session.
- [ ] Follow-ups (separate, not this PR): admin nav link if/when a nav shell
      lands; UNK-313 (clean bucket on non-EPUB); charting library (needs ADR);
      writeback job history; date-range activity filter.
