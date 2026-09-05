---
type: ADR
profile-version: 1
id: "REV-ADR-0015"
title: "First-party session layer on the tower-sessions core"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-04"
decision-makers:
  - "John Unkovich"
---

# First-party session layer on the tower-sessions core

## Context and problem statement

Reverie's session/auth stack rests on four crates by a single maintainer (maxcountryman): `tower-sessions` (with
`tower-sessions-core`), `tower-sessions-sqlx-store`, and `axum-login`. Two of those wrappers are abandoned on
crates.io:

- `axum-login` 0.18.0: last released 2025-07-20 (around 11 months). Peer-pins `tower-sessions = "0.14"`. The upstream
  fix bumping it to `tower-sessions` 0.15 merged 2026-05-07 (commit `151c72d`) but has never been released.
- `tower-sessions-sqlx-store` 0.15.0: last released 2025-01-01 (around 17 months). Its `tower-sessions-core`
  dependency pins `^0.14`.

These are two independent peer-pins that jointly block the coordinated `tower-sessions` 0.14 to 0.15 bump. Removing
one does not unblock the bump; the other still holds 0.14. `tower-sessions` 0.15 (released 2026-02-01) carries a
memory-ordering race fix and a `rand` 0.9 update worth picking up, and the blocked bump re-surfaces on every
dependency sweep at an ongoing human-attention cost.

By contrast, `tower-sessions` core is healthy: current (0.15.0), with 73 crates.io dependents, the de-facto axum
session standard and the maintained successor to the abandoned `async-session` / `axum-sessions` lineage. The rot is
in the wrappers, not the primitive.

Reverie consumes only a thin slice of `axum-login`: the `AuthnBackend` and `AuthUser` traits, the `AuthSession`
extractor (login, logout, `.session`, `.user`), and `AuthManagerLayer`. It does not use axum-login's authorization
layer (`AuthzBackend`, permissions, groups, `login_required!`): role-based access control (`CurrentUser` with
`require_admin` / `require_not_child`) and the Basic-auth device-token fallback are already first-party. Likewise the
Postgres `SessionStore` is a four-method surface (`load` / `save` / `delete` plus `ExpiredDeletion::delete_expired`)
over a table Reverie already owns end-to-end: the `tower_sessions` schema, its migration, role grants, expiry index,
and the hourly reaper (`services::session_sweep`, which already hand-writes SQL against `tower_sessions.session`).

Should Reverie keep depending on two abandoned single-maintainer wrappers on its authentication-critical path, or
replace that thin slice with first-party code on the maintained `tower-sessions` core?

## Decision drivers

- Unblock the coordinated upgrade permanently, not contingent on two unresponsive upstreams.
- Minimise abandoned / single-maintainer dependency surface on the auth-critical path: the OpenSSF Scorecard
  `Maintained` signal and supply-chain hardening for an OSS, multi-user-exposed threat model.
- Preserve the OWASP Session Management invariants already in place and test-locked.
- Do not relocate the risk onto another small-org wrapper, and do not re-implement the one piece (the
  request/response session lifecycle) that is both healthy upstream and the most error-prone to hand-roll.
- Favour net removal: Reverie already owns the store's table and reaper.

## Considered options

- A1+A2: keep `tower-sessions` core; replace `axum-login` and `tower-sessions-sqlx-store` with first-party code
- F: replace the stack with `axum-session` (AscendingCreations)
- G: fully first-party, dropping `tower-sessions` core as well
- Keep waiting / git-dep override `axum-login` to unreleased `main`

## Decision outcome

Chosen option: **A1+A2: keep `tower-sessions` core; replace `axum-login` and `tower-sessions-sqlx-store` with
first-party code**, because it unblocks the 0.15 bump without depending on either abandoned wrapper and takes
the maxcountryman dependency count on the auth path from four crates to one: the healthy one. `tower-sessions` core
stays on its maintained release line; `axum-login` and `tower-sessions-sqlx-store` are deleted; Reverie reimplements
the thin slice it uses as first-party code.

`axum-login` is replaced by session login / logout helpers on `tower_sessions::Session` (login: `cycle_id()` then
persist `user_id` and `session_version`; logout: `flush()`), per-request user rehydration folded into the existing
`CurrentUser` extractor (read session, then `user_id`, then load user, then compare `session_version` for
invalidation), and a direct call to the existing OIDC upsert from the `/auth/callback` handler.
`tower-sessions-sqlx-store` is replaced by a first-party `SessionStore` + `ExpiredDeletion` implementation against the
unchanged `tower_sessions.session` table.

