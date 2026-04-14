# Plan: OIDC Authentication and Device Tokens

## Summary

Implement the dual authentication model: OIDC (Authorization Code + PKCE) for browser
sessions and device tokens (argon2-hashed) for OPDS/reader apps. On successful OIDC
auth, upsert the user record. Device tokens are generated in the web UI and stored as
argon2 hashes. A `CurrentUser` extractor resolves identity from either mechanism and
populates request extensions. First user auto-promoted to admin.

## User Story

As a Tome user,
I want to authenticate via my OIDC provider (Authentik) for web access and generate
device tokens for my e-reader apps,
so that all my devices can access my RLS-scoped library securely.

## Problem -> Solution

**Current state:** Server has no authentication. All endpoints are open. `AppError::Unauthorized`
and `acquire_with_rls()` exist but are unused.

**Desired state:** OIDC login flow, device token CRUD, auth middleware that resolves
`CurrentUser` from either mechanism, first-user bootstrap to admin.

## Metadata

- **Complexity**: Large
- **Source PRD**: `/home/coder/Tome/plans/BLUEPRINT.md`
- **PRD Phase**: Step 3 — OIDC Authentication and Device Tokens
- **Estimated Files**: 14-16

---

## UX Design

N/A for this step — no frontend UI yet. Auth endpoints are API-only. The OIDC flow
is browser-based (redirects). Device token management will get a UI in Step 10.

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `backend/src/error.rs` | all | AppError — Unauthorized variant, IntoResponse |
| P0 | `backend/src/db.rs` | all | acquire_with_rls — first real consumer |
| P0 | `backend/src/state.rs` | all | AppState — will add OidcClient |
| P0 | `backend/src/config.rs` | all | Config — adding OIDC fields |
| P0 | `backend/src/main.rs` | all | Router setup, build_router, test_router |
| P0 | `backend/src/routes/health.rs` | all | Existing route pattern to mirror |
| P1 | `backend/migrations/20260412150002_core_tables.up.sql` | 1-22 | users table schema |
| P1 | `backend/migrations/20260412150004_user_features.up.sql` | 19-28 | device_tokens table schema |
| P1 | `backend/migrations/20260412150007_search_rls_and_reserved.up.sql` | 35-115 | RLS policies using current_setting |
| P2 | `plans/DESIGN_BRIEF.md` | 145-179 | Auth design decisions |
| P2 | `.env.example` | all | Env var patterns |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| openidconnect crate | docs.rs/openidconnect | CoreClient, AuthorizationCode flow, PKCE, custom HTTP client for testing |
| argon2 crate (RustCrypto) | docs.rs/argon2 | Argon2::default(), hash_password(password, &salt), verify_password |
| tower-sessions | docs.rs/tower-sessions | SessionManagerLayer, CookieConfig, Session extractor |
| base64ct | docs.rs/base64ct | Constant-time Base64 encoding for token generation |

---

## Patterns to Mirror

### CONFIG_PATTERN

```rust
// SOURCE: backend/src/config.rs:22-52
// Required vars fail fast with ConfigError::MissingVar.
// Optional vars use .unwrap_or_else(|_| "default".into()).
// Parsing errors use ConfigError::Invalid { var, reason }.
```

### ERROR_HANDLING

```rust
// SOURCE: backend/src/error.rs:4-16
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### ROUTE_PATTERN

```rust
// SOURCE: backend/src/routes/health.rs:9-13
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(ready))
}
```

### DB_RLS_PATTERN

```rust
// SOURCE: backend/src/db.rs:18-26
pub async fn acquire_with_rls(pool: &PgPool, user_id: uuid::Uuid)
    -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error>
