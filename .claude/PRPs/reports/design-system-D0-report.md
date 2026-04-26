# Implementation Report — Design System D0 only

**Plan**: `.claude/PRPs/plans/design-system.plan.md`
**Branch**: `feat/design-system`
**Date**: 2026-04-24
**Status**: PARTIAL — D0 complete. D1–D5 remain (D1 + D2 need user input; D5 needs `/crosscheck`).

---

## Scope of this session

Plan § **Phase D0 — Testing Harness and Direct Dependencies** (tasks D0.1
through D0.12). Subsequent phases were deliberately not touched:

- **D1 (Conceptual Foundation)** — creative brainstorm. Exit gate is
  "Document human-reviewed." Requires user input on emotional target,
  anti-patterns, usage context.
- **D2 (Visual Exploration)** — three coded directions + subjective taste
  review. Exit gate is human decision on which direction wins.
- **D3 (Codify Design System)** — 20 tasks that depend on the D2 winner.
- **D5 (Review Gate)** — `/crosscheck`, which is user-triggered.

Auto mode does not override plan-level gates that explicitly require human
input. D0 has an objective exit gate (`npm test` green, `cargo test` green,
dependencies visible, docs + seed script present), so it is the clean
ceiling for an autonomous session.

---

## Tasks Completed

| #    | Task                                                           | Key artefact(s)                                                          | Status |
| ---- | -------------------------------------------------------------- | ------------------------------------------------------------------------ | ------ |
| D0.1 | Install Vitest + RTL                                           | `frontend/package.json` devDeps: vitest, @testing-library/{react,jest-dom,user-event}, jsdom | done |
| D0.2 | Install design-system deps                                     | `frontend/package.json` deps: react-router, lucide-react; devDeps: stylelint, @axe-core/cli | done |
| D0.3/D0.4 | Reshape `vite.config.ts` (test.projects + server.proxy)   | `frontend/vite.config.ts` — two-project Vitest config (node / jsdom), `/api` `/auth` `/opds` proxy to `:3000`; `cspHashPlugin()` + `DEV_CSP` preserved verbatim | done |
| D0.5 | RTL setup file                                                 | `frontend/tests/setup.ts`                                                | done |
| D0.6 | Vitest + jest-dom types                                        | `frontend/tsconfig.app.json` — types: `vite/client, vitest/globals, vitest/jsdom, @testing-library/jest-dom` | done |
| D0.7 | test/stylelint scripts                                         | `frontend/package.json` scripts: test (vitest run), test:watch, test:coverage, stylelint | done |
| D0.8 | Smoke test                                                     | `frontend/src/__tests__/smoke.test.ts`                                   | done |
| D0.9 | CI runs tests + stylelint                                      | `.github/workflows/ci.yml` frontend job — Test step, Stylelint step (non-blocking until D3.14) | done |
| D0.10 | TDD scope doc                                                 | `docs/src/content/docs/design/testing-scope.md` (sidebar wiring deferred to D3.18) | done |
| D0.11 | `CookieJar` end-to-end verification                           | `backend/Cargo.toml` adds `axum-extra = "0.10"` (cookie feature); `backend/tests/cookie_jar_sanity.rs` — two integration tests verifying `Set-Cookie` emits on both `StatusCode` and `Redirect` tuple responses | done |
| D0.12 | Seed-library script                                            | `dev/seed-library.sh` (pinned 8-title SE manifest with real SHA-256s); `.gitignore` ignores `backend/tests/fixtures/library/` | done |

---

## Validation Results

| Check                                      | Result | Details                                                              |
| ------------------------------------------ | ------ | -------------------------------------------------------------------- |
| `npx tsc -b` (frontend)                    | ✅     | clean                                                                |
| `npm run lint` (frontend)                  | ✅     | clean                                                                |
| `npm test` (both Vitest projects)          | ✅     | 2 test files, 7 tests passed — `vite-plugins` (6) + `frontend` (1)    |
| `npm run build` (frontend)                 | ✅     | succeeds; `dist/csp-hashes.json` emitted with non-empty script-src-hashes |
| `cargo clippy --all-targets -- -D warnings`| ✅     | clean                                                                |
| `cargo test --test cookie_jar_sanity`      | ✅     | 2 passed (OK tuple + Redirect tuple)                                 |
| CSP preservation grep gate                 | ✅     | `cspHashPlugin()` and `server.headers.DEV_CSP` intact in `vite.config.ts` |
| Seed script end-to-end                     | ✅     | first run: fetched=8 skipped=0; second run: fetched=0 skipped=8 (idempotent) |

