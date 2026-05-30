---
status: accepted
date: 2026-05-08
decision-makers: john
---

# Tiered comment policy for an OSS-released codebase

## Context and Problem Statement

Global Claude Code instructions (`~/.claude/CLAUDE.md`) ratify
"default to no comments — only add one when the WHY is non-obvious"
as the cross-project default. The rule is well-tuned for a solo-dev
or shared-team context where the readers are the author and a
small group with shared conversation history; under those
conditions, comments rot fast and self-explanatory naming carries
most of the load.

Reverie's audience profile is different. The project is published
open-source under a self-hosting positioning: code is read by
external contributors, security auditors evaluating whether to
trust the codebase on their own infrastructure, and operators
inspecting deployments before installing. None of those readers
share conversation history with the maintainer. Their cold-read
needs are explicit:

- External contributors orienting cold need to know what each
  module is _for_ before they can change it safely. Self-evident
  naming carries less weight when the reader has zero project
  context.
- Security auditors look up security-critical code without project
  context. They need explicit threat-model statements at the
  boundary, not implicit ones reconstructed from naming.
- `cargo doc` consumers (a class that includes some auditors and
  contributors) read the rendered library reference. Empty
  docstrings on `pub` items render as a lazy library shape — a
  trust signal in the wrong direction.

The project has hit two concrete signals that the global default
needs amendment:

1. **CodeRabbit `finishing_touches.docstrings` has been clicked at
   least once on a merged PR.** PR #178 commit `034e837` added
   JSDoc to `frontend/vite-plugins/hmr-config.ts`; the bot also
   clipped an existing WHY-comment mid-sentence. Maintainer
   post-fix preserved most of the information but in a different
   shape. Net: docstring-generation is happening already, just
   without a policy or quality bar.
2. **Greptile's first dependency-governance catch (UNK-155 row
   #17) explicitly cited that the version-pin rationale "lives in
   the PR body and inline `Cargo.toml` comment, neither of which
   is a durable decision record".** The implicit observation is
   that documentation surfaces matter for stranger-readers — the
   PR body is invisible to anyone reading the merged tree. The
   same observation applies to comment policy: documentation
   visible at the call site is a different surface than
   documentation buried in PR review history.

The status quo (one rule for all comment surfaces) loses the
benefits of explicit documentation on the surfaces where readers
need it most, in service of a rule that was tuned for a different
audience.

## Decision

Adopt a tiered comment policy. The original "default to no
comments" rule is preserved for internal items; explicit-
documentation expectations apply to public API and security-
critical code.

### Tier 1 — Public API (`pub` items at module boundaries)

Every `pub fn`, `pub struct`, `pub enum`, `pub trait`, and `pub
const` exposed at a module boundary carries a `///` Rust doc
comment (or JSDoc on TypeScript exports). Module tops carry
`//!` (Rust) or a file-header docblock (TypeScript) stating
purpose, invariants, and load-bearing constraints.

Required content:

- **Purpose** in one sentence. What this is for.
- **Invariants**. What must hold true for callers; what this
  function guarantees.
- **Non-obvious WHY** where applicable. The constraint, decision,
  or threat-model context that motivated the shape.
- **`# Errors`** section for `pub fn` returning `Result<…>` —
  enumerate variants and trigger conditions
  (`clippy::missing_errors_doc` enforces this for any pub fn
  returning Result; already active per the strict-lint policy
  ADR).
- **`# Panics`** section for any `pub fn` that may panic
  (`clippy::missing_panics_doc`).
- **`# Safety`** section for `pub unsafe fn`
  (`clippy::missing_safety_doc`); aligns with the project
  `// SAFETY:` rule in `backend/CLAUDE.md`.

Anti-patterns:

- **Pure signature restatement.** "Returns the user by id" on
  `pub fn user_by_id(id: UserId) -> Option<User>` adds zero
  value. Prefer no docstring to a signature-restating one.
- **Clipping or replacing existing leading comments.** If a
  function or block already carries a leading WHY-comment block,
  the new docstring is placed _above_ the existing block, not in
  it, not below it, not in place of part of it. Existing comments
  preserved verbatim.
- **Generic boilerplate.** "@param x The x parameter" / "/// Constructor"
  is noise.

### Tier 2 — Security-critical code