// Uses: set_config('app.current_user_id', $1::text, true)
```

### STATE_PATTERN

```rust
// SOURCE: backend/src/state.rs:1-9
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}
```

### TEST_PATTERN

```rust
// SOURCE: backend/src/main.rs:67-91
// Unit tests use TestServer with a lazy pool (no real DB).
// DB-dependent tests use #[ignore] or run in CI with postgres service.
// Config constructed inline with test values.
```

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/Cargo.toml` | UPDATE | Add openidconnect, argon2, rand, tower-sessions, base64ct, reqwest, wiremock |
| `backend/src/config.rs` | UPDATE | Add OIDC config fields |
| `backend/src/state.rs` | UPDATE | Add OidcClient to AppState |
| `backend/src/error.rs` | UPDATE | Remove #[allow(dead_code)], add Forbidden variant |
| `backend/src/db.rs` | UPDATE | Remove #[allow(dead_code)] |
| `backend/src/auth/mod.rs` | CREATE | Re-exports for auth module |
| `backend/src/auth/oidc.rs` | CREATE | OIDC client setup, login/callback handlers |
| `backend/src/auth/token.rs` | CREATE | Device token generation, hashing, validation |
| `backend/src/auth/middleware.rs` | CREATE | CurrentUser extractor (session cookie OR Basic auth) |
| `backend/src/models/user.rs` | CREATE | User struct, upsert, find_by_id queries |
| `backend/src/models/device_token.rs` | CREATE | DeviceToken struct, CRUD queries |
| `backend/src/models/mod.rs` | UPDATE | Add user, device_token modules |
| `backend/src/routes/auth.rs` | CREATE | Auth route handlers (login, callback, logout, me) |
| `backend/src/routes/tokens.rs` | CREATE | Device token CRUD endpoints |
| `backend/src/routes/mod.rs` | UPDATE | Add auth, tokens modules |
| `backend/src/main.rs` | UPDATE | Wire OIDC client init, session layer, new routes |
| `.env.example` | UPDATE | Add OIDC config vars |

## NOT Building

- Frontend UI for login/token management (Step 10)
- Role-based authorization middleware beyond CurrentUser (future step)
- Session storage in database (cookie-only for single-instance deployment)
- Token refresh / sliding sessions (OIDC session is one-shot; cookie has fixed expiry)
- OPDS route protection (Step 9 — just the auth extractor)

---

## Step-by-Step Tasks

### Task 1: Add dependencies to Cargo.toml

- **ACTION**: Add auth-related crates
- **IMPLEMENT**: Add to `[dependencies]`:
  - `openidconnect = "4"` (OIDC client)
  - `argon2 = "0.5"` (password hashing, RustCrypto)
  - `rand = "0.9"` (cryptographic random for token generation)
  - `base64ct = { version = "1", features = ["std"] }` (constant-time base64)
  - `tower-sessions = "0.13"` (session middleware)
  - `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }` (HTTP client for OIDC discovery)
  Add to `[dev-dependencies]`:
  - `wiremock = "0.6"` (mock HTTP server for OIDC tests)
- **MIRROR**: Cargo.toml dep style (inline features, alphabetical)
- **IMPORTS**: N/A
- **GOTCHA**: `openidconnect` pulls in `oauth2` and `reqwest`. Use `reqwest` with `rustls-tls` not `native-tls` to match the sqlx TLS choice. Do NOT add `base64` — use `base64ct` for constant-time encoding.
- **VALIDATE**: `cargo check`

### Task 2: Add OIDC config fields

- **ACTION**: Extend `Config` with OIDC settings
- **IMPLEMENT**: Add fields:
  ```rust
  pub oidc_issuer_url: String,    // OIDC_ISSUER_URL, required
  pub oidc_client_id: String,     // OIDC_CLIENT_ID, required
  pub oidc_client_secret: String, // OIDC_CLIENT_SECRET, required
  pub oidc_redirect_uri: String,  // OIDC_REDIRECT_URI, required
  pub session_secret: String,     // SESSION_SECRET, required (32+ bytes hex)
  ```
  All required — fail fast with `ConfigError::MissingVar`. Update `.env.example` with commented examples.
- **MIRROR**: CONFIG_PATTERN
- **IMPORTS**: N/A
- **GOTCHA**: Update ALL test `Config` construction sites (main.rs test_router, config tests) with the new fields. Use empty strings in unit tests where OIDC isn't exercised.
- **VALIDATE**: `cargo check`, existing config tests still pass

### Task 3: Create `src/models/user.rs`

