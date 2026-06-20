---
status: accepted
date: 2026-06-04
supersedes: ["superseded/2026-05-08-tower-sessions-sqlx-store.md"]
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# First-party session layer on tower-sessions core; drop axum-login and tower-sessions-sqlx-store

## Context and Problem Statement

Reverie's session/auth stack rests on four crates by a single maintainer
(maxcountryman): `tower-sessions` (with `tower-sessions-core`),
`tower-sessions-sqlx-store`, and `axum-login`. Two of those wrappers are
abandoned on crates.io:

- **`axum-login` 0.18.0**: last released 2025-07-20 (~11 months). Peer-pins
  `tower-sessions = "0.14"`. The upstream fix bumping it to `tower-sessions`
  0.15 merged 2026-05-07 (commit `151c72d`) but has never been released.
- **`tower-sessions-sqlx-store` 0.15.0**: last released 2025-01-01 (~17
  months). Its `tower-sessions-core` dependency pins `^0.14`.

These are two independent peer-pins that jointly block
The coordinated `tower-sessions` upgrade issue: the `tower-sessions`
0.14 → 0.15 bump (Renovate PR
[#128](https://github.com/unkos-dev/reverie/pull/128)). Removing one does not
unblock the bump; the other still holds 0.14. `tower-sessions` 0.15 (released
2026-02-01) carries a memory-ordering race fix and a `rand` 0.9 update worth
picking up, and Renovate re-surfaces the blocked PR every sweep at an ongoing
human-attention cost.

By contrast, `tower-sessions` **core is healthy**: current (0.15.0), 73
crates.io dependents: the de-facto axum session standard and the maintained
successor to the abandoned `async-session` / `axum-sessions` lineage. The rot
is in the wrappers, not the primitive.

Reverie consumes only a thin slice of `axum-login`: the `AuthnBackend` and
`AuthUser` traits, the `AuthSession` extractor (login / logout / `.session` /
`.user`), and `AuthManagerLayer`. It does **not** use axum-login's
authorization layer (`AuthzBackend`, permissions, groups, `login_required!`):
role-based access control (`CurrentUser` with `require_admin` /
`require_not_child`) and the Basic-auth device-token fallback are already
first-party. Likewise the Postgres `SessionStore` is a four-method surface
(`load` / `save` / `delete` plus `ExpiredDeletion::delete_expired`) over a
table Reverie already owns end-to-end: the `tower_sessions` schema, its
migration, role grants, expiry index, and the hourly reaper
(`services::session_sweep`, which already hand-writes SQL against
`tower_sessions.session`).

Should Reverie keep depending on two abandoned single-maintainer wrappers on
its authentication-critical path, or replace that thin slice with first-party
code on the maintained `tower-sessions` core?

## Decision Drivers

- Unblock the coordinated upgrade permanently, not contingent on two unresponsive upstreams.
- Minimize abandoned / single-maintainer dependency surface on the
  auth-critical path: OpenSSF Scorecard `Maintained` signal and supply-chain
  hardening for an OSS, multi-user-exposed threat model.
- Preserve the OWASP Session Management invariants already in place and
  test-locked.
- Do not relocate the risk onto another small-org wrapper, and do not
  re-implement the one piece (the request/response session lifecycle) that is
  both healthy upstream and the most error-prone to hand-roll.
- Favour net removal: Reverie already owns the store's table and reaper.

## Considered Options

- **A1+A2: Keep `tower-sessions` core; replace `axum-login` and
  `tower-sessions-sqlx-store` with first-party code.**
- **F: Replace the stack with `axum-session` (AscendingCreations).**
- **G: Fully first-party, dropping `tower-sessions` core as well.**
- **Keep waiting / git-dep override `axum-login` to unreleased `main`.**

## Decision Outcome

Chosen option: **A1+A2**. Keep `tower-sessions` core on its maintained release
line; delete `axum-login` and `tower-sessions-sqlx-store`; reimplement the thin
slice Reverie uses as first-party code. This unblocks the 0.15 bump without
depending on either abandoned wrapper and takes the maxcountryman dependency
count on the auth path from four crates to one: the healthy one.

`axum-login` is replaced by session login / logout helpers on
`tower_sessions::Session` (login = `cycle_id()` then persist `user_id` and
`session_version`; logout = `flush()`), per-request user rehydration folded into
the existing `CurrentUser` extractor (read session → `user_id` → load user →
compare `session_version` for invalidation), and a direct call to the existing
OIDC upsert from the `/auth/callback` handler.
`tower-sessions-sqlx-store` is replaced by a first-party `SessionStore` +
`ExpiredDeletion` implementation against the unchanged `tower_sessions.session`
table.

The session-table schema, its RLS-exemption, the role grants (`reverie_app`
DML; `reverie_readonly` column-scoped `SELECT (id, expiry_date)`;
`reverie_ingestion` none), and the `expiry_date` index are **unchanged and
remain in force**: the first-party store targets the identical table, so the
data-layer decisions from the superseded sqlx-store ADR are carried forward
intact.

### Consequences

- Good, because the coordinated upgrade unblocks permanently: `tower-sessions` 0.15 lands
  without waiting on an `axum-login` 0.19 or a `tower-sessions-sqlx-store`
  release that may never come.
- Good, because the auth-critical path drops from four maxcountryman crates to
  one (the maintained core). Net code removal plus ~150–200 lines of
  first-party code mirroring patterns already in-tree.
- Good, because the `session_version` force-logout lever and the `cycle_id`
  fixation defence become explicit first-party code rather than indirection
  through axum-login's auth-hash machinery.
- Bad, because Reverie now owns the per-request session lifecycle and the
  store's serialization / expiry semantics. This is security-critical code;
  correctness rests on the existing HTTP-layer auth tests plus the store's
  restart-survival and expired-not-returned contract tests.
- Bad, because cutover invalidates all live sessions once: existing sessions
  store identity under axum-login's data key the new code will not read, so the
  deploy logs every user out a single time. Acceptable for a v0.x
  single-instance deployment.
- Neutral, because there is no session-table migration: schema and grants are
  unchanged.

### Confirmation

Fixation defence (`cycle_id` on login) and the `session_version` force-logout
lever stay covered by the existing HTTP-layer auth tests; the first-party
`SessionStore` keeps the restart-survival and expired-session contract tests.
No `axum-login` or `tower-sessions-sqlx-store` entry remains in
`backend/Cargo.toml`.

## Pros and Cons of the Options

### A1+A2: first-party on tower-sessions core

- Good, because it unblocks 0.15 with no dependency on either abandoned
  wrapper.
- Good, because it stays on the 73-dependent ecosystem standard for the hard
  part: the session-lifecycle middleware.
- Good, because it reuses the session table, grants, index, and reaper Reverie
  already owns.
- Neutral, because Reverie writes ~150–200 lines it did not before, mostly
  mirroring existing patterns (raw `Session` usage in `routes/auth.rs`, SQL in
  `session_sweep.rs`).
- Bad, because it owns security-critical session-lifecycle and store code.

### F: axum-session (AscendingCreations)

- Good, because it is actively maintained (0.20.1, 2026-05) and org-backed.
- Bad, because it relocates the abandonment risk: ~13 dependents versus
  `tower-sessions`' 73, roughly 5–10× less adoption → _more_ exposed to
  single-org abandonment, not less.
- Bad, because it is a full migration onto an unfamiliar API for no functional
  gain; Reverie uses the most basic session slice.

### G: fully first-party (drop core too)

- Good, because it leaves zero third-party session crates.
- Bad, because it hand-rolls the request/response session lifecycle: the exact
  place session-fixation and save-race bugs live, replacing a healthy
  maintained crate. Highest own-security-code for the weakest marginal risk
  reduction.

### Keep waiting / git-dep override

- Bad, because 11 + 17 months of upstream silence give no projected timeline. A
  git-dep override trades crates.io-stale for unreviewed-HEAD (cargo-audit and
  Renovate are blind to git deps) and still does not clear the sqlx-store pin.
  Rejected by the project's supply-chain posture.

## More Information

- The coordinated `tower-sessions` upgrade issue: the `tower-sessions`
  0.14 → 0.15 bump this decision unblocks. Implementation is tracked there and
  in `prp-plan` output under `.claude/PRPs/plans/`, not in this ADR.
- Supersedes
  [`superseded/2026-05-08-tower-sessions-sqlx-store.md`](superseded/2026-05-08-tower-sessions-sqlx-store.md)
  , that ADR adopted `tower-sessions-sqlx-store` and explicitly rejected a
  hand-written store; the context changed when the store became the second pin
  blocking the coordinated upgrade. Its session-table schema, grants, RLS-exemption, and
  expiry-index decisions are carried forward in the Decision Outcome above.
- The 0.14 pin was tracked in `debt/2026-05-21-tower-sessions-0-14-pin.md`
  (since purged: `git log --diff-filter=D -- debt/` recovers it): this
  decision's first-party replacement was its lift path, and PR #424 removed
  both `axum-login` and `tower-sessions-sqlx-store`, unpinning `tower-sessions`
  to 0.15.
- Adoption / health basis (2026-06-04): `tower-sessions` 0.15.0 (2026-02-01),
  73 dependents; `axum-login` 0.18.0 (2025-07-20), frozen;
  `tower-sessions-sqlx-store` 0.15.0 (2025-01-01), frozen; `axum-session`
  0.20.1 (2026-05-09), 13 dependents.
- Revisit trigger: if maintaining the first-party session lifecycle proves
  error-prone (recurring session bugs), reconsider adopting a maintained
  middleware.
- OWASP Session Management Cheat Sheet: the invariant set the first-party code
  must preserve (high-entropy CSPRNG id, rotation on login, `HttpOnly` /
  `SameSite=Lax`, idle expiry, server-side invalidation).
- `tower-sessions` upstream:
  <https://github.com/maxcountryman/tower-sessions>
