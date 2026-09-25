---
type: ADR
profile-version: 1
id: "REV-ADR-0004"
title: "Tiered comment policy for an open-source codebase"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-08"
decision-makers:
  - "John Unkovich"
---

# Tiered comment policy for an open-source codebase

## Context and problem statement

A cross-project convention ratifies "default to no comments, only add one when the WHY is non-obvious" as the default.
The rule is well tuned for a solo-developer or shared-team context, where the readers are the author and a small group
with shared conversation history; under those conditions comments rot fast and self-explanatory naming carries most of
the load.

Reverie's audience is different. The project is published open source under a self-hosting positioning: code is read by
external contributors, security auditors deciding whether to trust the codebase on their own infrastructure, and
operators inspecting a deployment before installing it. None of those readers share conversation history with the
maintainer.

- External contributors orienting cold need to know what a module is for before they can change it safely; self-evident
  naming carries less weight when the reader has zero project context.
- Security auditors read security-critical code without project context. They need explicit threat-model statements at
  the boundary, not implicit ones reconstructed from naming.
- `cargo doc` consumers, a class that includes some auditors and contributors, read the rendered library reference.
  Empty docstrings on `pub` items render as a lazy library shape, a trust signal in the wrong direction for a
  self-hosted product.

The project has already seen two signals that the global default needs amendment here. Automated docstring generation
has run on a merged change without a policy or quality bar to guide it, and in doing so damaged an existing WHY-comment
it ran across; the generation activity is happening already, it just has no shape. Separately, automated
dependency-governance review has flagged that a version-pin's rationale, left only in a pull request body and an inline
comment, is not a durable decision record: a pull request body is invisible to anyone reading the merged tree. Both
point at the same fact: the documentation surface a reader lands on matters more than whichever surface the author found
convenient at the time.

The status quo, one comment rule for every surface, loses the benefit of explicit documentation on the surfaces where
these readers need it most, in service of a rule tuned for a different audience. Which surfaces should carry explicit
documentation, and which should keep the low-comment default?

## Decision drivers

- External contributors orienting cold need a module's purpose stated explicitly; naming alone assumes context a
  stranger does not have.
- Security auditors need explicit threat-model statements at security boundaries, not statements reconstructed from
  naming or git history.
- `cargo doc`'s rendered reference is a real trust signal for self-hosters evaluating the codebase before they install
  it, so empty docstrings on `pub` items cost more here than in an internal tool.
- Automated docstring generation is already happening on individual changes, inconsistently and without a quality bar; a
  policy is needed to give that activity a shape rather than let it happen ad hoc.
- Rationale left only in review history is invisible to a reader of the merged tree; the decision needs a surface that
  ships with the code.

## Considered options

- Adopt a tiered comment policy: keep the low-comment default for internal code, and require explicit documentation on
  public API and security-critical code.
- Status quo: the global "default to no comments" rule applies in full, everywhere.
- Inverse status quo: require docstrings on every item, public and private.
- Document only `pub` items, with no separate tier for security-critical code.
- Encode the policy only in lint configuration, without amending the project's agent instructions.
- Rely on automated docstring generation as the primary mechanism for the initial backfill.
- Author the initial backfill as a single pull request covering every backend `pub` item.

## Decision outcome

Chosen option: **adopt a tiered comment policy**, because it keeps the low-comment default where it already works,
internal code, while giving the audience that actually needs explicit documentation, external contributors, security
auditors, and `cargo doc` readers, an explicit floor on the surfaces they read.

Tier 1, public API. Every `pub fn`, `pub struct`, `pub enum`, `pub trait`, and `pub const` exposed at a module boundary
carries a `///` Rust doc comment, or JSDoc on a TypeScript export. Module tops carry `//!` in Rust, or a file-header
docblock in TypeScript, stating purpose, invariants, and load-bearing constraints. Required content is the purpose in
one sentence, the invariants a caller can rely on, the non-obvious WHY where one exists, an `# Errors` section on any
`pub fn` returning `Result<…>`, a `# Panics` section on any `pub fn` that may panic, and a `# Safety` section on any
`pub unsafe fn`, aligning with the `// SAFETY:` convention in `backend/AGENTS.md`. A docstring that only restates the
signature, that clips or replaces an existing leading WHY-comment rather than sitting above it, or that repeats generic
boilerplate, is worse than no docstring and is not acceptable under this tier.