- **ACTION**: Define User model and database queries
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
  pub struct User {
      pub id: uuid::Uuid,
      pub oidc_subject: String,
      pub display_name: String,
      pub email: Option<String>,
      pub role: String,  // user_role enum as text
      pub is_child: bool,
      pub created_at: time::OffsetDateTime,
      pub updated_at: time::OffsetDateTime,
  }
  ```
  Queries:
  - `find_by_id(pool, id) -> Option<User>` — SELECT by primary key
  - `find_by_oidc_subject(pool, subject) -> Option<User>` — SELECT by oidc_subject
  - `upsert_from_oidc(pool, subject, display_name, email) -> User` — INSERT ON CONFLICT (oidc_subject) UPDATE display_name, email, updated_at. Returns the user.
  - `promote_if_first_user(pool, id) -> bool` — atomically promote to admin only if this is the sole user: `UPDATE users SET role = 'admin'::user_role WHERE id = $1 AND (SELECT count(*) FROM users) = 1`. Returns true if promoted. Avoids TOCTOU race from separate count + set_role calls.
- **MIRROR**: DB_RLS_PATTERN (use pool directly for auth queries — no RLS on user lookup itself, RLS is for content access)
- **IMPORTS**: `sqlx`, `uuid::Uuid`, `time::OffsetDateTime`, `serde::Serialize`
- **GOTCHA**: The `role` column is a Postgres enum `user_role`. Use `sqlx::Type` derive or query as text with cast. Simplest: `role::text` in SELECT, `$1::user_role` in INSERT/UPDATE. The `chk_child_role_sync` constraint means setting `role = 'child'` must also set `is_child = true` and vice versa.
- **VALIDATE**: Integration test (needs DB): upsert a user, find by subject, verify fields

### Task 4: Create `src/models/device_token.rs`

- **ACTION**: Define DeviceToken model and CRUD queries
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
  pub struct DeviceToken {
      pub id: uuid::Uuid,
      pub user_id: uuid::Uuid,
      pub name: String,
      #[serde(skip)]  // Never serialize the hash
      pub token_hash: String,
      pub last_used_at: Option<time::OffsetDateTime>,
      pub created_at: time::OffsetDateTime,
      pub revoked_at: Option<time::OffsetDateTime>,
  }
  ```
  Queries:
  - `create(pool, user_id, name, token_hash) -> DeviceToken`
  - `list_for_user(pool, user_id) -> Vec<DeviceToken>` — active (non-revoked) tokens only
  - `find_active_for_user(pool, user_id) -> Vec<DeviceToken>` — all non-revoked, for auth checking
  - `revoke(pool, id, user_id) -> bool` — SET revoked_at = now() WHERE id = $1 AND user_id = $2
  - `update_last_used(pool, id) -> ()` — SET last_used_at = now()
- **MIRROR**: Same query style as user.rs
- **IMPORTS**: `sqlx`, `uuid::Uuid`, `time::OffsetDateTime`, `serde::Serialize`
- **GOTCHA**: Never return `token_hash` in API responses — `#[serde(skip)]` on the field. The `list_for_user` query should exclude revoked tokens (WHERE revoked_at IS NULL). The `revoke` query must scope to user_id to prevent users revoking other users' tokens.
- **VALIDATE**: Integration test: create token, list, revoke, verify revoked not in list

### Task 5: Create `src/auth/token.rs`

- **ACTION**: Device token generation and verification logic
- **IMPLEMENT**:
  ```rust
  use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
  use argon2::password_hash::SaltString;
  use rand::rngs::OsRng;
  use base64ct::{Base64UrlUnpadded, Encoding};

  /// Generate a cryptographically random device token (32 bytes, base64url).
  /// Returns (plaintext_token, argon2_hash).
  pub fn generate_device_token() -> (String, String) {
      let mut bytes = [0u8; 32];
      rand::fill(&mut bytes);
      let plaintext = Base64UrlUnpadded::encode_string(&bytes);
      let salt = SaltString::generate(&mut OsRng);
      let hash = Argon2::default()
          .hash_password(plaintext.as_bytes(), &salt)
          .expect("argon2 hash failed")
          .to_string();
      (plaintext, hash)
  }

  /// Verify a plaintext token against a stored argon2 hash.
  pub fn verify_device_token(plaintext: &str, hash: &str) -> bool {
      let parsed = match PasswordHash::new(hash) {
          Ok(h) => h,
          Err(_) => return false,
      };
      Argon2::default()
          .verify_password(plaintext.as_bytes(), &parsed)
          .is_ok()
  }
  ```
