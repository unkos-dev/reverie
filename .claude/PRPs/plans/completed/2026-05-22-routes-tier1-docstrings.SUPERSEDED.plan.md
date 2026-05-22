# Feature: Tier 1 docstring backfill for `backend/src/routes/` (Phase 3)

## Summary

Phase 3 graduation of `backend/src/routes/` under the tiered comment policy
(`adr/2026-05-08-tiered-comment-policy.md`). One PR carrying Tier 1 docstrings
on every `pub` item in `backend/src/routes/**` (minus `tests.rs` Tier 4
files), with Tier 2 threat annotations on auth- and credential-handling
surfaces (`routes/auth.rs`, `routes/tokens.rs`, and every OPDS file sitting
behind Basic-only auth). Sibling PRs already shipped for `auth/` (#189),
`security/` (#190), `models/` (#191); this is the next module in the ADR's
audience-criticality order (auth → security → models → **routes** →
services).

## Problem Statement

`backend/src/routes/**` `pub` items currently carry minimal docstrings — most
are one-line purpose statements that satisfy `#![deny(missing_docs)]` but do
not satisfy Tier 1's authoring bar (purpose **plus** invariants **plus**
non-obvious WHY **plus** `# Errors` for `Result`-returning fns). Per-file
audit confirms:

- `routes/auth.rs::router` — one-line, no invariants or threat statement
  despite holding the OIDC flow surface.
- `routes/tokens.rs::router` — one-line, no statement of the
  plaintext-returned-exactly-once invariant.
- `routes/opds/mod.rs::router_enabled` / `covers_router` — short docstrings,
  but no Tier 2 Basic-only auth boundary statement.
- `routes/opds/feed.rs` — already richly documented (this file is the shape
  to _match_, not extend).
- Several smaller files (`enrichment.rs`, `ingestion.rs`, `metadata.rs`,
  `spa.rs`, `health.rs`, `opds/{download, root, opensearch, shelves}.rs`)
  carry single-line docstrings without invariants or `# Errors` sections.

The `missing_docs` lint is silent on routes (every `pub` item already has a
docstring of some shape), so the gap is not lint-detectable — it is a
quality/authoring gap that the Tier 1 policy and Phase 4 lint
(`missing_errors_doc` re-enable) jointly close.

## Solution Statement

Per-file authoring pass under the existing `#![deny(missing_docs)]`
floor. No attribute moves, no structural change. Every `pub` item gets a
`///` block carrying:

1. **Purpose** in one sentence.
2. **Invariants** — what callers must hold, what this function guarantees.
3. **Non-obvious WHY** where one exists.
4. **`# Errors`** section for every `pub fn` returning `Result<…>` (Phase 4
   re-enables `clippy::missing_errors_doc`; authoring now makes Phase 4 a
   config flip).
5. **Tier 2 threat annotation** on auth-/credential-/Basic-auth-handling
   surfaces.

Module tops carry `//!` with module purpose plus invariants. Existing
leading comments are preserved verbatim — new docstrings placed _above_,
never in place of or below.

## Metadata

| Field            | Value                                                              |
| ---------------- | ------------------------------------------------------------------ |
| Type             | DOCS (Tier 1 backfill)                                             |
| Complexity       | LOW (per-item authoring; no behaviour change)                      |
| Systems Affected | `backend/src/routes/**` (lib crate only, no binary or test impact) |
| Dependencies     | None — pure docstring text                                         |
| Estimated Tasks  | 19 (one per non-tests file)                                        |
| PR shape         | One PR, branch `docs/unk-XXX-routes-tier1-docstrings`              |

## Inventory (pub-item counts, from `rg -c '^pub ' src/routes/`)

| File                        | pub items | Tier | Notes                                                                              |
| --------------------------- | --------- | ---- | ---------------------------------------------------------------------------------- |
| `routes/mod.rs`             | 8         | T1   | Already has `//!` + per-`pub mod` `///`; verify Tier 1 substance, augment if thin. |
| `routes/auth.rs`            | 2         | T2   | OIDC + session + theme-cookie surface. Threat statement + ADR xref.                |
| `routes/tokens.rs`          | 1         | T2   | Device-token issue/list/revoke. Plaintext-once invariant.                          |
| `routes/health.rs`          | 1         | T1   | Liveness + readiness. Already has Tier 1 substance — verify only.                  |
| `routes/ingestion.rs`       | 1         | T1   | Library-scan trigger.                                                              |
| `routes/enrichment.rs`      | 1         | T1   | Metadata-enrichment trigger.                                                       |
| `routes/metadata.rs`        | 1         | T1   | Metadata-version review.                                                           |
| `routes/spa.rs`             | 1         | T1   | SPA asset router (conditional).                                                    |
| `routes/opds/mod.rs`        | 12        | T2   | Basic-only auth boundary; URL-scoped pairing model.                                |
| `routes/opds/feed.rs`       | 24        | T1   | **Already richly documented; pattern source for other files.** Verify only.        |
| `routes/opds/cursor.rs`     | 2         | T1   | Opaque cursor encoding for pagination.                                             |
| `routes/opds/scope.rs`      | 3         | T1   | URL scope discriminator (`/library` vs `/shelves/{id}`).                           |
| `routes/opds/library.rs`    | 3         | T2   | Library acquisition feed (behind Basic-only).                                      |
| `routes/opds/shelves.rs`    | 1         | T2   | Shelf-scoped acquisition feed.                                                     |
| `routes/opds/root.rs`       | 1         | T2   | Root navigation feed.                                                              |
| `routes/opds/download.rs`   | 1         | T2   | EPUB acquisition (file delivery; AuthZ at scope check).                            |
| `routes/opds/covers.rs`     | 2         | T2   | Dual-mount cover (OPDS + cookie-or-Basic API).                                     |
| `routes/opds/xml.rs`        | 1         | T1   | XML text sanitiser. Pure helper; document the XML 1.0 invalid-char policy.         |
| `routes/opds/opensearch.rs` | 1         | T2   | OpenSearch descriptor (search surface).                                            |
| **Total**                   | **66**    |      | (brief said ~68; 67 with `routes/mod.rs` module-top counted)                       |
| `routes/opds/tests.rs`      | —         | T4   | **Excluded** — Tier 4, no docstrings on tests.                                     |

---

## Mandatory Reading

| Priority | File                                              | Lines | Why Read This                                                   |
| -------- | ------------------------------------------------- | ----- | --------------------------------------------------------------- |
| P0       | `adr/2026-05-08-tiered-comment-policy.md`         | all   | Tier 1 vs Tier 2 vs Tier 4; anti-patterns; threat format.       |
| P0       | `backend/src/routes/opds/feed.rs`                 | 1–135 | Pattern source for Tier 1 (purpose + invariants + WHY).         |
| P0       | `backend/src/auth/middleware.rs`                  | all   | Tier 2 threat-annotation shape (sibling PR #189).               |
| P1       | `backend/src/auth/token.rs`                       | all   | Tier 2 threat-annotation shape (constant-time / hash).          |
| P1       | `backend/src/lib.rs` lines 1–14, 48–86, 170–205   | —     | Module-top `//!` shape; `# Errors` shape on `pub async fn run`. |
| P1       | `backend/CLAUDE.md` § Comment Policy + Rust Rules | —     | Project rules; `// SAFETY:` precedent.                          |
| P2       | `CLAUDE.md` (repo root) § Comment Policy (Tiered) | —     | Tiered policy summary in author-visible form.                   |

External research: none. Pure docstring authoring; no library / framework
docs needed.

---

## Patterns to Mirror

### Module-top `//!` shape — sourced from `routes/opds/feed.rs:1–27`

```rust
//! OPDS 1.2 Atom XML feed builder.
//!
//! Pure, stateless helper — no DB access, no I/O. Callers build an
//! [`AcquisitionEntry`] per row and feed it through [`FeedBuilder`].
//! Everything a client sees that originates from user data flows through
//! [`super::xml::sanitise_xml_text`] first, and through quick-xml's
//! `BytesText::new` / `push_attribute` auto-escaping on write.
//!
//! Namespaces: OPDS 1.2 uses the default (unprefixed) namespace for Atom
//! elements. Only `opds:`, `dc:`, and `opensearch:` are explicitly prefixed.
//! Do NOT declare `xmlns:atom` — treat Atom as the default.
```

- One-line purpose, then invariants and load-bearing constraints. Module
  tops on Tier 2 files (`routes/auth.rs`, `routes/tokens.rs`, `routes/opds/mod.rs`)
  add a one-line threat-model statement near the top.

### Tier 1 `///` on a `pub fn` returning `Result<…>` — sourced from `lib.rs:170–205`

```rust
/// Boot and run the Reverie API server until shutdown.
///
/// Loads configuration from the environment, finalises CSP headers, opens
/// the primary and ingestion DB pools, initialises the OIDC client, builds
/// the router, spawns the ingestion watcher, the enrichment queue, and the
/// writeback worker […].
///
/// Caller is responsible for installing a tokio runtime — typically by
/// being invoked from a `#[tokio::main]` `async fn main` in the binary
/// crate. Failures during startup return an error rather than panicking […].
///
/// # Errors
///
/// Returns an error when:
/// - configuration cannot be loaded from the environment […];
/// - the API or HTML CSP string fails to parse […];
/// - […]
```

### Tier 1 `///` on a `pub struct` with fields — sourced from `routes/opds/feed.rs:99–118`

```rust
/// One book row, ready for emission as an acquisition `<entry>`.
#[derive(Debug, Clone)]
pub struct AcquisitionEntry {
    /// Manifestation id; embedded in entry id and acquisition / cover URLs.
    pub manifestation_id: Uuid,
    /// Work title rendered as `<title>`.
    pub work_title: String,
    // …
}
```

### Tier 2 threat annotation — sourced from PR #189 (`auth/middleware.rs::verify_basic`)

```rust
/// Verify a Basic-Authorization header against the device-token store.
///
/// THREAT: timing side-channel on the token-lookup loop. The full
/// iteration (no short-circuit on first miss) closes the
/// token-position leak; constant-time hash comparison is delegated
/// to [`super::token::verify_device_token`]. See
/// `adr/2026-05-08-tiered-comment-policy.md` § Tier 2 for the policy.
///
/// # Errors
/// Returns `AuthError::InvalidCredentials` when no row's hash matches.
```

### Anti-patterns (REFUSE — skip docstring rather than commit any of these)

- Pure signature restatement: `/// Returns the router` on `pub fn router() -> Router<…>`.
- Generic boilerplate: `/// @param state The state parameter`.
- Clipping existing leading comments — new `///` goes **above** any
  existing `//` block; never replaces it. PR #178's `hmr-config.ts` is the
  canonical negative example.
- Restating axum routing in prose: `/// Mounts GET /foo and POST /bar at
…`. The signature shows that; the docstring belongs at the WHY level.

---

## Files to Change

All under `backend/src/routes/`. No new files; no file deletions; no
test changes; no Cargo.toml changes.

| File                            | Action                                            |
| ------------------------------- | ------------------------------------------------- |
| `src/routes/mod.rs`             | UPDATE — verify Tier 1 substance, augment if thin |
| `src/routes/auth.rs`            | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/tokens.rs`          | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/health.rs`          | UPDATE — Tier 1 (verify-only likely)              |
| `src/routes/ingestion.rs`       | UPDATE — Tier 1                                   |
| `src/routes/enrichment.rs`      | UPDATE — Tier 1                                   |
| `src/routes/metadata.rs`        | UPDATE — Tier 1                                   |
| `src/routes/spa.rs`             | UPDATE — Tier 1                                   |
| `src/routes/opds/mod.rs`        | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/feed.rs`       | VERIFY — already at Tier 1; no edits expected     |
| `src/routes/opds/cursor.rs`     | UPDATE — Tier 1                                   |
| `src/routes/opds/scope.rs`      | UPDATE — Tier 1                                   |
| `src/routes/opds/library.rs`    | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/shelves.rs`    | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/root.rs`       | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/download.rs`   | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/covers.rs`     | UPDATE — Tier 1 + Tier 2                          |
| `src/routes/opds/xml.rs`        | UPDATE — Tier 1                                   |
| `src/routes/opds/opensearch.rs` | UPDATE — Tier 1 + Tier 2                          |

---

## NOT Building (Scope Limits)

- **Phase 4 — re-enabling `clippy::missing_errors_doc` on `Cargo.toml`.**
  Authoring `# Errors` sections now makes Phase 4 a config flip; the flip
  itself is the next PR after every Phase-3 module ships (routes,
  services, plus the smaller config/db/error/state graduations).
- **`services/` module backfill.** Next Phase-3 ticket after this PR.
- **Test docstrings.** Tier 4: no docstrings on `#[test]` /
  `#[sqlx::test]`. `routes/opds/tests.rs` is excluded.
- **ADR amendment.** Only triggered if authoring surfaces something that
  contradicts the Tier 1 / Tier 2 specs.
- **Touching non-routes files.** Even single-line drive-by improvements
  elsewhere block under "match existing patterns; surgical changes."
- **Behaviour changes / refactors.** Zero source-of-truth changes to
  routing, handler logic, or function signatures. Docstring-only diff.
- **Removing pre-existing leading comments** — preserved verbatim.

---

## Step-by-Step Tasks

Execute in order. Each task is one file. After every file, run the
**per-file validation** block at the end of this section. The full
validation suite runs once at the end before commit.

### Task 0 — Branch + Linear setup

- **ACTION**: Branch from `main`. Branch name: `docs/unk-XXX-routes-tier1-docstrings`
  (substitute the actual UNK after step 0b).
- **ACTION 0a**: `git fetch origin && git switch -c docs/unk-XXX-routes-tier1-docstrings origin/main`.
- **ACTION 0b**: Search Linear for the next unassigned UNK ticket in the
  Phase-3 module-backfill sequence (siblings: UNK-197 auth, UNK-198
  security, UNK-199 models). If none exists, create one — parent UNK-190,
  title "Phase 3: routes module Tier 1 docstrings", labels matching
  siblings.
- **ACTION 0c**: Rename the branch to the actual UNK (e.g.
  `docs/unk-200-routes-tier1-docstrings`) and update the branch name
  everywhere it appears below.

### Tasks 1–19 — One per file

For each file in the Files-to-Change table, in this order:

1. `mod.rs` (top-level routes barrel)
2. `health.rs` (smallest verify-only)
3. `spa.rs`
4. `ingestion.rs`
5. `enrichment.rs`
6. `metadata.rs`
7. `tokens.rs` (T2)
8. `auth.rs` (T2)
9. `opds/mod.rs` (T2; OPDS barrel + Basic-only boundary)
10. `opds/xml.rs`
11. `opds/cursor.rs`
12. `opds/scope.rs`
13. `opds/feed.rs` (verify only)
14. `opds/root.rs` (T2)
15. `opds/opensearch.rs` (T2)
16. `opds/library.rs` (T2)
17. `opds/shelves.rs` (T2)
18. `opds/download.rs` (T2)
19. `opds/covers.rs` (T2)

**For every file:**

- **ACTION**: For every `pub fn`, `pub struct`, `pub enum`, `pub trait`,
  `pub const`, `pub mod`, write a `///` block following the patterns
  above.
- **ACTION**: For the module top, write or extend `//!` with purpose +
  invariants + (for T2) one-line threat statement + ADR cross-reference.
- **ACTION**: Every `pub fn` returning `Result<…>` (including `async fn`
  handlers returning `Result<impl IntoResponse, AppError>`) gets a
  `# Errors` section enumerating the variants and trigger conditions.
- **ACTION**: For T2 files, add `// THREAT:` annotations inline at any
  non-obvious mitigation point in the handler body that doesn't already
  carry one.
- **MIRROR**: `routes/opds/feed.rs:1–135` for shape; PR #189
  `auth/middleware.rs` for T2 shape.
- **NEVER**: Remove, clip, or replace any existing `//` comment. New `///`
  goes above existing leading comments, not over them.
- **VALIDATE (per-file)**: `cargo check --lib --locked` — must build clean
  (proves docstring syntax is valid and no accidental cfg/use breakage).

### Task 20 — Full validation suite

Run from `backend/`, in order. **All four must pass before commit:**

```bash
cargo fmt --check
cargo build --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Then run the pre-push hook directly:

```bash
sh .husky/pre-push
```

Then re-verify the lint floor matches Phase-3 expectations:

```bash
cargo rustc --lib -- -W missing_docs 2>&1 | grep -E '^warning|--> src/routes' | wc -l
# Expected: 0

cargo clippy --lib -- -W clippy::missing_errors_doc 2>&1 | grep -E '^warning|--> src/routes' | wc -l
# Expected: 0  (current baseline outside routes is 4 warnings — all in services/)
```

### Task 21 — Commit

- **Conventional Commits**: `docs(routes): backfill Tier 1+2 docstrings (UNK-XXX)`.
- Single commit (or 2-3 if logical groupings emerge — Tier 1 batch, Tier 2
  batch).
- No `--no-verify`.

### Task 22 — PR

- Push branch; open PR against `main`.
- PR title: `docs(routes): backfill Tier 1+2 docstrings (UNK-XXX)`.
- PR body must include:
  - **File-by-file pub count (before)** — copy from the inventory table.
  - **Tier-classification summary** — which files received Tier 1 only vs
    Tier 1+2.
  - **Confirmation** that `cargo rustc --lib -- -W missing_docs` and
    `cargo clippy --lib -- -W clippy::missing_errors_doc` both still
    return zero warnings on `src/routes/**` (Phase 4 explicitly out of
    scope).
  - **Test plan** checklist mirroring sibling PR #189's shape.
  - **Follow-ups** — Phase 3c `services/`, then Phase 4 lint re-enable.
- **NEVER** include "Generated with Claude Code" attribution in PR body
  (per global feedback memory).
- Comment on UNK-190 with the state-change summary when PR opens.
- **STOP** — agents do not merge; hand off at "PR green and ready for review."

---

## Validation Commands

### Level 1 — Static analysis (fmt + clippy floor)

```bash
cd backend
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

**EXPECT**: Exit 0, no warnings.

### Level 2 — Build + tests

```bash
cd backend
cargo build --workspace --locked
cargo test --workspace --locked
```

**EXPECT**: All tests pass (docstring-only diff should not change behaviour).

### Level 3 — Doc-lint ratchet

```bash
cd backend
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps -p reverie-api
```

**EXPECT**: Builds clean. Three known pre-existing `private_intra_doc_links`
warnings on `security/` and `services/enrichment/http.rs` are
**out of scope** (sibling PR #189 documented this carve-out).

### Level 4 — Pre-push hook

```bash
cd /home/coder/reverie
sh .husky/pre-push
```

**EXPECT**: Exit 0.

### Level 5 — Manual smoke

- Browse `cargo doc --no-deps -p reverie-api --open` and visually confirm
  the rendered `reverie_api::routes` module page reads as a self-contained
  library reference.

---

## Acceptance Criteria

- [ ] Every `pub` item in `backend/src/routes/**` (minus `tests.rs`)
      carries a Tier 1 `///` block: purpose + invariants + non-obvious WHY
      where applicable.
- [ ] Every `pub fn` returning `Result<…>` in scope carries a `# Errors`
      section enumerating variants and triggers.
- [ ] Every Tier 2 file (`auth.rs`, `tokens.rs`, every OPDS file)
      carries a one-line threat-model statement near the top of its
      module `//!` plus inline `// THREAT:` annotations at non-obvious
      mitigations.
- [ ] Tier 2 docstrings cross-reference relevant ADRs by relative path
      (`adr/2026-05-08-tiered-comment-policy.md` for the policy itself;
      `adr/2026-05-08-tower-sessions-sqlx-store.md` etc. where session
      decisions apply).
- [ ] No existing `//` leading comments were removed, clipped, or
      replaced. (Manual diff review check.)
- [ ] No anti-patterns committed: zero pure signature restatements, zero
      generic `@param` boilerplate.
- [ ] `cargo fmt --check` clean.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` clean.
- [ ] `cargo test --workspace --locked` green.
- [ ] `sh .husky/pre-push` exits 0.
- [ ] `routes/**` contributes zero `missing_docs` warnings and zero
      `missing_errors_doc` warnings (baseline outside scope: 4
      `missing_errors_doc` warnings in `services/`).
- [ ] PR body includes file-by-file pub count, Tier classification, and
      Phase-4-out-of-scope confirmation.
- [ ] Linear UNK-190 receives a comment with the state-change summary
      when PR opens.

---

## Completion Checklist

- [ ] Task 0 — Linear ticket secured + branch created
- [ ] Tasks 1–19 — Per-file authoring (per-file `cargo check` clean)
- [ ] Task 20 — Full validation suite green (fmt + build + clippy + test + husky pre-push)
- [ ] Doc-lint ratchet check confirms zero `missing_docs` /
      `missing_errors_doc` warnings on `routes/**`
- [ ] Manual `cargo doc` page review
- [ ] Task 21 — Conventional commit
- [ ] Task 22 — PR opened, body complete, Linear updated
- [ ] **HAND-OFF**: PR green and ready for human review (no agent merge).

---

## Risks and Mitigations

| Risk                                                                                                                             | Likelihood | Impact | Mitigation                                                                                                                                                                                               |
| -------------------------------------------------------------------------------------------------------------------------------- | ---------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Authoring drift: docstring describes behaviour the code doesn't actually have.                                                   | MED        | MED    | Read the function body before writing the docstring; cross-check invariants against the existing tests under `tests.rs`. If unclear, mark item as "owner clarification needed" and halt — do not invent. |
| Clipping a pre-existing WHY-comment (PR #178 anti-pattern).                                                                      | LOW        | HIGH   | Every per-file edit checked by manual diff for `-//` lines on the original side. If any leading `//` block exists on a `pub` item, the new `///` goes **above** that block, never in place of.           |
| `# Errors` section enumerates the wrong variants because the handler's `Result` type is type-erased through `impl IntoResponse`. | MED        | LOW    | Trace `?`-paths and `map_err` sites; document the actual `AppError` variants reachable, not the wire status codes. PR #189 `auth/oidc.rs` is the precedent.                                              |
| Adding a stray `use` or `mod` reorder while editing — escapes scope and risks behaviour drift.                                   | LOW        | MED    | Per-file `cargo check --lib --locked` after every file. Format with `cargo fmt` once at the end; do not reorder imports manually.                                                                        |
| `cargo doc` exposes a private-intra-doc-link warning we accidentally introduce.                                                  | LOW        | LOW    | After per-file edit, if any `[Type]` link is added, check it resolves at the visibility level the link is emitted from (use `[`crate::path::Type`]` rather than `[Type]` for cross-module refs).         |
| Single PR is too large for review (the brief expected ~67 items across 19 files).                                                | MED        | LOW    | Commit in 2–3 logical groups (top-level + handlers, then OPDS sub-tree T1, then OPDS sub-tree T2) so the PR is reviewable in batches. The PR itself remains one — per-module shape per ADR Phase 3.      |
| Halt-and-surface case fires repeatedly (>1 file with unclear invariants).                                                        | LOW        | MED    | Per the escape hatch below, commit partial state, push, file Linear sub-issues parented to the docstring ticket, comment on UNK-190, end loop. Do not iterate further inside one Ralph cycle.            |

---

## Escape Hatch (Ralph-loop bounded N+3)

Halt and surface (do not iterate further) if any of:

- A `pub` item's purpose is unclear from code reading + git blame + ADR
  cross-reference. Action: leave a `// TODO(UNK-XXX-sub): owner
clarification needed` _above_ (never replacing) any existing leading
  comment, do NOT author a speculative docstring (would trip
  `missing_docs` once we ship). Commit smaller scope, file Linear
  sub-issue.
- Ralph iteration count exceeds **N+3 = 22** where N = 19 files in scope.
- `cargo test` reveals docstring-induced behaviour drift (vanishingly
  unlikely on doc-only changes — but a real signal if it happens; halt
  and inspect).

**On halt:** commit partial state on feature branch, push, file Linear
sub-issue with the empirical blocker, comment on UNK-190 with the state,
end the loop.

---

## Notes

- `routes/opds/feed.rs` is the **shape source** — every other file should
  match its level of detail. The file ships verify-only.
- The ADR's original Phase-2 design (per-module `#![allow(missing_docs)]`
  shields + graduate by removing the allow) was abandoned in execution:
  `lib.rs:15` carries `#![deny(missing_docs)]` already, no per-module
  shields exist, prior modules backfilled under the deny directly.
  **Graduation = pure docstring authoring + maintaining the deny green.
  No attribute moves in this PR.**
- `clippy::missing_errors_doc` stays `allow` in `Cargo.toml` — Phase 4
  re-enables it. Authoring `# Errors` sections now makes Phase 4 a free
  config flip.
- `routes/opds/tests.rs` is **Tier 4**: no docstrings on `#[test]` /
  `#[sqlx::test]` functions. The file is excluded from this pass.
- Sibling PR shape to mirror: **PR #189 (auth)** — same Tier 1+2 mix,
  threat-annotation tabular summary in the PR body, follow-ups listed
  explicitly. Read its body before writing this PR's body.
- Per global feedback: **never include "Generated with Claude Code"
  attribution in the PR body**.
- Per global feedback: **`/loop`-based Ralph execution is bounded by the
  escape hatch above**; one pivot maximum per session, commit-durable
  before any pivot, file blocker if pivot count reaches 2.