Tier 2, security-critical code. Code under `backend/src/auth/`, `backend/src/security/`, and any function handling
credentials, sessions, the OIDC flow, role assertions, row-level-security context, secret material, or response-header
policy carries explicit threat-model annotations beyond the standard docstring: `// THREAT:` comments inline for
non-obvious mitigations, stating the attack vector being closed, any pre-existing protection, and the invariant this
code adds; a one-line threat statement near the top of the docstring on a security-boundary function; and a reference to
the relevant decision record by relative path when the context motivating the code lives in one.

Tier 3, internal code. Private functions, private structs, and private modules keep the original rule in full: no
docstring is required, and a comment is added only when the WHY is non-obvious, a constraint is hidden, or the code
would surprise a future reader. Tier 4, tests. Test functions, whichever runner they use, do not carry docstrings; the
test name is the spec, and a docstring restating it is noise. `test_support/` modules carry module-top docs where a
helper's purpose is non-obvious; the helpers themselves stay bare unless they encode a WHY a future reader would not
infer.

### Consequences

- Positive: security auditors are served directly. Explicit threat-model annotations on security-critical code let an
  auditor read the boundary without reconstructing intent from naming or git history.
- Positive: `cargo doc`'s rendered library reference becomes a real documentation surface, moving the project's trust
  signal in the right direction for self-hosters evaluating the codebase.
- Positive: the carve-out for private code preserves the low-comment default where it already worked; refactoring
  internal code does not require rewriting docstrings, so comment rot is contained to the surfaces that explicitly
  demand them.
- Positive: modules graduate independently, so review burden is distributed rather than concentrated in one large
  change.
- Negative: the initial backfill is a real cost, across several hundred backend `pub` items and the frontend equivalent;
  authoring quality docstrings is non-trivial even with subagent dispatch.
- Negative: splitting the backend into a library and a thin binary entry is a structural change, not just attribute
  placement; every internal call site referencing the crate from tests or the binary is touched, and it has to land
  before any docstring authoring begins. Accepting that the doc lints stay silent on a bin-only crate was rejected
  because it makes the whole enforcement mechanism decorative.
- Negative: documented surfaces can now drift; a function whose semantics diverge from its docstring is worse than one
  with no docstring, so review has to catch docstring drift in changes that touch documented code.
- Negative: frontend docstring presence lost its mechanical floor when `eslint-plugin-jsdoc` was removed with the ESLint
  toolchain; it is now a reviewed convention rather than a lint gate.

## Pros and cons of the options

### Adopt a tiered comment policy

- Positive: serves the open-source audience, contributors, auditors, and `cargo doc` readers, on exactly the surfaces
  they read, while keeping the low-comment default where it already worked.
- Negative: a real backfill cost, and an ongoing risk of docstring drift on the surfaces it newly documents.

### Status quo: global "default to no comments" applies in full

- Negative: loses the benefit of explicit documentation on the surfaces the open-source audience needs it most; the rule
  was tuned for a different audience than Reverie now has.

### Inverse status quo: require docstrings on every item, public and private

- Negative: a wave of low-signal, signature-restating comments on internal code, where the original rule was already
  correct; comment-rot risk dominates any benefit.

### Document only `pub` items, with no separate tier for security-critical code

- Negative: security-critical code carries a higher documentation bar than public API in general; a generic Tier 1 rule
  misses the threat-model context that only a dedicated tier makes explicit.

### Encode the policy only in lint configuration, without amending the project's agent instructions

- Negative: agents read the project's agent instructions before they read lint configuration, so the actionable rule
  needs to live there for docstrings to be authored under the policy before continuous integration flags the gap; lint
  configuration is the enforcement floor, not the prescriptive source.

### Rely on automated docstring generation as the primary mechanism for the initial backfill

- Negative: quality has been inconsistent, including damaging an existing WHY-comment, and running it on every change
  during backfill inflates review noise; subagent dispatch under maintainer review is the cleaner shape for a one-time
  backfill.

### Author the initial backfill as a single pull request covering every backend `pub` item

- Negative: review burden concentrates in one place, and one low-quality file blocks the whole batch; per-module changes
  distribute the review cost and let a single module be rejected without losing the rest.

## More information

- [Strict lint policy: pedantic clippy and strict frontend lint](./0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md):
  the pedantic clippy allow-list this policy narrows: `missing_errors_doc`, `missing_panics_doc`, and
  `too_long_first_doc_paragraph` are enforced now that the backfill is complete.
- [Adopt oxlint, replacing the ESLint toolchain](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md): the toolchain
  change that removed the frontend docstring lint.
- Enforcement reach: `missing_panics_doc` sees a function's own body, so the `# Panics` obligation applies to a panic
  the function itself raises; a panic reached through a function it calls belongs to that function's section.
- `backend/AGENTS.md` carries the `// SAFETY:` convention Tier 2 references for unsafe code.
