# Implementation Report

**Plan**: `.claude/PRPs/plans/completed/first-party-session-layer.plan.md`
**Linear**: UNK-101
**Branch**: `feat/unk-101-first-party-session-layer`
**Date**: 2026-06-04
**Status**: COMPLETE

---

## Summary

Replaced the two abandoned single-maintainer wrappers on Reverie's
auth-critical path — `axum-login` 0.18 and `tower-sessions-sqlx-store` 0.15 —
with first-party code on the maintained `tower-sessions` 0.15 core, implementing
ADR `2026-06-04-first-party-session-layer.md`. This unblocked the long-stuck
`tower-sessions` 0.14→0.15 bump (Renovate PR #128) without waiting on either
upstream. Net: 3 maxcountryman crates on the auth path → 1 (the maintained core).

---

## Assessment vs Reality

| Metric     | Predicted   | Actual      | Reasoning                                                                                  |
| ---------- | ----------- | ----------- | ----------------------------------------------------------------------------------------- |
| Complexity | MEDIUM-HIGH | MEDIUM      | tower-sessions 0.15 = zero API break (verified) made the store a near-verbatim port; the `AuthBackend.pool == state.pool` simplification removed the only architectural unknown. Wide-but-mechanical test ripple. |
| Confidence | 8.5/10      | matched     | Plan one-passed. No pivots; the three planning gates (RLS rehydration, missing force-logout test, call-site enumeration) and the dep-ordering fix all held at execution time. |

No deviation from the plan's approach. The only execution-time additions were a
single doc-backticks clippy fix (`MessagePack`) and `cargo fmt` normalization —
both expected mechanical polish, not design changes.

---

## Tasks Completed

| #   | Task                                                | Status |
| --- | --------------------------------------------------- | ------ |
| 1   | Force-logout E2E test (TDD, written first)          | ✅     |
| 2   | `Cargo.toml` additive (`rmp-serde`)                 | ✅ (folded into single dep change) |
| 3   | `auth/store.rs` — first-party `PostgresStore`       | ✅     |
| 4   | `auth/session.rs` — login/logout helpers            | ✅     |
| 5   | `models/user.rs` — strip `AuthUser` coupling        | ✅     |
| 6   | Delete `auth/backend.rs` + update `auth/mod.rs`     | ✅     |
| 7   | `middleware.rs` — rewrite `CurrentUser`, drop `AuthCtx` | ✅  |
| 8   | `routes/auth.rs` — handlers on `Session`, direct upsert | ✅  |
| 9   | `lib.rs` — router signature, layer stack, run() wiring | ✅   |
| 10  | `session_sweep.rs` — swap store type                | ✅     |
| 11  | Update remaining `build_router` / `AuthBackend` call sites | ✅ |
| 12  | Repoint store contract tests                        | ✅     |
| 13  | Ref-gate (`rg` proves no code refs remain)          | ✅     |
| 14  | `Cargo.toml` — remove dead crates, bump 0.15        | ✅     |
| 15  | Flip debt (`lifted`) + ADR (`accepted`) + README    | ✅     |
| 16  | Regenerate `.sqlx` + full gate                      | ✅     |

---

## Validation Results

| Check                  | Result | Details                                                        |
| ---------------------- | ------ | -------------------------------------------------------------- |
| fmt (`--check`)        | ✅     | clean                                                          |
| clippy (`-D warnings`) | ✅     | clean (1 doc-backticks fix applied)                            |
| Lib tests              | ✅     | 708 passed, 0 failed, 1 ignored (incl. new force-logout test)  |
| Integration tests      | ✅     | `cookie_jar_sanity`: 2 passed                                  |
| Build (workspace)      | ✅     | bin + lib compile                                              |
| `.sqlx` `--check`      | ✅     | cache current (6 new store query entries)                     |
| Dep assertions         | ✅     | no `axum-login`, no `tower-sessions-sqlx-store`; `tower-sessions` 0.15.0 |

---

## Files Changed

| File | Action |
| --- | --- |
| `backend/src/auth/store.rs` | CREATE |
| `backend/src/auth/session.rs` | CREATE |
| `backend/src/auth/backend.rs` | DELETE |
| `backend/src/auth/mod.rs` | UPDATE |
| `backend/src/auth/middleware.rs` | UPDATE |
| `backend/src/models/user.rs` | UPDATE |
| `backend/src/routes/auth.rs` | UPDATE (+ force-logout test) |
| `backend/src/lib.rs` | UPDATE |
| `backend/src/services/session_sweep.rs` | UPDATE |
| `backend/src/test_support.rs` | UPDATE |
| `backend/src/security/headers.rs` | UPDATE |
| `backend/src/routes/tokens.rs` | UPDATE |
| `backend/src/routes/library/tests.rs` | UPDATE |
| `backend/Cargo.toml` / `Cargo.lock` | UPDATE |
| `backend/.sqlx/*` | +6 query entries |
| `adr/2026-06-04-first-party-session-layer.md` / `adr/README.md` | UPDATE (accepted) |
| `debt/2026-05-21-tower-sessions-0-14-pin.md` | UPDATE (lifted) |

---

## Security review (Hard Rule 6)

OWASP Session Management invariants preserved and test-locked:

- **High-entropy CSPRNG id** — tower-sessions `Id::default` (CSPRNG `i128`); unchanged.
- **Rotation on login** — `auth::session::login` calls `cycle_id` (session-fixation test green).
- **`HttpOnly` + `SameSite=Lax`** — unchanged on `SessionManagerLayer`.
- **`Secure` flag** — explicit `.with_secure(false)` (0.15 defaults `true`; backend is behind a TLS-terminating proxy). Load-bearing; documented inline + in ADR scope.
- **Idle expiry** — `Expiry::OnInactivity(24h)`; unchanged.
- **Server-side invalidation** — `flush` on logout; `session_version` force-logout now first-party and end-to-end tested.
- **Store** — parameterised `query!` macros only (no runtime SQL, no allowlist entry); `load` filters `expiry_date > now()` (expired-not-returned test green).

Will this stand up to security review? Yes — the invariant set is unchanged from
the axum-login path, the one behavioural risk (force-logout enforcement moving
into `CurrentUser`) is now covered by an end-to-end test that was absent before,
and the `Secure` default flip is explicitly neutralised.

---

## Deviations from Plan

None of substance. Task 2 (additive `rmp-serde`) was folded into the single
`Cargo.toml` change since the implementation was compiled and validated as one
batch against the identical 0.15 API rather than in the plan's staged
0.14-then-0.15 sequence; the end state and the ref-gate (Task 13) are identical.

---

## Follow-up (not in scope)

- `backend/CLAUDE.md` Project Structure section still lists
  `backend.rs # axum-login AuthnBackend` (now deleted) and describes
  `middleware.rs` without the store/session split. Stale doc — flagged for the
  user (CLAUDE.md edits require explicit approval).

---

## Next Steps

- [ ] Push branch + open PR (`Closes UNK-101`)
- [ ] Pre-merge review (santa-method / prp-review per workflow)
- [ ] User reviews + merges (agents never merge)
