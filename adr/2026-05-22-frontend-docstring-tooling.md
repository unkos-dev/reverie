---
status: accepted
date: 2026-05-22
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Frontend docstring linting via `eslint-plugin-jsdoc`

## Context and Problem Statement

`adr/2026-05-08-tiered-comment-policy.md` ratifies a tiered comment
policy for Reverie's OSS-released codebase: Tier 1 (`pub` / `export`
items at module boundaries) and Tier 2 (security-critical surfaces)
carry explicit docstrings; Tier 3 (internal) and Tier 4 (tests +
generator output) do not. Backend Phase 3 graduated under PRs
\#189–#194 between 2026-05-08 and 2026-05-12, every backend module
now sits behind a `#![deny(missing_docs)]` floor at the crate root
with `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"` in CI.

Frontend is the only outstanding scope on the rollout
(the frontend docstring tooling task). The frontend
tree currently has zero JSDoc enforcement: no plugin, no rule, no
ratchet. Authoring without a lint floor would not reproduce
backend's monotonic ratchet: review discipline alone cannot
guarantee a graduated directory stays graduated. The OSS-audience
case the parent ADR makes for backend applies identically to the
frontend tree: external contributors, security auditors, and
self-hosters cold-read TypeScript exports the same way they
cold-read Rust `pub` items.

The decision before us is **which tool enforces the JSDoc floor**.
The frontend stack pins `eslint@^10.4.0` and already carries a
flat-config registration of `typescript-eslint`'s `strictTypeChecked`
preset, `@eslint-react`, `eslint-plugin-react-hooks`, and
`eslint-plugin-react-refresh`. The chosen tool must coexist with
that stack and pass through the `flat`-config registration pattern
established in `adr/2026-05-04-replace-eslint-plugin-react.md`.

A second sub-decision rides along: **which rule set** to enable.
TypeScript already encodes the WHAT (parameter types, return types,
async-ness) in the type signature. The parent policy ADR explicitly
flags `@param x - the x parameter` boilerplate as an anti-pattern.
A heavy rule set risks generating exactly that boilerplate.