- **MIRROR**: N/A — standalone crypto module
- **IMPORTS**: `argon2`, `rand`, `base64ct`
- **GOTCHA**: `rand` v0.9 provides `rand::fill(&mut bytes)`. Use `Argon2::default()` which is Argon2id — the recommended variant. Token verification is constant-time via argon2 internally.
- **VALIDATE**: Unit test: generate token, verify with correct plaintext succeeds, verify with wrong plaintext fails

### Task 6: Create `src/auth/oidc.rs`

- **ACTION**: OIDC client initialization and route handlers
- **IMPLEMENT**:
  ```rust
  use openidconnect::{
      CoreClient, CoreProviderMetadata, ClientId, ClientSecret,
      IssuerUrl, RedirectUrl, AuthorizationCode, CsrfToken,
      Nonce, PkceCodeChallenge, PkceCodeVerifier, Scope,
      TokenResponse, reqwest::async_http_client,
  };
  ```
  Functions:
  - `init_oidc_client(config) -> Result<CoreClient>` — discover provider metadata from `oidc_issuer_url`, create CoreClient with client_id, client_secret, redirect_uri.
  - Store PKCE verifier + CSRF token + nonce in the session during login.
  - Login handler: generate auth URL with PKCE, store verifier in session, redirect.
  - Callback handler: exchange code for tokens, validate ID token, extract subject + name + email, upsert user, check if first user (promote to admin), **regenerate session ID** (`session.cycle_id()` or equivalent) to prevent session fixation, set user_id in session, redirect to `/`.
- **MIRROR**: ROUTE_PATTERN for handler signatures, ERROR_HANDLING for error returns
- **IMPORTS**: `openidconnect`, `tower_sessions::Session`
- **GOTCHA**: `openidconnect` v4 uses `reqwest::async_http_client` as the default async HTTP client function. Provider discovery is async and can fail if the issuer URL is wrong — handle gracefully at startup (log error and exit, don't panic silently). Store PKCE verifier in session (it's needed during callback). The nonce must be validated against the ID token's nonce claim.
- **VALIDATE**: Unit test with wiremock: mock discovery endpoint, mock token endpoint, verify user upserted after callback

### Task 7: Create `src/auth/middleware.rs`

