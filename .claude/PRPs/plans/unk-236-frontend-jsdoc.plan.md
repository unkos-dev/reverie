# Feature: Phase 4 — Frontend JSDoc backfill + lint enforcement (UNK-236)

## Summary

Mirror the backend Phase 3 graduation (auth → security → models → routes →
root → services, complete 2026-05-12 under PRs #189–#194) onto the
frontend tree. **18 in-scope files** under `frontend/src/**` +
`frontend/vite-plugins/**`. Tier 1 JSDoc on every public export; Tier 2
threat annotations on three security paths (`vite-plugins/csp-hash.ts`,
`vite-plugins/allowed-hosts.ts`, `src/fouc/fouc.js`). Lint enforcement
graduates `warn → error` per directory once authored.

Ticket: [UNK-236](https://linear.app/unkos/issue/UNK-236) — Backlog,
Priority High. Parent: UNK-190.

## Problem Statement

Frontend public exports (functions, components, props interfaces, Vite
plugin factories) carry no JSDoc. Backend has graduated under tiered
comment policy; frontend is the only outstanding scope on the rollout. OSS
audience (external contributors, security auditors, self-hosters) cold-reads
the rendered library reference and the source — same justification as ADR
`2026-05-08-tiered-comment-policy.md` Tier 1.

Lint currently silent: no `eslint-plugin-jsdoc` (or equivalent) installed.
Authoring without a lint floor cannot enforce the ratchet pattern that
backend used (`#![deny(missing_docs)]` per-module graduation).

## Solution Statement

**Two-stage:** ADR-gated tooling decision first, then authoring under
the chosen lint floor.

- **Stage A** — Tooling ADR (own PR). Pick `eslint-plugin-jsdoc` vs
  `typescript-eslint`-only via the `adr` skill's Socratic capture. Ratify,
  index in `adr/README.md`. **Blocking gate — authoring cannot open
  until ADR is `accepted`.**
- **Stage B** — Plugin install + flat-config registration. Run plugin at
  `warn` initially; capture census output (per-file finding count).
  Single PR or split per directory subject to census volume.
- **Stage C** — Per-directory JSDoc authoring + ratchet flip (`warn →
error`). One PR per directory grouping. Subagent dispatch optional;
  18 files is small enough for single-thread authoring.

## Metadata

| Field            | Value                                                                |
| ---------------- | -------------------------------------------------------------------- |
| Type             | DOCS + LINT-INFRA (multi-PR; ADR-gated)                              |
| Complexity       | MEDIUM (ADR + plugin install + authoring + graduation)               |
| Systems Affected | `frontend/{src,vite-plugins}/**`, `frontend/eslint.config.js`        |
| Dependencies     | `eslint@^10.4.0` (installed); `eslint-plugin-jsdoc` (TBD)            |
| Authoring scope  | 18 files (ticket said 48 — Tier 4 carve-outs bring it down)          |
| PR shape         | 1 (ADR) + 1 (plugin install) + N (authoring; N = 4–7 per grouping)   |
| Branch base      | `feature/unk-236-frontend-jsdoc-backfill` (per Linear gitBranchName) |

## Pre-flight verification (already done)

- `eslint@^10.4.0` pinned (`frontend/package.json`).
- `eslint-plugin-jsdoc` peer dep range: `^7.0.0 || ^8.0.0 || ^9.0.0 ||
^10.0.0` — supports current eslint pin.
- In-scope file count: **18** (excl. shadcn UI primitives, test files,
  `tests/setup.ts`).

## In-scope file inventory

| Path                                                    | Tier | Notes                                                   |
| ------------------------------------------------------- | ---- | ------------------------------------------------------- |
| `frontend/vite-plugins/csp-hash.ts`                     | T2   | Hashes FOUC inline script into HTML CSP — load-bearing. |
| `frontend/vite-plugins/allowed-hosts.ts`                | T2   | Vite dev-server DNS-rebinding guard.                    |
| `frontend/vite-plugins/hmr-config.ts`                   | T1   | Operator surface; non-security.                         |
| `frontend/src/fouc/fouc.js`                             | T2   | The single inline script CSP hashes against.            |
| `frontend/src/main.tsx`                                 | T1   | Root mount.                                             |
| `frontend/src/App.tsx`                                  | T1   | Router setup.                                           |
| `frontend/src/routes/design.tsx`                        | T1   | Dev-only `design` route entry.                          |
| `frontend/src/lib/utils.ts`                             | T1   | `cn` helper etc.                                        |
| `frontend/src/lib/theme/ThemeProvider.tsx`              | T1   | Theme context provider.                                 |
| `frontend/src/lib/theme/api.ts`                         | T1   | Theme PATCH client.                                     |
| `frontend/src/lib/theme/cookie.ts`                      | T1   | Cookie read/write helpers.                              |
| `frontend/src/components/Lockup.tsx`                    | T1   | Brand lockup component.                                 |
| `frontend/src/components/theme-switcher.tsx`            | T1   | Theme switcher UI.                                      |
| `frontend/src/pages/design/library.tsx`                 | T1   | Hero screen.                                            |
| `frontend/src/pages/design/system.tsx`                  | T1   | Design system page.                                     |
| `frontend/src/pages/design/book.tsx`                    | T1   | Hero screen.                                            |
| `frontend/src/pages/design/components/CoverArtwork.tsx` | T1   | Page-local component.                                   |
| `frontend/src/pages/design/fixtures/books.ts`           | T1   | Static fixture data.                                    |
| **`frontend/src/components/ui/**`\*\*                   | T4   | **Excluded** — shadcn-generated.                        |
| **`frontend/tests/**`, `\*.test.{ts,tsx}`\*\*           | T4   | **Excluded** — test name is spec.                       |

---

## Mandatory Reading

| Priority | File                                              | Why                                                        |
| -------- | ------------------------------------------------- | ---------------------------------------------------------- |
| P0       | `adr/2026-05-08-tiered-comment-policy.md`         | Tier 1 / Tier 2 / Tier 4; anti-patterns; threat shape.     |
| P0       | `frontend/CLAUDE.md`                              | Frontend conventions (no enum, no `as`, hooks rules).      |
| P0       | UNK-236 ticket body                               | Hard rules + lessons from Phase 3c-4 (PR #194).            |
| P0       | UNK-187 row #11 / `feedback_bot_review_triage.md` | CR outside-diff finding triage pattern.                    |
| P1       | PR #194 (backend services) + #189 (backend auth)  | Tier 1+2 shape; threat-annotation tabular PR body.         |
| P1       | `frontend/eslint.config.js` (current)             | Flat-config registration pattern to mirror.                |
| P1       | `frontend/vite-plugins/allowed-hosts.ts`          | Already carries security-claim inline comments — preserve. |
| P2       | adr/README.md                                     | ADR index format.                                          |

External docs (Stage A research):

- `eslint-plugin-jsdoc` README — rule names, flat-config example, `contexts` filter.
- `typescript-eslint` `recommended-type-checked` config — what doc-related rules ship by default.

---

## NOT Building (Scope Limits)

- **Behavioural fixes.** If bot review surfaces frontend bugs during the
  authoring PRs, file separate UNK tickets (UNK-234 / UNK-235 precedent on
  PR #194). Do not stuff fixes into this PR.
- **Backend touches.** `backend/CLAUDE.md` Phase 3 complete; backend
  out of scope.
- **New ESLint rules beyond require-description set.** Rule expansion is
  its own decision.
- **shadcn `components/ui/*`.** Tier 4 carve-out; lint must auto-suppress
  via flat-config override.
- **Test files.** Tier 4; test name is the spec.
- **Refactor surrounding code.** Pure docstring + eslint config diff.

---

## Stage A — Tooling ADR (gating, own PR)

**Cannot be auto-executed.** Requires Socratic capture via `adr` skill.

### Tasks

1. **Branch**: `chore/unk-236-frontend-docstring-tooling-adr` from `main`.
2. **Invoke** `Skill("adr")` with topic: "Frontend docstring linting
   tooling: `eslint-plugin-jsdoc` vs `typescript-eslint`-only doc rules".
3. **Socratic capture** (skill drives) — covers:
   - Problem: enforce Tier 1+2 docstrings on public exports without
     drift; ratchet warn→error.
   - Alternatives: (a) `eslint-plugin-jsdoc` — `require-jsdoc`,
     `require-description`, `require-param-description`,
     `require-returns-description`, `no-undefined-types` (TS-aware mode);
     (b) `typescript-eslint`-only — relies on `valid-jsdoc`-like rules
     (deprecated upstream) or custom AST rule via `no-restricted-syntax`;
     (c) custom rule.
   - Decision drivers: ESLint 10 peer-dep compatibility (a: yes, peer
     range `^7 || … || ^10`); TS-awareness for typed props; flat-config
     ergonomics; maintenance burden.
4. **Draft** `adr/2026-MM-DD-frontend-docstring-tooling.md` (MADR shape).
5. **Index** in `adr/README.md`.
6. **Validate** via skill checklist (agent-readiness).
7. **PR**: title `docs(adr): frontend docstring linting tooling
(UNK-236)`; body explains the decision + alternatives rejected;
   request user review.
8. **STOP** — wait for ADR `accepted` status before opening Stage B.

### Acceptance (Stage A)

- [ ] ADR file authored, status `accepted` after user review.
- [ ] Indexed in `adr/README.md`.
- [ ] PR merged.

---

## Stage B — Plugin install + flat-config registration

**Gated on Stage A acceptance.**

### Stage B tasks

1. **Branch**: `feature/unk-236-jsdoc-plugin-install` from `main`.
2. **Install**: `cd frontend && npm install --save-dev
eslint-plugin-jsdoc@<pinned>` (pin to current latest matching
   eslint@^10).
3. **Register** in `frontend/eslint.config.js` — mirror the existing
   `reactHooks` / `reactX` registration pattern; flat-config block scoped
   to `frontend/src/**/*.{ts,tsx,js}` + `frontend/vite-plugins/**/*.ts`.
4. **Carve-outs** (per scope-limits table):
   - `frontend/src/components/ui/**` — disable docstring rules (shadcn-generated).
   - `frontend/tests/**`, `**/*.test.{ts,tsx}`, `**/*.spec.{ts,tsx}` — disable docstring rules (Tier 4).
5. **Rule set** (per ADR decision; assuming `eslint-plugin-jsdoc` wins):
   - `jsdoc/require-jsdoc` — scoped to `ExportNamedDeclaration`,
     `ExportDefaultDeclaration`, `TSInterfaceDeclaration` with `export`.
   - `jsdoc/require-description`
   - `jsdoc/require-param-description`
   - `jsdoc/require-returns-description`
   - `jsdoc/no-undefined-types` (TS mode)
   - **All `warn`** initially.
6. **Census**: `cd frontend && npm run lint > /tmp/jsdoc-census.txt
2>&1`. Tally warnings per directory. Record in PR body.
7. **PR**: title `chore(frontend): install eslint-plugin-jsdoc + flat-config
wiring (UNK-236)`; body includes census table.

### Validation (Stage B)

```bash
cd frontend
npm run lint     # exits 0 (warnings allowed; rules at warn)
npm run build    # tsc -b && vite build
npm test         # vitest run
```

### Acceptance (Stage B)

- [ ] Plugin installed, flat-config wired.
- [ ] Carve-outs scoped (shadcn UI + tests).
- [ ] `npm run lint` runs clean (warnings counted, exit 0).
- [ ] `npm run build` succeeds.
- [ ] `npm test` green.
- [ ] PR body includes per-directory census.

---

## Stage C — Per-directory JSDoc authoring + ratchet flip

**Gated on Stage B merge.** Multiple PRs; one per directory grouping.

### Grouping (PR-per-row)

| #   | Grouping                                       | Files | Tier mix |
| --- | ---------------------------------------------- | ----- | -------- |
| 1   | `vite-plugins/**` + `src/fouc/`                | 4     | T2 + T1  |
| 2   | `src/lib/**`                                   | 4     | T1       |
| 3   | `src/components/**` (excl. UI)                 | 2     | T1       |
| 4   | `src/pages/design/**`                          | 5     | T1       |
| 5   | `src/routes/` + `src/App.tsx` + `src/main.tsx` | 3     | T1       |

### Per-PR shape (one PR per grouping above)

1. **Branch**: `feature/unk-236-jsdoc-<grouping-slug>` from `main`.
2. **Author** JSDoc per file (patterns below).
3. **Flip ratchet**: in `frontend/eslint.config.js`, add a directory-scoped
   override that promotes `warn → error` for this grouping.
4. **Validate**:

   ```bash
   cd frontend
   npm run lint            # exit 0; 0 errors, 0 warnings in scope
   npm run build
   npm test
   ```

5. **Commit** (Conventional Commits):
   `docs(frontend/<grouping>): backfill Tier 1[+2] JSDoc (UNK-236)`.
6. **PR** body: file-by-file public-export count (before); Tier
   classification; threat-annotation rows for T2; confirmation that
   `npm run lint` returns 0 errors 0 warnings on the directory.

### Authoring patterns

#### Tier 1 — module-top JSDoc (functions / components)

```typescript
/**
 * Short one-line purpose.
 *
 * Longer paragraph: invariants, what callers must hold, what this
 * guarantees. Non-obvious WHY where present.
 *
 * @param x - description (only when non-obvious; skip if name carries it)
 * @returns description (only when non-obvious)
 */
export function foo(x: Bar): Baz { ... }
```

#### Tier 1 — exported interface / props

```typescript
/**
 * Props for {@link ThemeSwitcher}.
 *
 * Invariants: `value` mirrors the cookie set by the backend on /auth/me.
 */
export interface ThemeSwitcherProps {
  /** Current theme; `system` defers to OS-level `prefers-color-scheme`. */
  value: ThemePreference;
  /** Persistence call; awaited so error toasts can branch on the result. */
  onChange: (next: ThemePreference) => Promise<void>;
}
```

#### Tier 2 — threat annotation shape (mirror backend `auth/middleware.rs::verify_basic`)

```typescript
/**
 * Build the script-src hash list for HTML CSP.
 *
 * THREAT: an attacker who can inject a new inline `<script>` into
 * `index.html` will bypass CSP if the hash list is regenerated to
 * cover it. Mitigation: this plugin hashes ONLY the
 * `frontend/src/fouc/fouc.js` file; any other inline script in
 * `index.html` is intentionally unhashed and will be blocked by
 * the runtime CSP at navigation time.
 *
 * Cross-references:
 * - adr/2026-05-08-tiered-comment-policy.md § Tier 2
 * - backend/src/security/csp.rs (matching backend-side policy)
 *
 * @returns Vite plugin emitting a hash sidecar consumed by
 *   `backend/src/security/dist_validation.rs` at startup.
 */
export function cspHash(): Plugin { ... }
```

### Anti-patterns (REFUSE — skip the JSDoc rather than commit these)

- Pure signature restatement (`/** Returns the value */`).
- Generic `@param x - The x parameter`.
- Clipping or replacing existing leading `//` comments — new `/**` block
  goes **above** any existing leading comments, never replaces them. PR
  #178 (`hmr-config.ts` commit `034e837`) is the canonical negative.

### Validation (per PR)

```bash
cd frontend
npm run lint                # 0 errors, 0 warnings in scope
npm run build               # tsc -b && vite build
npm test                    # vitest run
```

Then verify lint floor per grouping:

```bash
cd frontend
npx eslint <grouping-glob> --max-warnings 0
```

### Acceptance (per Stage C PR)

- [ ] Every public export in grouping carries Tier 1 (or Tier 1+2) JSDoc.
- [ ] T2 files carry inline `// THREAT:` plus top-of-JSDoc threat statement
      plus ADR cross-reference.
- [ ] No existing `//` leading comments removed, clipped, or replaced.
- [ ] Directory-scoped lint override flipped warn→error.
- [ ] `npm run lint` exits 0 with 0 warnings in scope.
- [ ] `npm run build` + `npm test` green.
- [ ] PR body groups items by file; lists Tier classification; declares
      out-of-scope for any behavioural findings bots raise (file
      separate tickets per UNK-234/235 discipline).
- [ ] CR + Greptile review tally rows added to UNK-187 + UNK-155 per
      `feedback_bot_review_triage.md`.

---

## Validation Commands (across stages)

### Level 1 — Lint

```bash
cd frontend && npm run lint
```

### Level 2 — Typecheck + build

```bash
cd frontend && npm run build
```

### Level 3 — Tests

```bash
cd frontend && npm test
```

### Level 4 — Pre-push hook

```bash
sh /home/coder/reverie/.husky/pre-push
```

### Level 5 — Browser smoke (after Stage C grouping 4 — design pages)

Use `agent-browser` against `localhost:5173`. Pages must still render —
docstring-only diff but JSDoc lint can surface symbol-level issues that
type-check missed.

---

## Risks and Mitigations

| Risk                                                                                                   | Likelihood          | Impact | Mitigation                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------ | ------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| ADR Socratic capture stalls — decision drivers underspecified.                                         | MED                 | MED    | Pre-flight tooling-comparison memo (1 page); skill drives Q&A; halt and surface if drivers not converging by Q5.                            |
| `eslint-plugin-jsdoc` flat-config wiring conflicts with existing `tseslint.configs.strictTypeChecked`. | LOW                 | LOW    | Stage B validation suite runs `npm run lint` clean; conflict surfaces immediately.                                                          |
| Authoring drift — JSDoc describes behaviour the code doesn't have.                                     | MED                 | MED    | Read function body before writing; cross-check against tests; halt + "owner clarification needed" comment if uncertain.                     |
| Clipping pre-existing WHY-comment (PR #178 anti-pattern, repeated risk on `allowed-hosts.ts`).         | LOW                 | HIGH   | Per-file manual diff: `git diff <file>` and confirm zero `-//` lines on existing comment blocks.                                            |
| CR / Greptile surfaces behavioural findings during docs PRs — scope creep risk.                        | MED                 | LOW    | UNK-234/235 discipline: file separate tickets, do NOT fold into the docs PR.                                                                |
| Single-session autonomous Ralph compounds pivots (already pivoted routes → UNK-236).                   | HIGH (if attempted) | HIGH   | **Hand back at end of plan authoring.** User drives ADR Socratic capture; Ralph only re-engages on Stage B / Stage C if explicitly invoked. |
| Census discrepancy: lint surfaces materially more findings than 18-file count suggests.                | MED                 | LOW    | Stage B `warn`-mode census output drives Stage C grouping subdivisions — let data shape the work, don't pre-commit.                         |

---

## Completion Checklist (full feature; multi-PR)

- [ ] Stage A — Tooling ADR authored, accepted, indexed, PR merged.
- [ ] Stage B — Plugin installed, flat-config wired, census captured, PR merged.
- [ ] Stage C-1 — `vite-plugins/**` + `fouc/` (T2): PR merged.
- [ ] Stage C-2 — `lib/**`: PR merged.
- [ ] Stage C-3 — `components/**` (excl. UI): PR merged.
- [ ] Stage C-4 — `pages/design/**`: PR merged.
- [ ] Stage C-5 — `routes/` + `App.tsx` + `main.tsx`: PR merged.
- [ ] Final lint floor: `npm run lint` exits 0 with the docstring rule
      set at `error` for all in-scope directories.
- [ ] CR + Greptile tally rows appended for every PR.
- [ ] UNK-236 closed; comment with state-change summary on UNK-190.

---

## Notes

- **18 files in scope** (ticket said 48 — Tier 4 carve-outs of shadcn UI +
  test files account for the gap).
- `vite-plugins/allowed-hosts.ts` already carries security-claim inline
  comments from UNK-170 — preserve verbatim. The JSDoc block goes above.
- `fouc/fouc.js` is a `.js` file (not TS) — the existing CSP-hash
  pipeline matches it specifically. Stage B lint config must include
  `js` in its `files` glob.
- Per global feedback memory: **never include "Generated with Claude
  Code" attribution in any PR body**.
- Per global feedback memory: **agents never merge; hand off at "PR
  green, ready for review"**.
- Per global feedback memory: **one pivot max per session**. Plan
  authoring is the only work this session; execution waits.