A third sub-decision: **ratchet model**. Backend graduated module
by module with per-module `#[allow(missing_docs)]` shields removed
one PR at a time. Frontend has a smaller in-scope footprint (18
files vs. backend's ~342 `pub` items) and a single flat-config file
governing all linting, so the cost of a per-directory ratchet is
relatively higher.

## Decision

### Tool — `eslint-plugin-jsdoc`

Adopt `eslint-plugin-jsdoc` (current latest matching
`eslint@^10.4.0`, peer-dep range `^7 || ^8 || ^9 || ^10`). Pin in
`frontend/package.json` under `devDependencies`. Register through
the existing flat-config block scoped to
`frontend/src/**/*.{ts,tsx,js}` and
`frontend/vite-plugins/**/*.ts`.

### Rule set: presence + description only

Enable a minimal pair of rules:

- `jsdoc/require-jsdoc` — scoped via the rule's `contexts` filter
  to `ExportNamedDeclaration`, `ExportDefaultDeclaration`, and
  `TSInterfaceDeclaration` with an `export` modifier. Public
  exports only; internal helpers in the same file are Tier 3 and
  exempt by construction.
- `jsdoc/require-description` — any JSDoc block that does exist
  must carry a description sentence. Closes the empty-`/** */`
  loophole.

**Do not** enable `jsdoc/require-param-description`,
`jsdoc/require-returns-description`, or any rule that requires a
specific JSDoc tag to be present. Rationale: TypeScript types
encode the WHAT (parameter shape, return shape); JSDoc carries the
WHY (purpose, invariants, non-obvious constraints, threat-model
annotations for Tier 2). Requiring `@param`/`@returns` descriptions
would create machine pressure toward the very boilerplate the
parent policy bans. Backend parallel: `clippy::missing_errors_doc`
is the only Rust doc-content rule the project enables, and it is
enabled precisely because `Result<T, E>` error variants are not
expressed in the function signature. TypeScript's `Promise<T>` and
union return types are expressed in the signature; the JSDoc
equivalent of `# Errors` is therefore not required.

### Ratchet: single big-bang flip

Run the rules at `warn` during the Stage B install PR and through
every Stage C authoring PR. A subsequent flip PR (Stage D, see
Implementation Plan below) flips both rules to `error` and adds
`--max-warnings 0` enforcement in CI. Rationale: backend's
per-module ratchet existed because
backend had ~342 `pub` items across nine modules; per-directory
graduation distributed review burden. Frontend's 18 files across
five groupings make per-directory ratchet overhead
disproportionate; a single config flip after authoring is complete
hits the same monotonic-ratchet property with less ceremony. The
risk this trades for (a regression slipping in between the
authoring PR and the flip PR) is bounded by the parent ADR's
review discipline (every PR touching in-scope files must add JSDoc
on new exports) plus the warn-mode lint floor that runs throughout
the rollout.

### Carve-outs

`frontend/eslint.config.js` adds two scoped overrides that disable
both rules:

1. `frontend/src/components/ui/**` — shadcn primitives are
   generator output; per parent ADR Tier 4.
2. `frontend/tests/**`, `**/*.test.{ts,tsx}`, `**/*.spec.{ts,tsx}`,
   `frontend/tests/setup.ts` — test files are Tier 4 per parent
   ADR (test name is the spec).

`frontend/src/fouc/fouc.js` is in-scope despite being `.js` (not
TS), the security pinning relationship to
`vite-plugins/csp-hash.ts` makes it a Tier 2 surface. The plugin's
`files` glob must include `*.js`.

## Consequences

- **Good**: frontend cold-read surface graduates to the same
  documentation bar as the backend `pub` API. OSS-audience case
  closed across both halves of the codebase.
- **Good** — minimal rule set avoids `@param`-boilerplate
  anti-pattern called out by the parent ADR. Lint enforces
  presence; policy text + reviewer + bot review enforce content
  quality.
- **Good** — `eslint-plugin-jsdoc` is well-maintained, widely
  adopted in TS+ESLint stacks, and supports flat config. Zero new
  custom rule code to maintain.
- **Good**: single big-bang flip avoids the per-directory PR
  overhead that would have multiplied review burden for a
  comparatively small in-scope footprint.
- **Bad**: new devDependency (one additional npm package and its
  transitive tree). Dependabot / Renovate surface grows by one.
  Trade-off explicitly accepted vs. the maintenance cost of a
  hand-rolled `no-restricted-syntax` rule.
- **Bad**: between the final authoring PR and the flip PR, a new
  un-documented `export` could merge under `warn` mode without
  breaking the build. Mitigation: every Stage C PR carries the
  ratchet-flip in its scope guard; reviewer treats any new
  un-documented export in those PRs as a blocker.
- **Bad**: single big-bang flip means the final PR carries both
  the config flip and any final authoring carry-over. If that PR
  fails review, the flip stalls. Mitigation: keep the flip-only PR
  separate from the last authoring PR: config diff is small and
  reviewable in isolation.
- **Neutral**: the rule set does not enforce Tier 2 threat
  annotations. Those are a content requirement, not a presence
  requirement; review discipline and the parent ADR's section
  template carry that load.

## Alternatives Considered

### `typescript-eslint` only (no extra dependency)

Use the existing `typescript-eslint` parser + a custom
`no-restricted-syntax` selector to require a leading comment on
every `ExportNamedDeclaration`. Zero new package.

Rejected — `typescript-eslint` upstream removed the `valid-jsdoc`
rule; no native equivalent exists. A custom AST selector can
enforce presence but cannot enforce shape (description sentence,
empty-block detection). Maintenance burden falls on us; the
existing project pattern from
`adr/2026-05-04-replace-eslint-plugin-react.md` is to pick up
well-maintained ecosystem plugins rather than hand-roll AST
selectors when the plugin exists.

### Hand-rolled custom rule plugin

Maximum control. Could express exact Tier 1 / Tier 2 distinction
in the rule itself.

Rejected: substantial AST-rule code to write and maintain for a
rule set that `eslint-plugin-jsdoc` already provides. The
maintenance burden is on the project forever; the marginal control
gained does not justify the cost when the policy text + reviewer +
bot review already carry content-quality enforcement.

### Per-directory ratchet (warn → error one directory at a time)

Mirror backend Phase 3 module-by-module graduation. Each Stage C
PR flips its own directory from `warn` to `error` after authoring.

Rejected on cost grounds. Backend's per-module ratchet was load-
bearing because backend had ~342 `pub` items across nine modules;
the per-module PR shape distributed review burden meaningfully.
Frontend's 18 files across five groupings yield a much smaller
review burden per PR. A second config diff per Stage C PR
(authoring + flip) doubles the PR-config surface for an enforcement
property that the single big-bang flip captures identically.

### Full rule set (require-jsdoc + require-description + require-param-description + require-returns-description)

Maximum machine enforcement. Catches authors who omit
`@param`/`@returns` content when they choose to write those tags.

Rejected on signal-to-noise grounds. Parent ADR explicitly bans
`@param x - the x parameter` boilerplate. Enabling
`require-param-description` creates machine pressure toward
exactly that anti-pattern. The lint floor is for presence and
non-emptiness; content quality is a review concern with policy
text as the authority.

### `warn`-only forever (no error graduation)

Skip the final flip. Rely on review discipline + bot review to
catch missing JSDoc indefinitely.

Rejected: no ratchet means no monotonic guarantee. The
OSS-audience case in the parent ADR rests on the ratchet:
documentation is a property the codebase preserves under change,
not a property reviewers happen to catch on the day.

## More Information

- Parent: `adr/2026-05-08-tiered-comment-policy.md` (tier
  definitions, anti-patterns, threat-annotation shape).
- Backend ratchet precedent: `backend/src/lib.rs:15`
  (`#![deny(missing_docs)]`), enabled under PRs #189–#194.
- Flat-config registration precedent:
  `adr/2026-05-04-replace-eslint-plugin-react.md`.
- Strict-lint family: `adr/2026-05-03-strict-lint-policy.md`.
- Implementation plan ingest:
  `.claude/PRPs/plans/unk-236-frontend-jsdoc.plan.md`.
- Tracker: the frontend docstring tooling task
  (parent: the comment policy phased rollout task).
- `eslint-plugin-jsdoc` peer-dep range verified 2026-05-22:
  `^7 || ^8 || ^9 || ^10` — covers the pinned
  `eslint@^10.4.0`.