- **ACTION**: Create `CurrentUser` extractor
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone)]
  pub struct CurrentUser {
      pub user_id: uuid::Uuid,
      pub role: String,
      pub is_child: bool,
  }
  ```
  Implement `FromRequestParts<AppState>` for `CurrentUser`:
  1. Try session cookie: extract `user_id` from `Session`, load user from DB
  2. If no session, try `Authorization: Basic` header: username = user_id UUID, password = device token. Load active tokens for that user, verify against each with argon2.
  3. If neither succeeds, return `AppError::Unauthorized`
  
  On successful device token auth, update `last_used_at`.
- **MIRROR**: ERROR_HANDLING
- **IMPORTS**: `axum::extract::FromRequestParts`, `axum::http::request::Parts`, `tower_sessions::Session`
- **GOTCHA**: Basic auth username = user_id (UUID string). This scopes the token lookup to avoid O(n) argon2 checks. Parse the Authorization header manually or use `axum-extra`'s `TypedHeader<Authorization<Basic>>`. The argon2 verify is CPU-intensive — scope to the user's tokens (typically 1-3) not all tokens.
- **VALIDATE**: Integration test: create user + token, make request with Basic auth, verify CurrentUser resolved

### Task 8: Create `src/auth/mod.rs`

- **ACTION**: Module re-exports
- **IMPLEMENT**: `pub mod middleware; pub mod oidc; pub mod token;`
- **MIRROR**: routes/mod.rs pattern
- **IMPORTS**: N/A
- **GOTCHA**: None
- **VALIDATE**: `cargo check`

### Task 9: Create `src/routes/auth.rs`

- **ACTION**: Auth route handlers
- **IMPLEMENT**:
  - `GET /auth/login` — redirect to OIDC provider
  - `GET /auth/callback` — handle OIDC callback, upsert user, set session
  - `POST /auth/logout` — clear session cookie
  - `GET /auth/me` — return current user info (requires `CurrentUser` extractor)
  Routes grouped under `pub fn router() -> Router<AppState>`
- **MIRROR**: ROUTE_PATTERN
- **IMPORTS**: `crate::auth::oidc`, `crate::auth::middleware::CurrentUser`, `tower_sessions::Session`
- **GOTCHA**: `/auth/callback` must handle errors gracefully — if OIDC exchange fails, redirect to a login error page (or return JSON error for now). Don't panic on invalid state/code.
- **VALIDATE**: Integration test for `/auth/me` with device token auth. OIDC flow tested via wiremock.

### Task 10: Create `src/routes/tokens.rs`

- **ACTION**: Device token CRUD endpoints
- **IMPLEMENT**:
  - `POST /api/tokens` — create new device token. Requires `CurrentUser`. Body: `{ "name": "My Kindle" }`. Returns `{ "id": ..., "name": ..., "token": "<plaintext>" }` — plaintext shown ONCE.
  - `GET /api/tokens` — list user's active tokens. Requires `CurrentUser`. Returns array (no token_hash, no plaintext).
  - `DELETE /api/tokens/:id` — revoke a token. Requires `CurrentUser`. Scoped to user's own tokens.
  Routes grouped under `pub fn router() -> Router<AppState>`.
- **MIRROR**: ROUTE_PATTERN, ERROR_HANDLING
- **IMPORTS**: `crate::auth::middleware::CurrentUser`, `crate::auth::token`, `crate::models::device_token`
- **GOTCHA**: The plaintext token is returned ONLY on creation. After that, only the name/id/dates are visible. If a user loses their token, they must revoke and create a new one.
- **VALIDATE**: Integration test: create token (verify plaintext returned), list (verify no hash/plaintext), revoke, list again (verify removed)

### Task 11: Update `src/state.rs`

- **ACTION**: Add OidcClient to AppState
- **IMPLEMENT**:
  ```rust
  use openidconnect::core::CoreClient;
  
  #[derive(Clone)]
  pub struct AppState {
      pub pool: PgPool,
      pub config: Config,
      pub oidc_client: CoreClient,
  }
  ```
- **MIRROR**: STATE_PATTERN
- **IMPORTS**: `openidconnect::core::CoreClient`
- **GOTCHA**: `CoreClient` implements `Clone`. Update ALL test sites that construct AppState.
- **VALIDATE**: `cargo check`

### Task 12: Update `src/main.rs`

- **ACTION**: Wire OIDC client init, session layer, new routes
- **IMPLEMENT**:
  - Add `mod auth;` declaration
  - In `main()`: call `auth::oidc::init_oidc_client(&config).await` to create the client
  - Add `SessionManagerLayer` from tower-sessions with cookie config (signed, HttpOnly, SameSite=Lax). `Secure` flag intentionally omitted — backend runs behind a TLS-terminating reverse proxy and sees plain HTTP, so `Secure` would break cookies.
  - Merge `routes::auth::router()` and `routes::tokens::router()` into build_router
  - Update `build_router` to accept and layer session middleware
  - Update `test_router` with new AppState fields
- **MIRROR**: Existing main.rs structure
- **IMPORTS**: `tower_sessions`, `crate::auth`
- **GOTCHA**: Session layer must be added BEFORE routes that use Session. OIDC client init is async and can fail — `.expect()` is fine at startup (fail fast). The session secret from config is used to sign cookies.
- **VALIDATE**: `cargo build`, existing health test still passes

### Task 13: Update `.env.example`

- **ACTION**: Add OIDC and session config vars
- **IMPLEMENT**: Add commented examples:
  ```
  # --- Authentication (OIDC) ---
  OIDC_ISSUER_URL=https://auth.example.com/application/o/tome/
  OIDC_CLIENT_ID=tome
  OIDC_CLIENT_SECRET=your-client-secret-here
  OIDC_REDIRECT_URI=http://localhost:3000/auth/callback
  SESSION_SECRET=generate-a-random-64-char-hex-string
  ```
- **MIRROR**: Existing .env.example comment style
- **IMPORTS**: N/A
- **GOTCHA**: Don't put real secrets. Use placeholder values.
- **VALIDATE**: Visual check

### Task 14: Remove dead_code allows and add Forbidden variant

- **ACTION**: Clean up now that AppError and acquire_with_rls have real consumers
- **IMPLEMENT**:
  - Remove `#[allow(dead_code)]` from `AppError` in error.rs
  - Remove `#[allow(dead_code)]` from `acquire_with_rls` in db.rs
  - Add `AppError::Forbidden` variant mapping to 403 (needed for "not admin" checks)