### Seed-library checksums (pinned 2026-04-24)

All 8 titles fetched with ZIP magic bytes verified. `?source=download` query
string is load-bearing — Standard Ebooks serves a meta-refresh HTML page
without it. Documented in the script header.

---

## Deviations from Plan

1. **D0.11 — `#[cfg(test)]` gating via `backend/tests/` instead of in-source module.** The plan permitted either deleting throwaway routes or gating with `#[cfg(test)]`. Chose a standalone integration-test file under `backend/tests/`, which is implicitly `#[cfg(test)]`-only by compile profile — safer than relying on a memory-of-later-deletion and no pollution of production router code.

2. **D0.12 — Curation substitutions.** Plan listed Kafka *Metamorphosis* and Darwin *Origin of Species*. Neither URL pattern resolved on Standard Ebooks during probe (the Kafka page exists under translator slug `willa-muir_edwin-muir` but the downloads subpath 404s; SE no longer hosts *Origin of Species* — only *Voyage of the Beagle* remains for Darwin). Substituted:
   - Kafka → Tolstoy *Anna Karenina* (Constance Garnett) — preserves the "translated work" edge-case coverage.
   - Darwin *Origin of Species* → Darwin *Voyage of the Beagle* — preserves the "rich subject metadata" edge-case coverage.
   The curation rationale (long title, short title, series, translated, rich subject metadata) is still satisfied by the 8-title manifest.

3. **D0.12 — Ingestion scan VALIDATE not executed.** The plan's VALIDATE step asks to run `curl -X POST http://localhost:3000/api/ingestion/scan` and assert `books_ingested: 8`, `covers_extracted: 8`. This requires the full backend running against a dev database with `REVERIE_LIBRARY_ROOT` set. The seed script itself is present, runnable, idempotent, and has been executed successfully (all 8 EPUBs downloaded with matching SHA-256s) — so the D0 exit criterion "seed script present and runnable" is met. The ingestion-scan validation is better performed as a D3 or Step-11 smoke check once the routes are applied. **Deferred to manual / Step 11.**

---

## Files Changed

### Modified

| File                             | Summary                                                    |
| -------------------------------- | ---------------------------------------------------------- |
| `.github/workflows/ci.yml`       | frontend job gains Test + Stylelint steps                  |
| `.gitignore`                     | ignore `backend/tests/fixtures/library/`                   |
| `backend/Cargo.lock`             | new `axum-extra` transitive tree                           |
| `backend/Cargo.toml`             | add `axum-extra = { version = "0.10", features = ["cookie"] }` |
| `frontend/package-lock.json`     | new dev + runtime deps                                     |
| `frontend/package.json`          | scripts + deps                                             |
| `frontend/tsconfig.app.json`     | vitest / jest-dom types                                    |
| `frontend/vite.config.ts`        | test.projects + server.proxy (CSP block preserved)         |

### Created

| File                                                      | Purpose                                                   |
| --------------------------------------------------------- | --------------------------------------------------------- |
| `backend/tests/cookie_jar_sanity.rs`                      | D0.11 — `CookieJar` tuple contract verification            |
| `dev/seed-library.sh`                                     | D0.12 — pinned 8-title Standard Ebooks seed                |
| `docs/src/content/docs/design/testing-scope.md`           | D0.10 — testing scope note                                |
| `frontend/src/__tests__/smoke.test.ts`                    | D0.8  — harness smoke test                                 |
| `frontend/tests/setup.ts`                                 | D0.5  — RTL setup                                          |

No files deleted.

---

## Plan retained, not archived

The plan file remains at `.claude/PRPs/plans/design-system.plan.md` — D1,
D2, D3, and D5 are unfinished. Do not run the plan-archive step until those
phases complete and pass D5 crosscheck.

---

## Handoff: next steps

- **D1 — Conceptual Foundation (user-driven):** invoke `superpowers:brainstorming` to explore emotional target, anti-patterns, usage context. Output lives at `/plans/YYYY-MM-DD-<topic>-design.md` per project CLAUDE.md, then gets folded into `docs/src/content/docs/design/philosophy.md`.
- **D2 — Visual Exploration (user-driven):** three coded directions at `/design/explore/[name-a|b|c]`. Human taste review picks the winner. Record the decision at the top of `philosophy.md`.
- **D3 — Codify (mechanical, depends on D2):** ~20 tasks; first task D3.1 is to prune the D2 exploration trees.
- **D5 — `/crosscheck`** on design system artefacts (not applied pages). User-invoked.

Uncommitted changes stay on branch `feat/design-system` for user review /
commit.