The session-table schema, its RLS-exemption, the role grants (`reverie_app` DML; `reverie_readonly` column-scoped
`SELECT (id, expiry_date)`; `reverie_ingestion` none), and the `expiry_date` index are unchanged and remain in force:
the first-party store targets the identical table, so the data-layer decisions from the earlier
tower-sessions-sqlx-store decision are carried forward intact.

### Consequences

- Positive: the coordinated upgrade unblocks permanently: `tower-sessions` 0.15 lands without waiting on an
  `axum-login` 0.19 or a `tower-sessions-sqlx-store` release that may never come.
- Positive: the auth-critical path drops from four maxcountryman crates to one (the maintained core). Net code
  removal plus around 150-200 lines of first-party code mirroring patterns already in-tree.
- Positive: the `session_version` force-logout lever and the `cycle_id` fixation defence become explicit first-party
  code rather than indirection through axum-login's auth-hash machinery.
- Positive: no additional migration: the session table's schema and grants are unchanged, since the first-party
  store targets the same table.
- Positive: `backend/Cargo.toml` carries no `axum-login` or `tower-sessions-sqlx-store` entry; only `tower-sessions`
  remains on the auth-critical path.
- Negative: Reverie now owns the per-request session lifecycle and the store's serialization / expiry semantics.
  This is security-critical code; correctness rests on the existing HTTP-layer auth tests plus the store's
  restart-survival and expired-not-returned contract tests.
- Negative: cutover invalidates all live sessions once: existing sessions store identity under axum-login's data key
  the new code will not read, so the deploy logs every user out a single time. Acceptable for a v0.x
  single-instance deployment.

## Pros and cons of the options

### A1+A2: keep `tower-sessions` core; replace `axum-login` and `tower-sessions-sqlx-store` with first-party code

- Positive: it unblocks 0.15 with no dependency on either abandoned wrapper.
- Positive: it stays on the 73-dependent ecosystem standard for the hard part: the session-lifecycle middleware.
- Positive: it reuses the session table, grants, index, and reaper Reverie already owns.
- Neutral: Reverie writes around 150-200 lines it did not before, mostly mirroring existing patterns (raw `Session`
  usage in `routes/auth.rs`, SQL in `session_sweep.rs`).
- Negative: it owns security-critical session-lifecycle and store code.

### F: replace the stack with `axum-session` (AscendingCreations)

- Positive: it is actively maintained (0.20.1, 2026-05) and org-backed.
- Negative: it relocates the abandonment risk: around 13 dependents versus `tower-sessions`' 73, roughly 5-10x less
  adoption, so it is more exposed to single-org abandonment, not less.
- Negative: it is a full migration onto an unfamiliar API for no functional gain; Reverie uses the most basic
  session slice.

### G: fully first-party, dropping `tower-sessions` core as well

- Positive: it leaves zero third-party session crates.
- Negative: it hand-rolls the request/response session lifecycle, the exact place session-fixation and save-race
  bugs live, replacing a healthy maintained crate. Highest own-security-code for the weakest marginal risk
  reduction.

### Keep waiting / git-dep override `axum-login` to unreleased `main`

- Negative: 11 plus 17 months of upstream silence give no projected timeline. A git-dep override trades
  crates.io-stale for unreviewed-HEAD (dependency auditing and update tooling are blind to git deps) and still does
  not clear the sqlx-store pin. Rejected by the project's supply-chain posture.

## More information

No row-level security applies to the session table. The earlier tower-sessions-sqlx-store decision recorded why:
`SessionStore::load` runs before any auth context exists, since the cookie's session id is the bootstrap that
resolves the user, so "RLS-gating the session lookup is chicken-and-egg. Access is enforced at the role-grant
boundary instead."

A Redis-backed session store (`tower-sessions-redis-store`) was rejected for the same reason the earlier
tower-sessions-sqlx-store decision rejected it: it "adds a second persistence dependency to the deployment" per
[Single-image distribution with central CSP enforcement](./0003-single-image-distribution-with-central-csp-enforcement.md),
a second piece of infrastructure the self-hoster install path does not have.

Adoption / health basis (2026-06-04): `tower-sessions` 0.15.0 (2026-02-01), 73 dependents; `axum-login` 0.18.0
(2025-07-20), frozen; `tower-sessions-sqlx-store` 0.15.0 (2025-01-01), frozen; `axum-session` 0.20.1 (2026-05-09),
13 dependents.

Revisit trigger: if maintaining the first-party session lifecycle proves error-prone (recurring session bugs),
reconsider adopting a maintained middleware.

OWASP Session Management Cheat Sheet: the invariant set the first-party code must preserve (high-entropy CSPRNG id,
rotation on login, `HttpOnly` / `SameSite=Lax`, idle expiry, server-side invalidation).

`tower-sessions` upstream: <https://github.com/maxcountryman/tower-sessions>