- **MIRROR**: ERROR_HANDLING
- **IMPORTS**: N/A
- **GOTCHA**: Verify nothing else has dead_code warnings after removing the allows.
- **VALIDATE**: `cargo clippy -- -D warnings`

### Task 15: Write tests

- **ACTION**: Comprehensive unit and integration tests
- **IMPLEMENT**:
  - **Unit tests** (no DB):
    - `auth/token.rs`: generate_device_token produces valid base64url, verify succeeds with correct token, fails with wrong token
    - `error.rs`: Forbidden variant returns 403
  - **Integration tests** (DB required):
    - `models/user.rs`: upsert_from_oidc creates user, second call updates, find_by_id works, count returns correct value, first-user promotion
    - `models/device_token.rs`: create, list, revoke, list-after-revoke
    - `routes/tokens.rs`: full CRUD via HTTP (POST create, GET list, DELETE revoke) using device token auth
    - `routes/auth.rs`: GET /auth/me returns user info with device token auth, returns 401 without auth
    - `auth/middleware.rs`: CurrentUser resolves from Basic auth, rejects invalid token, rejects revoked token
  - **OIDC tests** (wiremock, no real provider):
    - Mock discovery endpoint, mock token exchange, verify user created after callback
- **MIRROR**: TEST_PATTERN
- **IMPORTS**: `axum_test`, `wiremock`, `sqlx`
- **GOTCHA**: wiremock tests need careful setup of OIDC discovery document JSON and token response JSON. Use `openidconnect`'s expected formats.
- **VALIDATE**: `cargo test` (unit), `cargo test -- --include-ignored` (all)

---

## Testing Strategy

### Unit Tests

| Test | Input | Expected Output | Edge Case? |
|---|---|---|---|
| `generate_device_token_format` | Call generate | 43-char base64url string + PHC hash string | No |
| `verify_correct_token` | Matching plaintext + hash | true | No |
| `verify_wrong_token` | Wrong plaintext + hash | false | No |
| `verify_malformed_hash` | Plaintext + garbage | false | Yes |
| `forbidden_returns_403` | `AppError::Forbidden` | 403 | No |

### Integration Tests (DB required)

| Test | Input | Expected Output | Edge Case? |
|---|---|---|---|
| `upsert_creates_user` | New OIDC subject | User with default role 'adult' | No |
| `upsert_updates_existing` | Same subject, new name | Updated display_name | No |
| `first_user_promoted_to_admin` | Upsert when count=0 | role='admin' | No |
| `create_and_list_tokens` | POST token, GET list | Token in list, no hash | No |
| `revoke_token` | DELETE token | Not in list | No |
| `auth_me_with_device_token` | GET /auth/me + Basic auth | 200 with user info | No |
| `auth_me_without_auth` | GET /auth/me | 401 | No |
| `auth_rejects_revoked_token` | Basic auth with revoked token | 401 | Yes |
| `auth_rejects_wrong_token` | Basic auth with bad password | 401 | Yes |
| `concurrent_first_users_only_one_admin` | Two concurrent upsert+promote flows | Exactly one admin exists | Yes |

### Edge Cases Checklist

