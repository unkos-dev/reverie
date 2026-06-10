# Implementation Report

**Plan**: `.claude/PRPs/plans/completed/unk-376-library-openapi-coverage.plan.md`
**Epic**: UNK-376 (docs-as-done Phase 2 — full OpenAPI route coverage)
**Branch**: `feat/unk-376-library-openapi-coverage`
**Date**: 2026-06-10
**Status**: COMPLETE (local bar); integration suite runs in CI

---

## Summary

First coverage cluster of the UNK-376 epic: migrated the `library` route module to
`utoipa_axum::OpenApiRouter` so its four data routes (`GET /api/v1/books`,
`/books/{id}`, `/works/{id}`, `/search`) are documented in `docs/openapi.json`.
Mirrors the `health` pilot — the only module previously on the pattern. Establishes
the full pattern stack (`#[utoipa::path]`, `ToSchema` DTOs, `IntoParams` query
structs, deny-by-default security inheritance, component registration) for the later
mechanical 3–4-module batches.

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning                                                                                  |
| ---------- | --------- | ------ | ------------------------------------------------------------------------------------------ |
| Complexity | MEDIUM    | MEDIUM | As scoped. Two non-obvious gotchas surfaced (below), both caught by local validation.       |
| Confidence | 8/10      | high   | Integration `split_for_parts` move was exactly as analysed; no surprises in the router wiring. |

**Deviations / discoveries during implementation:**

1. **`IntoParams` rendered query params as `in: path`, `required: true`.** utoipa could
   not auto-detect the extractor (the `list` handler wraps it as
   `Result<Query<ListParams>, QueryRejection>`, defeating detection) and defaulted to
   `parameter_in = Path`. Fixed with explicit `#[into_params(parameter_in = Query)]` on
   both `ListParams` and `SearchParams`. Verified via `jq`: all params now `in: query`,
   `required: false`.
2. **`SortMode` produced a dangling `$ref`.** The `?sort=` param emits
   `$ref: #/components/schemas/SortMode`, but utoipa's `routes!` only auto-collects
   **response-body** schemas — not schemas referenced solely by an `IntoParams` field. The
   byte-drift gate passed (consistent dangling ref), but the Starlight docs build failed on
   the unresolved `$ref`. Fixed by registering `SortMode` in `ApiDoc`
   `components(schemas(...))` and adding a guard assertion to `spec_covers_library_routes`
   so the unit test catches it locally (the drift test alone cannot).

`search` was given its own `pub(super) fn router()` (its own `OpenApiRouter` merged by
the parent) so the `routes!` macro resolves the handler in the module that defines it —
cleaner than `routes!(search::search)` across a module boundary.

---

## Tasks Completed

| # | Task | Files | Status |
| - | ---- | ----- | ------ |
| 1 | TDD red-first spec-coverage assertion | `backend/tests/gen_openapi.rs` | ✅ |
| 2 | `ToSchema` on embedded enums + `SortMode` | `models/{enrichment,ingestion,validation}_status.rs`, `routes/cursor.rs` | ✅ |
| 3 | `ToSchema` on library DTOs + `#[schema(ignore)]` created_at | `models/library.rs` | ✅ |
| 4 | `IntoParams` (+ explicit `parameter_in = Query`) | `routes/library/mod.rs`, `search.rs` | ✅ |
| 5 | `ToSchema` on `BookListResponse` + `#[utoipa::path]` ×4 | `routes/library/mod.rs`, `search.rs` | ✅ |
| 6 | `router()` → `OpenApiRouter`; `search` submodule router | `routes/library/mod.rs`, `search.rs` | ✅ |
| 7 | Merge into `pilot_router` + tag; register `SortMode`; remove `lib.rs` dup mount | `openapi.rs`, `lib.rs` | ✅ |
| 8 | Integration safety net | (compile-verified; runs in CI) | ⏭️ CI |
| 9 | Regenerate + commit `docs/openapi.json`; full gate | `docs/openapi.json` | ✅ |

---

## Validation Results

| Check | Result | Details |
| ----- | ------ | ------- |
| `cargo check --tests` (offline) | ✅ | compiles; router type-move did not break the test harness |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | ✅ | clean |
| `cargo fmt --all -- --check` | ✅ | clean |
| `cargo test --test gen_openapi` | ✅ | 4/4 — drift green, `spec_covers_library_routes` + SortMode guard pass |
| Docs build (`cd docs && npm run build`) | ✅ | Starlight resolves all `$ref`s (caught + fixed the SortMode dangling ref) |
| Backend `#[sqlx::test]` integration suite | ⏭️ CI | local provisioning DB intentionally unreachable from the workspace (Incus cutover); CI's postgres:18 service is the authoritative gate |

`jq` spec checks: 6 paths (4 library + 2 health); `/api/v1/books` has no op-level
`security` (inherits `session_cookie`); query params `in: query`; `created_at` absent
from `BookListRow`; `Link` header documented on the list 200; all DTO + `SortMode`
schemas registered.

---

## Security (hard rule 6)

Touches `routes/library` (user data) + the `openapi.rs` security surface.
- All four ops are `CurrentUser`-gated at runtime and inherit the document-level
  deny-by-default `session_cookie` requirement in the spec — none opts out, so the spec
  never misdocuments an authed route as public (OWASP fail-safe; UNK-380 model). Pinned
  by the `spec_covers_library_routes` no-op-level-security assertion.
- Annotations are doc-only: `split_for_parts().0` is the same served router. RLS, the
  `api_csp_layer` (applied at the `api_like` block, unaffected by the move), and the
  `ProblemDetails` error envelope are unchanged.
- No info leak: error responses reference the existing `ProblemDetails` shape;
  `created_at` (internal cursor key) is `#[schema(ignore)]`/`#[serde(skip)]`.
Stands up to security review.

---

## Next Steps

- [ ] Push branch; CI runs the integration suite (authoritative) + docs/drift gates.
- [ ] Bot reviews (Greptile/CodeRabbit) on PR; optional santa-method/adversarial pass.
- [ ] User reviews + merges (agents never merge).
- [ ] Next epic cluster: `series`+`dashboard` (book-adjacent), then larger batches; UNK-379
      grep-guard strictly last; readiness `ProblemDetails` reconcile folds into the
      cluster touching `health`/`ready`.