Code under `backend/src/auth/`, `backend/src/security/`, and any
function handling credentials, sessions, OIDC flow, role
assertions, RLS context, secret material, or response-header
policy carries explicit threat-model annotations beyond the
standard Tier 1 docstring.

Patterns:

- **`// THREAT:`** comments inline within function bodies for
  non-obvious mitigations. Format: state the attack vector being
  closed, the pre-existing protection (if any), and the additional
  invariant this code adds.
- **One-line threat statement near the top of Tier 1 docstrings**
  on security boundary functions. Example: "Constant-time
  comparison; non-constant-time would expose token prefix via
  timing side-channel."
- **Reference relevant ADRs** by relative path
  (`adr/2026-05-08-tower-sessions-sqlx-store.md` etc.) when the
  decision providing context lives in an ADR.

### Tier 3 — Internal non-public items

Private fns, private structs, private modules: no docstring
required. The original "default to no comments" rule is preserved
in full for this tier — only add a comment when the WHY is
non-obvious, when there's a hidden constraint, or when the code
would surprise a future reader.

### Tier 4 — Tests and test support

`#[test]` / `#[sqlx::test]` / Vitest / Playwright test functions
do not carry docstrings. The test name _is_ the spec; a docstring
that restates it is noise.

`test_support/` modules carry `//!` module-top docs where the
helper's purpose is non-obvious; helper functions stay bare
unless they encode a WHY future readers would not infer.

### Enforcement layering

Phased rollout — UNK-190 tracks the phases:

0. **Split `backend/` into `lib.rs` + thin `main.rs`** before any
   doc-lint work. `missing_docs` (and clippy `missing_errors_doc`
   etc.) only fire on items reachable from outside the crate. The
   current shape is a bin-only crate — every `pub fn` inside
   modules is crate-private to the lint, and the lint is silent.
   Verified empirically: `cargo rustc --bin reverie-api -- -W missing_docs`
   returns 2 warnings (both at `main.rs` root);
   `cargo clippy -- -W clippy::missing_errors_doc` returns 0.
   Without this prerequisite the entire enforcement floor in
   Phases 2–4 is decorative.
1. **`cargo doc -- -D rustdoc::broken_intra_doc_links`** in CI.
   No comment-policy dependency; just kills broken cross-refs in
   existing docs.
2. **`#![deny(missing_docs)]` at the new `lib.rs` crate root**,
   with `#![allow(missing_docs)]` on every not-yet-graduated
   module. Build stays green (every module is shielded).
   Graduating a module = removing its `#![allow(missing_docs)]`;
   from that point any undocumented `pub` item in that module
   fails CI. The ratchet is monotonic — once a module's allow is
   removed, it cannot regress without a visible diff.
3. **Modules graduate** in audience-criticality order: auth →
   security → models → routes → services. Each graduation is its
   own PR carrying both the docstring authoring and the
   `#![allow]` removal. New modules created mid-rollout author
   docstrings at creation rather than ship a fresh `#![allow]`.
4. **Re-enable previously-allowed clippy pedantic lints**
   (`missing_errors_doc`, currently `allow` in
   `backend/Cargo.toml` with the comment "inappropriate for
   application crates" — true while the crate was bin-only,
   moot after Phase 0). `missing_panics_doc` and
   `missing_safety_doc` are already at the pedantic-warn level
   per `adr/2026-05-03-strict-lint-policy.md`; Tier 1
   docstrings include these sections where applicable.
5. **Frontend mirror** via `eslint-plugin-jsdoc` with
   `require-description` on public exports.

### Authoring shape

Initial backfill batch: agent-driven authoring (haiku / sonnet
subagents per module, dispatched by the maintainer). Subagents
read this ADR + the CLAUDE.md tiered policy section, author
`///` / `//!` docs under the policy, return per-module diffs.
Maintainer reviews per-module PRs.

CodeRabbit's `finishing_touches.docstrings` is **not** the
primary mechanism for the initial backfill — quality is
inconsistent (PR #178 evidence: clipped an existing WHY-comment
mid-sentence) and the per-PR generation surface inflates review
noise during the parallel-trial window. CR docstring generation
may be used ad-hoc on individual PRs long-term, configured
via `.coderabbit.yaml` `path_instructions` to encode this
policy; generated content is reviewed and edited by the
maintainer before landing.

## Consequences

