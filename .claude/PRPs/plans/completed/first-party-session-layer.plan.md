# Feature: First-party session layer (drop axum-login + tower-sessions-sqlx-store)

## Summary

Replace the two abandoned single-maintainer crates on Reverie's auth-critical
path — `axum-login` 0.18 and `tower-sessions-sqlx-store` 0.15 — with ~150–200
lines of first-party code on the maintained `tower-sessions` **0.15** core. This
unblocks the long-stuck `tower-sessions` 0.14→0.15 bump (UNK-101 / Renovate
PR #128) permanently, without waiting on either upstream. Implements ADR
`adr/2026-06-04-first-party-session-layer.md` (option A1+A2).

Two replacements:

- **A1 (axum-login):** a first-party `SessionStore` + `ExpiredDeletion` Postgres
  implementation targeting the **unchanged** `tower_sessions.session` table.
- **A2 (axum-login):** session login/logout helpers on `tower_sessions::Session`
  (login = `cycle_id()` + persist `user_id`/`session_version`; logout =
  `flush()`), per-request user rehydration folded into the existing
  `CurrentUser` extractor, and a direct OIDC upsert call from `/auth/callback`.

## User Story

As a Reverie operator / maintainer
I want the session/auth stack to depend only on maintained crates
So that security patches land without waiting on abandoned upstreams, the
OpenSSF `Maintained` signal stays green, and Renovate stops re-surfacing a
permanently-blocked bump.

## Problem Statement

`tower-sessions` 0.14→0.15 cannot be bumped: `axum-login` 0.18 peer-pins
`tower-sessions = "0.14"` (fix merged upstream 2026-05-07, never released, ~11mo
silent) and `tower-sessions-sqlx-store` 0.15 pins `tower-sessions-core ^0.14`
(~17mo silent). Two independent peer-pins jointly block the bump; removing one
leaves the other holding 0.14. 0.15 carries a `MemoryStore` memory-ordering race
fix and a `rand` 0.9 update. Testable end state: `tower-sessions = "0.15"` in
`backend/Cargo.toml`, **no** `axum-login` or `tower-sessions-sqlx-store` entry,
and the full test suite green.

## Solution Statement

`tower-sessions` 0.15 has **zero breaking API changes** vs 0.14 (verified
against the v0.15.0 source: `SessionStore`/`ExpiredDeletion`/`Record`/`Id`/
`Session`/`SessionManagerLayer` signatures all identical; only `rand` 0.8→0.9
inside `Id::default` and a `MemoryStore` internal fix). So the first-party store
is a near-verbatim port of `tower-sessions-sqlx-store`'s `postgres_store.rs`
(same `rmp_serde` MessagePack envelope, same three-column table) minus its
migration machinery — Reverie already owns the table, grants, index, and reaper.
The axum-login slice Reverie uses is thin (no `AuthzBackend`/permissions/groups);
it collapses into direct `tower_sessions::Session` calls plus a
`session_version` comparison in `CurrentUser`.

Crucially, `AuthBackend { pool }` (lib.rs:299) already clones the **same** pool
that becomes `state.pool` (both `reverie_app`), and `users`/`device_tokens`
carry **no RLS** — so the upsert and rehydration both run on `state.pool` with
no new `AppState` field and no privilege change. `AuthBackend` is deleted whole.

## Metadata

| Field            | Value                                                                                   |
| ---------------- | --------------------------------------------------------------------------------------- |
| Type             | REFACTOR (dependency removal + first-party reimplementation)                            |
| Complexity       | MEDIUM-HIGH (security-critical path; wide test call-site ripple)                        |
| Systems Affected | `auth`, session store, router assembly, OIDC callback, session sweep                    |
| Dependencies     | tower-sessions 0.15.x, rmp-serde 1.x, async-trait 0.1 (present), sqlx 0.8.6 (unchanged) |
| Estimated Tasks  | 16                                                                                      |
| Linear           | UNK-101 — `Closes UNK-101` in PR body                                                   |

---

## UX Design

No end-user UX change. The only observable effect is a one-time mass logout on
deploy (existing sessions store identity under axum-login's `"axum-login.data"`
key the new code never reads). ADR-accepted for a v0.x single-instance deploy.

### Before State

```
┌──────────┐  cookie "id"   ┌─────────────────────────────────────────────┐
│ Browser  │ ─────────────► │ SessionManagerLayer (tower-sessions 0.14)    │
└──────────┘                │   load row ─► tower-sessions-sqlx-store      │
                            │              PostgresStore (rmp_serde)        │
                            │   └─► AuthManager (axum-login 0.18)           │
                            │         read "axum-login.data" {user_id,      │
                            │         auth_hash} ─► get_user ─► ct_eq       │
                            │         auth_hash vs session_version bytes    │
                            │         ─► insert AuthSession into extensions │
                            └───────────────┬──────────────────────────────┘
                                            ▼
                                   CurrentUser reads auth_session.user
   DATA_FLOW: cookie → store load → axum-login rehydrate+compare → CurrentUser
   PAIN_POINT: 3 abandoned crates on the auth path; 0.15 bump blocked
```

### After State

```
┌──────────┐  cookie "id"   ┌─────────────────────────────────────────────┐
│ Browser  │ ─────────────► │ SessionManagerLayer (tower-sessions 0.15)    │
└──────────┘                │   load row ─► auth::store::PostgresStore      │
                            │              (FIRST-PARTY, rmp_serde)         │
                            │   └─► insert Session into extensions          │
                            └───────────────┬──────────────────────────────┘
                                            ▼
                            CurrentUser extractor (FIRST-PARTY rehydrate):
                              Session.get("user_id") ─► find_by_id(state.pool)
                              ─► compare session.get("session_version") == user.session_version
                              ─► mismatch: flush() + Unauthorized
   DATA_FLOW: cookie → first-party store load → CurrentUser rehydrate+compare
   VALUE_ADD: 1 maintained crate (tower-sessions core); 0.15 unblocked;
              session_version force-logout is now explicit first-party code
```

### Interaction Changes

| Location                  | Before                                      | After                                              | Impact                         |
| ------------------------- | ------------------------------------------- | -------------------------------------------------- | ------------------------------ |
| `routes/auth.rs` handlers | take `AuthCtx` (`AuthSession<AuthBackend>`) | take `tower_sessions::Session`                     | direct session I/O, no wrapper |
| `/auth/callback`          | `auth_session.authenticate()` + `.login()`  | `upsert_…(&state.pool,…)` + `auth::session::login` | upsert is a direct call        |
| `CurrentUser`             | reads `auth_session.user`                   | reads `Session` + reloads user + version compare   | force-logout is first-party    |
| `build_router`            | `(state, auth_backend)`                     | `(state)`                                          | all call sites updated         |

---

## Mandatory Reading

Implementation agent MUST read these before starting:

| Priority | File                                                      | Lines                                                                 | Why                                                                                             |
| -------- | --------------------------------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| P0       | `adr/2026-06-04-first-party-session-layer.md`             | all                                                                   | The decision being implemented; invariants to preserve                                          |
| P0       | `backend/src/lib.rs`                                      | 62–166, 280–360                                                       | `build_router` / `build_router_with_session_store` / layer stack / `run()` wiring + sweep spawn |
| P0       | `backend/src/auth/middleware.rs`                          | 1–230                                                                 | `CurrentUser`, `AuthCtx` alias, `verify_basic` — the extractor to rewrite                       |
| P0       | `backend/src/routes/auth.rs`                              | 43–226                                                                | login/callback/logout/me handlers using `AuthCtx`                                               |
| P0       | `backend/src/auth/backend.rs`                             | all                                                                   | `AuthBackend` + `OidcCredentials` (to delete); upsert call shape                                |
| P1       | `backend/src/models/user.rs`                              | 1–145, 174 (`upsert_from_oidc_and_maybe_promote`), 111 (`find_by_id`) | `AuthUser` impl + `session_version_bytes` to remove; upsert/find signatures                     |
| P1       | `backend/src/services/session_sweep.rs`                   | all                                                                   | `ExpiredDeletion` driver — swap store type only                                                 |
| P1       | `backend/CLAUDE.md`                                       | "Database" + "Testing" sections                                       | `query!` macro convention, `.sqlx` cache, `#[sqlx::test]` role pools                            |
| P2       | `backend/migrations/20260526000000_initial_schema.up.sql` | session DDL + grants (search `tower_sessions`)                        | table shape (id text, data bytea, expiry_date timestamptz) — UNCHANGED                          |
| P2       | `backend/src/test_support.rs`                             | 160–180, 360–440                                                      | test router builders + `AuthBackend` ctors to update                                            |

### External Documentation

| Source                                                                                                                                             | Section                                    | Why                                                                                                |
| -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| [tower-sessions v0.15.0 `session_store.rs`](https://github.com/maxcountryman/tower-sessions/blob/v0.15.0/tower-sessions-core/src/session_store.rs) | `SessionStore`, `ExpiredDeletion`, `Error` | Exact trait signatures (`#[async_trait]`, `create(&mut Record)`, `Error::{Encode,Decode,Backend}`) |
| [tower-sessions v0.15.0 `session.rs`](https://github.com/maxcountryman/tower-sessions/blob/v0.15.0/tower-sessions-core/src/session.rs)             | `Record`, `Id`, `Session`                  | `Record{id,data,expiry_date:OffsetDateTime}`, `Id(i128)`, `cycle_id()->Result`, `flush()`          |
| [tower-sessions-stores `postgres_store.rs`](https://github.com/maxcountryman/tower-sessions-stores/blob/main/sqlx-store/src/postgres_store.rs)     | create/save/load/delete/delete_expired     | The reference impl to port (rmp_serde, SQL)                                                        |
| [tower-sessions v0.15.0 release](https://github.com/maxcountryman/tower-sessions/releases/tag/v0.15.0)                                             | changelog                                  | Confirms zero breaking API changes; rand 0.9 + MemoryStore fix only                                |

---

## Patterns to Mirror

**SESSION-STORE REFERENCE (port this, drop migrate machinery):**

```rust
// SOURCE: tower-sessions-stores sqlx-store/src/postgres_store.rs (v0.15-pairing)
// create(): loop to guarantee no id collision, then upsert.
#[async_trait]
impl SessionStore for PostgresStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        // loop: while id_exists(record.id)? { record.id = Id::default(); }
        // then save_with_conn (INSERT ... ON CONFLICT DO UPDATE)
    }
    async fn save(&self, record: &Record) -> session_store::Result<()> { /* upsert */ }
    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        // SELECT data WHERE id=$1 AND expiry_date > now(); rmp_serde::from_slice
    }
    async fn delete(&self, id: &Id) -> session_store::Result<()> { /* DELETE WHERE id=$1 */ }
}
#[async_trait]
impl ExpiredDeletion for PostgresStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        // DELETE FROM tower_sessions.session WHERE expiry_date < now()
    }
}
```

**SQLX MACRO + CROSS-SCHEMA TABLE (project convention, UNK-167):**

```rust
// SOURCE: backend/src/models/user.rs:111-123 — query_as! / query! with explicit columns
// The store's SQL targets the qualified table tower_sessions.session and must
// use query!/query_scalar! (NOT runtime sqlx::query) to stay off the
// .github/sqlx-runtime-allowlist.txt. Bind Id as TEXT via record.id.to_string();
// bind expiry_date: OffsetDateTime directly (sqlx 0.8 `time` feature).
sqlx::query!("INSERT INTO tower_sessions.session (id, data, expiry_date) \
              VALUES ($1, $2, $3) ON CONFLICT (id) DO UPDATE \
              SET data = excluded.data, expiry_date = excluded.expiry_date",
    id_str, &data_bytes, record.expiry_date)
```

**ERROR MAPPING:**

```rust
// session_store::Error has 3 String variants. Map:
//   sqlx::Error      -> Error::Backend(e.to_string())
//   rmp_serde encode -> Error::Encode(e.to_string())
//   rmp_serde decode -> Error::Decode(e.to_string())
// Do NOT use unwrap/expect (clippy::unwrap_used = deny in non-test code).
```

**SWEEP DRIVER (unchanged except store type):**

```rust
// SOURCE: backend/src/services/session_sweep.rs — swap PostgresStore import to
// crate::auth::store::PostgresStore. sweep_once/run_sweep bodies unchanged:
pub async fn sweep_once(store: &PostgresStore) -> Result<(), session_store::Error> {
    store.delete_expired().await
}
```

**LOGIN/LOGOUT HELPERS (new — auth/session.rs):**

```rust
// login: rotate id (fixation defence), then persist identity claims.
pub async fn login(session: &Session, user: &User) -> Result<(), session::Error> {
    session.cycle_id().await?;                       // unconditional, single login path
    session.insert("user_id", user.id).await?;
    session.insert("session_version", user.session_version).await?;
    Ok(())
}
// logout: clear + delete server-side + drop cookie.
pub async fn logout(session: &Session) -> Result<(), session::Error> {
    session.flush().await
}
```

**REHYDRATION IN CurrentUser (new):**

```rust
// SOURCE pattern: middleware.rs verify_basic uses find_by_id(&state.pool, id).
// Read Session from extensions (populated by SessionManagerLayer), then:
let session = Session::from_request_parts(parts, state).await.ok();
if let Some(session) = session
   && let Some(uid) = session.get::<Uuid>("user_id").await?
   && let Some(user) = user::find_by_id(&state.pool, uid).await?  // users has no RLS
{
    let stored_ver = session.get::<i32>("session_version").await?;
    if stored_ver == Some(user.session_version) {                 // plain ==; server-trusted
        return Ok(Self { user_id: user.id, role: user.role, is_child: user.is_child });
    }
    // force-logout: stale version -> wipe session row. No silent discard
    // (backend/CLAUDE.md hard rule): log on failure, then fall through.
    if let Err(e) = session.flush().await {
        tracing::warn!(error = %e, "force-logout flush failed");
    }
}
// NOTE: Session::from_request_parts(...).ok() above is fine — absence of a
// session is expected ("no cookie"), not a swallowed error.
// fall through to verify_basic (device tokens), else Unauthorized
```

---

## Files to Change

| File                                          | Action | Justification                                                                                                                                                                                                              |
| --------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend/Cargo.toml`                          | UPDATE | Add `rmp-serde` EARLY (Task 2, additive); remove `axum-login` + `tower-sessions-sqlx-store` and bump `tower-sessions` 0.14→0.15 LATE (Task 14, after ref-gate)                                                             |
| `backend/src/auth/store.rs`                   | CREATE | First-party `PostgresStore`: `SessionStore` + `ExpiredDeletion` on `tower_sessions.session`                                                                                                                                |
| `backend/src/auth/session.rs`                 | CREATE | `login` / `logout` helpers on `tower_sessions::Session`                                                                                                                                                                    |
| `backend/src/auth/backend.rs`                 | DELETE | `AuthBackend` + `OidcCredentials` + `AuthnBackend` impl no longer needed                                                                                                                                                   |
| `backend/src/auth/mod.rs`                     | UPDATE | Drop `pub mod backend;`; add `pub mod store;` + `pub mod session;`; rewrite `//!` (no axum-login refs)                                                                                                                     |
| `backend/src/auth/middleware.rs`              | UPDATE | Rewrite `CurrentUser::from_request_parts` (Session-based rehydrate); delete `AuthCtx` alias; keep `verify_basic`                                                                                                           |
| `backend/src/models/user.rs`                  | UPDATE | Remove `AuthUser` impl + `session_version_bytes` field/caching; keep `session_version: i32`, `upsert_…`, `find_by_id`                                                                                                      |
| `backend/src/routes/auth.rs`                  | UPDATE | Handlers take `Session`; callback does direct upsert + `auth::session::login`; logout uses `auth::session::logout`; update tests                                                                                           |
| `backend/src/lib.rs`                          | UPDATE | `build_router`/`build_router_with_session_store` drop `auth_backend` param; remove `AuthManagerLayerBuilder` (keep `SessionManagerLayer`); use first-party store; remove AuthBackend at L299; sweep uses first-party store |
| `backend/src/services/session_sweep.rs`       | UPDATE | Swap `PostgresStore` import to `crate::auth::store::PostgresStore`                                                                                                                                                         |
| `backend/src/test_support.rs`                 | UPDATE | Drop `AuthBackend` ctors; update `build_router*` calls                                                                                                                                                                     |
| `backend/src/security/headers.rs`             | UPDATE | Drop `AuthBackend` ctor in test; update `build_router_with_session_store` call                                                                                                                                             |
| `backend/src/routes/tokens.rs`                | UPDATE | Drop `AuthBackend` ctor in test; update `build_router` call                                                                                                                                                                |
| `backend/src/routes/library/tests.rs`         | UPDATE | Drop `AuthBackend` ctor; update `build_router` call                                                                                                                                                                        |
| `backend/.sqlx/`                              | UPDATE | Regenerate offline cache for new store `query!` macros                                                                                                                                                                     |
| `debt/2026-05-21-tower-sessions-0-14-pin.md`  | UPDATE | Flip `status: lifted` (constraint removed by this PR)                                                                                                                                                                      |
| `adr/2026-06-04-first-party-session-layer.md` | UPDATE | Flip `status: proposed → accepted` (ships with implementation)                                                                                                                                                             |
| `adr/README.md`                               | UPDATE | Reflect accepted status in index entry                                                                                                                                                                                     |

---

## NOT Building (Scope Limits)

- **No session-table migration.** Schema, grants, RLS-exemption, expiry index
  are carried forward unchanged (ADR). Do not touch the migration.
- **No `AuthzBackend` / permissions / groups / `login_required!`.** Reverie
  never used axum-login's authz layer; RBAC (`require_admin`/`require_not_child`)
  is already first-party and stays.
- **No change to the device-token Basic-auth fallback** beyond it remaining the
  second branch of `CurrentUser`.
- **No `sqlx` 0.9 bump.** Separate constraint (`debt/2026-06-02-sqlx-0-9-blocked.md`),
  out of scope. Stay on 0.8.6.
- **No `with_secure(true)` / cookie-name change.** Preserve current cookie config
  (`http_only`, `SameSite::Lax`, `OnInactivity(24h)`, default name `"id"`, no
  Secure — TLS-terminating proxy). Default `with_secure` is `true` in 0.15, so
  the builder MUST keep an explicit `.with_secure(false)` to preserve behaviour
  behind the proxy.
- **No migration of live sessions.** One-time mass logout on deploy is accepted.

---

## Step-by-Step Tasks

Execute in order. Each is independently verifiable. `{B}` = run from `backend/`.

### Task 1: Write the force-logout integration test FIRST (TDD, currently uncovered)

- **ACTION**: ADD an HTTP-layer test proving session_version force-logout, since
  no existing test covers "bump session_version in DB → next session request is
  rejected" (only DB-column-bump is tested today in `routes/users/tests.rs`).
- **IMPLEMENT**: in `routes/auth.rs` tests (mirror the OIDC E2E + shared
  `MemoryStore` pattern at `routes/auth.rs:519-702`): drive a full login, hit an
  authenticated endpoint (e.g. `/auth/me`) → 200; `UPDATE users SET
session_version = session_version + 1 WHERE id=$1`; hit `/auth/me` again →
  expect 401.
- **MIRROR**: `routes/auth.rs:519-702` (MockOidcProvider + shared store).
- **GOTCHA**: this test must FAIL to compile/pass until Tasks 3–8 land (it
  references the new behaviour). Keep it; it is the spec for force-logout.
- **VALIDATE**: `cargo test -p reverie_api force_logout 2>&1 | tail` — present and
  red (compile error or assertion) before implementation.

> **ORDERING (load-bearing):** tower-sessions 0.15's API is byte-identical to
> 0.14 (research-verified). So ALL new code is written and compiled against the
> still-pinned 0.14 first; an unreferenced `axum-login`/`tower-sessions-sqlx-store`
> in `Cargo.toml` is a dormant dep, not a compile error. The dep removal + 0.15
> bump (Task 14) lands ONLY after `rg` proves zero remaining references — so
> every `cargo check` gate below is green, with no multi-task red valley. The
> sole early Cargo.toml change is additive (`rmp-serde`, Task 2).

### Task 2: `Cargo.toml` — additive only (`rmp-serde`)

- **ACTION**: add `rmp-serde = "1"` to `[dependencies]`. Do NOT remove
  `axum-login` or `tower-sessions-sqlx-store`, do NOT bump `tower-sessions` yet.
- **GOTCHA**: `rmp-serde` is already in the tree transitively (via sqlx-store);
  promoting it to a direct dep is harmless and lets `store.rs` compile now.
  `async-trait` (0.1.89) already direct — reuse it.
- **VALIDATE**: `cargo check -p reverie_api` (still on tower-sessions 0.14).

### Task 3: CREATE `backend/src/auth/store.rs` — first-party `PostgresStore`

- **ACTION**: implement `#[derive(Clone, Debug)] pub struct PostgresStore {
pool: PgPool }` with `pub fn new(pool: PgPool)`, `#[async_trait] impl
SessionStore` (create/save/load/delete) and `#[async_trait] impl
ExpiredDeletion` (delete_expired), targeting `tower_sessions.session`.
- **IMPLEMENT**: port the reference `postgres_store.rs` logic. `create` MUST loop
  on id-existence (`SELECT EXISTS(SELECT 1 FROM tower_sessions.session WHERE
id=$1)`) regenerating `record.id = Id::default()` on collision, then upsert.
  `save`/`create` upsert via `INSERT … ON CONFLICT (id) DO UPDATE`. `load` filters
  `expiry_date > now()` and `rmp_serde::from_slice::<Record>`. Bind id as
  `record.id.to_string()`; on load parse back with `Id::from_str`.
- **MIRROR**: SQL shape from analyst trace; macro style from `models/user.rs`.
- **IMPORTS**: `tower_sessions::session_store::{self, ExpiredDeletion, SessionStore}`,
  `tower_sessions::session::{Id, Record}`, `async_trait::async_trait`, `sqlx::PgPool`.
- **GOTCHA**: use `query!`/`query_scalar!` macros (UNK-167), NOT runtime
  `sqlx::query` — avoids an allowlist entry. No `unwrap`/`expect`. Map errors to
  `session_store::Error::{Backend,Encode,Decode}`. Tier-2 docstrings + threat
  notes (cross-schema, RLS-exempt rationale → `backend/CLAUDE.md`).
- **VALIDATE**: `cargo check -p reverie_api` with dev DB up (the `query!` macros
  validate against the live `tower_sessions.session` table; `.sqlx` cache is
  regenerated later in Task 16).

### Task 4: CREATE `backend/src/auth/session.rs` — login/logout helpers

- **ACTION**: `pub async fn login(session: &Session, user: &User)` =
  `cycle_id().await?` then insert `user_id` (Uuid) + `session_version` (i32);
  `pub async fn logout(session: &Session)` = `session.flush().await`.
- **MIRROR**: raw `Session` usage already in `routes/auth.rs` (insert/get/remove).
- **IMPORTS**: `tower_sessions::Session`, `crate::models::user::User`.
- **GOTCHA**: `cycle_id` is called once, unconditionally (single login path);
  returns `Result`. Tier-2 docstrings: state the fixation-defence threat.
- **VALIDATE**: `cargo check -p reverie_api`.

### Task 5: `models/user.rs` — strip axum-login coupling

- **ACTION**: delete `impl AuthUser for User`, the `session_version_bytes` field,
  its `#[serde(skip)]`, and the caching in `From<UserRow>`. Keep
  `pub session_version: i32`, `upsert_from_oidc_and_maybe_promote`, `find_by_id`.
- **GOTCHA**: a unit test asserts `session_version_bytes` (≈ `user.rs:327`) —
  remove that assertion line. Update module `//!` doc (remove AuthUser mention).
- **VALIDATE**: `cargo check -p reverie_api`; `rg session_version_bytes src/` empty.

### Task 6: DELETE `backend/src/auth/backend.rs` + update `auth/mod.rs`

- **ACTION**: delete the file; in `auth/mod.rs` remove `pub mod backend;`, add
  `pub mod store;` and `pub mod session;`, rewrite the `//!` header to drop all
  axum-login references and describe the first-party store + helpers.
- **GOTCHA**: `OidcCredentials` lived here and is used by callback — its fields
  (subject/display_name/email) move inline into the callback's direct upsert call
  (Task 8). Confirm no other importer: `rg "auth::backend|OidcCredentials" src/`.
- **VALIDATE**: `rg "mod backend|axum_login|AuthnBackend" src/` returns nothing.

### Task 7: `middleware.rs` — rewrite `CurrentUser`, drop `AuthCtx`

- **ACTION**: delete `pub type AuthCtx = AuthSession<AuthBackend>;` and the
  axum-login import. Rewrite `CurrentUser::from_request_parts`: extract
  `tower_sessions::Session`, read `user_id` → `find_by_id(&state.pool, …)` →
  compare `session_version` (`==`) → on match build `CurrentUser`; on mismatch
  `session.flush()` then fall through; else `verify_basic` fallback; else
  `Unauthorized`. Keep `require_admin`/`require_not_child`/`verify_basic`.
- **MIRROR**: existing two-branch structure (`middleware.rs:196-222`).
- **IMPORTS**: `tower_sessions::Session`; remove `axum_login::AuthSession`.
- **GOTCHA**: Session-extract failure (no layer) must not 500 — treat as "no
  session", fall through. Preserve THREAT comments. `?`-propagate session/db
  errors as `AppError::Internal`.
- **VALIDATE**: `cargo check -p reverie_api`.

### Task 8: `routes/auth.rs` — handlers on `Session`; direct upsert

- **ACTION**: change `login`/`callback`/`logout`/`me` handler signatures from
  `AuthCtx` to `Session` (and `State<AppState>` where needed). `callback`:
  validate OIDC state from session, exchange/verify, call
  `user::upsert_from_oidc_and_maybe_promote(&state.pool, subject, display_name,
email)` directly, then `auth::session::login(&session, &user)`, remove
  single-use keys (`pkce_verifier`/`oidc_csrf_state`/`nonce`), insert
  `csrf_token`. `logout`: `auth::session::logout(&session)`. `me`: read
  `csrf_token` from session.
- **MIRROR**: existing handler bodies (only the wrapper type + login/upsert call
  change). Session key names stay identical except the dropped
  `"axum-login.data"`.
- **GOTCHA**: `authenticate()` returned `Option` — the direct upsert returns
  `Result<User, sqlx::Error>`; map error to `AppError::Internal`. Update the
  in-file tests that construct `AuthBackend` + `build_router_with_session_store`.
- **VALIDATE**: `cargo check -p reverie_api`.

### Task 9: `lib.rs` — router signature + layer stack + run() wiring

- **ACTION**: `build_router(state: AppState) -> Router` and
  `build_router_with_session_store<S>(state, session_store)` drop the
  `auth_backend` param. Replace `PostgresStore` import with
  `crate::auth::store::PostgresStore`. In `build_router`, construct
  `PostgresStore::new(state.pool.clone())`. In `build_router_with_session_store`,
  remove `AuthManagerLayerBuilder`; apply `SessionManagerLayer` directly where
  `auth_layer` was, preserving `.with_http_only(true)`, `.with_same_site(Lax)`,
  `.with_secure(false)`, `.with_expiry(OnInactivity(24h))`. In `run()`, delete the
  `AuthBackend` line (299) and the `auth_backend` arg to `build_router`; swap the
  sweep store to the first-party `PostgresStore`.
- **GOTCHA**: layer ORDER unchanged — `SessionManagerLayer` sits exactly where
  `auth_layer` sat (must wrap inner services; outermost is still `TraceLayer`).
  `with_secure` default is `true` in 0.15 — the explicit `false` is load-bearing.
  Update module `//!` doc references to `AuthBackend`.
- **VALIDATE**: `cargo check -p reverie_api`.

### Task 10: `services/session_sweep.rs` — swap store type

- **ACTION**: change `use tower_sessions_sqlx_store::PostgresStore;` to
  `use crate::auth::store::PostgresStore;`. Bodies unchanged.
- **VALIDATE**: `cargo check -p reverie_api`; existing sweep tests still target
  the trait.

### Task 11: Update remaining `build_router` / `AuthBackend` call sites

- **ACTION**: in `test_support.rs` (≈169,172,392,395,431,434),
  `security/headers.rs` (≈323,326), `routes/tokens.rs` (≈175,176),
  `routes/library/tests.rs` (≈43,46): delete `AuthBackend { … }` ctors and drop
  the `auth_backend` arg from `build_router*` calls.
- **GOTCHA**: these are mostly test helpers; ensure the shared-`MemoryStore`
  test path (`build_router_with_session_store(state, store)`) still compiles.
- **VALIDATE**: `cargo check -p reverie_api --tests`; `rg "AuthBackend" src/`
  returns nothing.

### Task 12: Repoint store contract tests at first-party store

- **ACTION**: `session_record_survives_store_restart` and
  `expired_session_is_not_returned` (`lib.rs:710-783`) currently build
  `tower_sessions_sqlx_store::PostgresStore` — repoint to
  `crate::auth::store::PostgresStore`. Keep assertions identical.
- **GOTCHA**: `Record`/`Id` come from `tower_sessions` (unchanged). The first
  store-drop-then-reload semantics must still hold (rows live only in the table).
- **VALIDATE**: `cargo test -p reverie_api session_record_survives_store_restart
expired_session_is_not_returned`.

### Task 13: GATE — prove zero remaining references (precondition for dep removal)

- **ACTION**: confirm no source still references the crates about to be removed.
- **VALIDATE**: each returns NOTHING:
  `rg "axum_login|axum-login" backend/src/`,
  `rg "tower_sessions_sqlx_store" backend/src/`,
  `rg "AuthCtx|AuthSession|AuthBackend|AuthnBackend|AuthUser" backend/src/`.
- **GOTCHA**: a non-empty result means an earlier task missed a site — fix it
  BEFORE Task 14; removing the deps with a live reference is the only way to turn
  this refactor's checkpoints red. This `rg` also catches any AuthCtx/AuthSession
  usage outside the two files traced during planning (insurance against an
  incomplete enumeration).

### Task 14: `Cargo.toml` — remove dead crates + bump `tower-sessions` 0.15

- **ACTION**: remove `axum-login = "0.18"`; remove the
  `tower-sessions-sqlx-store` block (and its multi-line comment); set
  `tower-sessions = "0.15"`. Replace the removed comment with one short line
  noting tower-sessions core is kept (the maintained primitive) per ADR
  `2026-06-04-first-party-session-layer.md`.
- **GOTCHA**: only reachable cleanly because Task 13 is green. `rand` (0.10.1)
  direct dep is unrelated to tower-sessions' internal `rand` 0.9 — leave it.
- **VALIDATE**: `cargo build -p reverie_api --locked` compiles;
  `cargo tree -p reverie_api -i axum-login` → not found;
  `cargo tree -p reverie_api -i tower-sessions-sqlx-store` → not found;
  `cargo tree -p reverie_api -i tower-sessions` → 0.15.x.

### Task 15: Flip debt + ADR status

- **ACTION**: `debt/2026-05-21-tower-sessions-0-14-pin.md` → `status: lifted`
  (add lift note: replaced by first-party layer, this PR). ADR
  `2026-06-04-first-party-session-layer.md` front matter `status: proposed` →
  `accepted`; update `adr/README.md` index row.
- **GOTCHA**: per memory `feedback_adr_status_flip_at_merge`, the flip ships in
  the SAME PR as implementation. Do NOT touch
  `debt/2026-06-02-sqlx-0-9-blocked.md` (separate constraint).
- **VALIDATE**: `rg "status:" debt/2026-05-21-tower-sessions-0-14-pin.md
adr/2026-06-04-first-party-session-layer.md`.

### Task 16: Regenerate `.sqlx` offline cache + full gate

- **ACTION**: `DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev
cargo sqlx prepare -- --tests` from `backend/`; stage new `.sqlx/` entries for
  the store queries.
- **GOTCHA**: dev DB must be up + migrated (`docker compose up -d`; seed roles +
  `cargo run -- migrate` per `backend/CLAUDE.md`). CI runs
  `cargo sqlx prepare --check` — stale cache fails the build.
- **VALIDATE**: `cargo sqlx prepare --check -- --tests` exits 0; then the full
  gate in "Validation Commands".

---

## Testing Strategy

### Tests to write / preserve

| Test                                                           | Status                               | Validates                                    |
| -------------------------------------------------------------- | ------------------------------------ | -------------------------------------------- |
| force-logout E2E (`/auth/me` 401 after `session_version` bump) | NEW (Task 1)                         | first-party version-compare enforcement      |
| session-fixation (id rotates across login)                     | PRESERVE (`routes/auth.rs:610-619`)  | `cycle_id` on login                          |
| OIDC callback E2E happy path                                   | PRESERVE (`routes/auth.rs:519-702`)  | login flow + direct upsert                   |
| `session_record_survives_store_restart`                        | REPOINT (Task 12)                    | first-party store persistence                |
| `expired_session_is_not_returned`                              | REPOINT (Task 12)                    | first-party `load` expiry filter             |
| sweep deletes expired / keeps live                             | PRESERVE (`session_sweep.rs:70-132`) | `delete_expired`                             |
| `session_version` DB-bump tests                                | PRESERVE (`routes/users/tests.rs`)   | bump side (complements new enforcement test) |

### Edge Cases Checklist

- [ ] No session cookie → falls through to Basic → Unauthorized (no 500)
- [ ] `user_id` present but user deleted → mismatch path → flush + Unauthorized
- [ ] `session_version` absent in session (legacy/partial) → treated as mismatch
- [ ] Expired session row → `load` returns `None` → Unauthorized
- [ ] Logout deletes the row server-side (next request with old cookie → 401)
- [ ] Device-token Basic auth still works with no session present

---

## Validation Commands

Dev DB up + migrated first (`docker compose up -d`; seed roles; `cargo run --
migrate`). All `{B}` = from `backend/`.

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

EXPECT: exit 0, no warnings.

### Level 2: TARGETED TESTS

```bash
cargo test -p reverie_api auth:: session sweep force_logout
```

EXPECT: new force-logout test green; store contract + fixation tests green.

### Level 3: FULL SUITE + BUILD

```bash
cargo sqlx prepare --check -- --tests
cargo test --workspace --locked
cargo build --workspace --locked
```

EXPECT: all pass; `.sqlx` cache current.

### Level 4: DEPENDENCY ASSERTIONS

```bash
cargo tree -p reverie_api -i axum-login            # -> error: not found (good)
cargo tree -p reverie_api -i tower-sessions-sqlx-store  # -> error: not found (good)
cargo tree -p reverie_api -i tower-sessions        # -> 0.15.x
```

### Level 5: SECURITY REVIEW (Hard Rule 6)

- [ ] OWASP session invariants preserved: high-entropy CSPRNG id (tower-sessions
      `Id::default`), rotation on login (`cycle_id`), `HttpOnly`+`SameSite=Lax`,
      idle expiry (24h `OnInactivity`), server-side invalidation
      (`flush`/`session_version`).
- [ ] `.with_secure(false)` retained (proxy-terminated TLS) — documented.
- [ ] Store SQL injection-safe (parameterised macros only).
- [ ] No new secret surfaced; consult `.claude/security/` session/auth notes.

---

## Acceptance Criteria

- [ ] `tower-sessions = "0.15"`; no `axum-login` / `tower-sessions-sqlx-store` in
      `Cargo.toml` or the tree (Level 4).
- [ ] First-party `PostgresStore` passes restart-survival + expired-not-returned.
- [ ] Force-logout E2E test green (new enforcement covered).
- [ ] Session fixation + OIDC callback E2E green.
- [ ] Level 1–3 exit 0; `.sqlx` cache committed and `--check`-clean.
- [ ] Code mirrors existing patterns (macro SQL, error mapping, Tier-2 docs).
- [ ] Debt entry `lifted`; ADR `accepted`; `Closes UNK-101` in PR body.

---

## Risks and Mitigations

| Risk                                                           | Likelihood | Impact | Mitigation                                                                           |
| -------------------------------------------------------------- | ---------- | ------ | ------------------------------------------------------------------------------------ |
| Force-logout enforcement silently lost (was inside axum-login) | MED        | HIGH   | Task 1 writes the E2E test FIRST; acceptance-gated                                   |
| Rehydration logs valid users out (RLS)                         | LOW        | HIGH   | Verified `users` has no RLS; rehydrate on `state.pool` (same pool verify_basic uses) |
| `.with_secure` default flip (true in 0.15) breaks proxy login  | MED        | HIGH   | Explicit `.with_secure(false)`; called out in Task 9 + NOT-Building                  |
| `create()` id-collision path mishandled                        | LOW        | MED    | Port the existence-loop verbatim (Task 3); default trait impl is non-collision-safe  |
| Stale `.sqlx` cache fails CI                                   | MED        | LOW    | Task 14 regenerate + `--check` in Level 3                                            |
| Wide test call-site ripple missed                              | MED        | MED    | Gate-3 enumerated all sites; Task 11 + `rg AuthBackend` empty check                  |

## Notes

- **Confidence: 8.5/10** for one-pass success. tower-sessions 0.15 = no API
  break (verified against v0.15.0 source), the store is a near-verbatim port, and
  the pool/RLS question is resolved (single `reverie_app` pool, non-RLS tables).
  The −1.5 is the security-critical hand-off of force-logout enforcement (now
  test-gated) and the breadth of test call-site edits.
- ADR Confirmation said force-logout "stays covered by existing HTTP-layer auth
  tests" — that is only half true (DB-bump is tested; end-to-end enforcement was
  not). Task 1 closes that gap; no ADR edit needed beyond the status flip.
- The `AuthBackend` "schema-owner / bypasses RLS" doc comment was inaccurate
  (it cloned `state.pool` = `reverie_app`); deleting the struct removes the
  misleading comment too. No behaviour change — `users`/`device_tokens` are
  non-RLS.