- [ ] Revoked device token returns 401
- [ ] Wrong plaintext token returns 401
- [ ] Missing Authorization header + no session returns 401
- [ ] Invalid Basic auth format returns 401
- [ ] OIDC callback with invalid state/code returns error
- [ ] First user auto-promoted to admin
- [ ] Second user is NOT admin
- [ ] Token plaintext returned only on creation, never on list
- [ ] User cannot revoke another user's token

---

## Validation Commands

### Static Analysis

```bash
cd backend && cargo fmt --check
```

EXPECT: Zero formatting issues

```bash
cd backend && cargo clippy -- -D warnings
```

EXPECT: Zero warnings (dead_code allows removed)

### Unit Tests

```bash
cd backend && cargo test
```

EXPECT: All non-ignored tests pass

### Full Test Suite (with DB)

```bash
cd backend && cargo test -- --include-ignored
```

EXPECT: All tests pass (CI has postgres service)

### Manual Validation

- [ ] Configure a local Authentik/Keycloak with OIDC client
- [ ] Set OIDC env vars, start server
- [ ] Navigate to `/auth/login` — redirected to provider
- [ ] Complete login — redirected back, session cookie set
- [ ] `GET /auth/me` returns user info
- [ ] First user has role=admin
- [ ] `POST /api/tokens` with `{"name": "test"}` returns plaintext
- [ ] Use plaintext as Basic auth password with user_id as username
- [ ] `GET /auth/me` with Basic auth returns same user
- [ ] `DELETE /api/tokens/:id` revokes token
- [ ] Basic auth with revoked token returns 401

---

## Acceptance Criteria

- [ ] OIDC login flow works end-to-end with a real provider
- [ ] Device tokens: create, list, revoke, authenticate
- [ ] `CurrentUser` extractor resolves from session cookie OR Basic auth
- [ ] Unauthenticated requests to protected routes return 401
- [ ] First user auto-promoted to admin
- [ ] No secrets leak in error responses
- [ ] All clippy warnings resolved (no dead_code allows remaining)
- [ ] Tests cover device token auth thoroughly (OIDC tested via wiremock)

## Completion Checklist

- [ ] Code follows discovered patterns
- [ ] Error handling matches codebase style (thiserror + IntoResponse)
- [ ] Logging uses tracing with structured fields
- [ ] Tests follow axum-test pattern
- [ ] No hardcoded secrets (config from env)
- [ ] `.env.example` updated
- [ ] No unnecessary scope additions
- [ ] Self-contained — no questions needed during implementation

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `openidconnect` v4 API changes from my knowledge | Medium | Medium | Check docs.rs during implementation, use context7 if available |
| wiremock OIDC test setup is complex | Medium | Low | Start with device token tests (simpler), add OIDC tests incrementally |
| `tower-sessions` cookie signing API may differ between versions | Medium | Medium | Pin version, check docs for Key/SigningKey API |
| `rand` v0.9 API — verify `rand::fill` compiles on aarch64 | Low | Low | Resolved: pinned rand 0.9 in Task 1, code sample uses 0.9 API |
| Session cookie not set correctly (SameSite, Secure flags) | Low | High | Test manually with browser; log cookie attributes |

## Notes

- **Design brief says `axum-oidc-client`** but we're using `openidconnect` directly. `axum-oidc` is a thin wrapper that would obscure the user upsert and session logic we need to control. The `openidconnect` crate is mature and well-maintained.
- **Session storage: `tower-sessions` with `CookieStore`**. No server-side session table — the cookie itself holds the session data (PKCE verifier during login, `user_id` after auth). Cookies are signed using `session_secret` from config. Trade-off: sessions are lost if `session_secret` rotates (all users re-login), but no DB overhead for a single-instance app.
- **Basic auth username = user_id UUID**: OPDS clients send Basic auth. Username carries the user identity so we scope token lookup (avoiding O(n) argon2 checks across all tokens).
- **`base64ct` not `base64`**: Constant-time encoding prevents timing side-channels on token comparison. Minor but correct.
- **`time` not `chrono`**: Per project decision (memory: `project_time_not_chrono.md`).