- Good — security audience served. Explicit threat-model
  annotations on Tier 2 code mean an auditor can read the
  security boundary without reconstructing intent from naming
  - git archaeology.
- Good — `cargo doc` rendered library reference becomes a real
  doc surface. Project trust signal shifts in the right
  direction for self-hosters evaluating the codebase.
- Good — Tier 3 carve-out preserves the agent-friendly
  internals workflow. Refactoring private code does not require
  rewriting docstrings; comments rot is contained to where the
  policy explicitly demands them.
- Good — phased rollout means no single mega-PR. Each module
  graduates independently; review burden distributed.
- Bad — initial backfill is a real cost. ~342 backend `pub`
  items + frontend equivalent. Even with subagent dispatch,
  authoring quality docstrings is non-trivial work.
- Bad — Phase 0 (`lib.rs` split) is structural change, not just
  attribute placement. Every internal call site referencing
  `crate::*` from tests or `main.rs` is touched. Carries its own
  review burden before any docstring authoring begins. The
  alternative — accepting that doc lints are silent on the
  binary — was rejected because it makes the entire enforcement
  layering decorative, defeating the ratchet that the OSS
  audience case justifies.
- Bad — comment rot is now possible on Tier 1/2 surfaces. A
  function whose semantics drift from its docstring is worse
  than a function with no docstring. Review discipline must
  catch docstring drift in PRs touching documented code.
- Neutral — CR's docstring auto-generation gets demoted from
  "first-pass content generator" to "ad-hoc tool with policy
  guard-rails". The CR parallel trial gate (2026-05-21)
  evaluates the reviewer feature; the docstring feature is
  evaluated separately under this policy.

## Alternatives Considered

- **Status quo (global "default to no comments" applies in
  full).** Rejected — loses the benefits of explicit
  documentation on the surfaces where the OSS audience needs it
  most. The global rule was tuned for a different audience.
- **Inverse status quo (require docstrings on every item,
  public and private).** Rejected — wave of low-signal
  signature-restating comments on internals where the original
  rule was correct. Comment rot risk dominates.
- **Document only `pub` items, drop the security tier.**
  Rejected — security-critical code has a higher documentation
  bar than other public API. A `pub fn require_admin` whose
  threat model is "string-comparison drift on enum rename"
  needs that drift surfaced in the docstring; a generic Tier 1
  rule misses it. The Tier 2 carve-out makes the security
  ratchet explicit.
- **Skip the CLAUDE.md amendment; encode policy only in clippy
  lints + eslint config.** Rejected — agents read CLAUDE.md
  before they read lint configs. The actionable rule needs to
  be in CLAUDE.md so future agents author docstrings under the
  policy _before_ CI flags the gap. Lint config is the
  enforcement floor; CLAUDE.md is the prescriptive intent.
- **Lean on CodeRabbit `finishing_touches.docstrings` for the
  initial backfill.** Rejected on quality grounds. PR #178
  evidence: CR clipped an existing WHY-comment mid-sentence.
  Configurable `path_instructions` could constrain output, but
  the per-PR generation surface inflates review noise during
  the CR parallel-trial window. Subagent dispatch under
  maintainer review is the cleaner shape for the initial
  backfill.
- **Single mega-PR documenting every backend pub item.**
  Rejected — review burden too high; one bad-quality file
  blocks the entire batch. Per-module PRs distribute the review
  cost and let the maintainer reject one module's docs without
  losing the whole batch.

## More Information

- Global cross-project rule:
  `~/.claude/CLAUDE.md` § "Comments" — the rule this ADR amends
  for the OSS-product context
- `adr/2026-05-03-strict-lint-policy.md` — the strict-lint
  policy whose pedantic clippy lints (`missing_errors_doc`,
  `missing_panics_doc`, `missing_safety_doc`) are the partial
  enforcement floor for this policy
- `backend/CLAUDE.md` § "Rust Code Rules" — `// SAFETY:`
  convention referenced by Tier 2
- CR docstring evidence: PR #178 commit `034e837`,
  `frontend/vite-plugins/hmr-config.ts` (clipped WHY-comment
  pattern that motivated demoting CR from primary backfill
  mechanism)
- UNK-155 row #17 (Greptile dependency-governance catch on PR
  #180) — adjacent observation that documentation visible at
  the call site matters more than documentation in PR review
  history
