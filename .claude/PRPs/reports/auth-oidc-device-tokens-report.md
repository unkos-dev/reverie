# Implementation Report: OIDC Authentication and Device Tokens

## Summary
Implemented dual authentication: OIDC (Authorization Code + PKCE) for browser sessions and device tokens (argon2-hashed) for OPDS/reader apps. CurrentUser extractor resolves identity from either mechanism. First user auto-promoted to admin.

## Assessment vs Reality

| Metric | Predicted (Plan) | Actual |
|---|---|---|
| Complexity | Large | Large |
| Confidence | High | High |
| Files Changed | 14-16 | 17 (11 modified, 6 created) |

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 1 | Add dependencies | done | rand 0.10 not 0.9; openidconnect needs `reqwest` feature; tower-sessions bumped to 0.15 (0.13 incompatible with axum 0.8) |
| 2 | Add OIDC config fields | done | |
| 3 | Create models/user.rs | done | |
| 4 | Create models/device_token.rs | done | |
| 5 | Create auth/token.rs | done | OsRng from argon2::password_hash::rand_core (rand_core 0.6 ecosystem) |
| 6 | Create auth/oidc.rs | done | Full OidcClient type alias needed due to set_redirect_uri state machine |
| 7 | Create auth/middleware.rs | done | |
| 8 | Create auth/mod.rs | done | |
| 9 | Create routes/auth.rs | done | exchange_code returns Result in v4 |
| 10 | Create routes/tokens.rs | done | |
| 11 | Update state.rs | done | |
| 12 | Update main.rs | done | MemoryStore instead of CookieStore; Expiry::OnInactivity |
| 13 | Update .env.example | done | |
| 14 | Remove dead_code + add Forbidden | done | Some #[allow(dead_code)] retained with documented reasons |
| 15 | Write tests | done | Unit tests only; integration tests require DB |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| Format (cargo fmt) | Pass | |
| Clippy (-D warnings) | Pass | Zero warnings |
| Unit Tests | Pass | 14 tests (4 new token/error tests) |
| Build | Pass | |
| Integration | N/A | DB-dependent tests marked #[ignore] |

## Files Changed

| File | Action | Lines |
|---|---|---|
| `backend/Cargo.toml` | UPDATED | +7 deps |
| `backend/src/config.rs` | UPDATED | +5 fields, updated all test sites |
| `backend/src/state.rs` | UPDATED | +OidcClient field |
| `backend/src/error.rs` | UPDATED | +Forbidden variant, removed dead_code allow |
| `backend/src/db.rs` | UPDATED | removed dead_code allow (re-added with documented reason) |
| `backend/src/main.rs` | UPDATED | session layer, new routes, test OidcClient helper |
| `backend/src/models/mod.rs` | UPDATED | +user, device_token modules |
| `backend/src/routes/mod.rs` | UPDATED | +auth, tokens modules |
| `backend/src/auth/mod.rs` | CREATED | Module re-exports |
| `backend/src/auth/oidc.rs` | CREATED | OidcClient type, init_oidc_client, exchange_http_client |
| `backend/src/auth/token.rs` | CREATED | generate_device_token, verify_device_token + 4 tests |
| `backend/src/auth/middleware.rs` | CREATED | CurrentUser extractor (session + Basic auth) |
| `backend/src/models/user.rs` | CREATED | User struct, CRUD queries + integration test |
| `backend/src/models/device_token.rs` | CREATED | DeviceToken struct, CRUD queries + integration test |
| `backend/src/routes/auth.rs` | CREATED | login, callback, logout, me handlers |
| `backend/src/routes/tokens.rs` | CREATED | create, list, revoke handlers |
| `.env.example` | UPDATED | +OIDC config vars |

## Deviations from Plan

1. **rand version**: Plan specified `0.9`, used `0.10` (lockfile already had 0.10.1)
2. **tower-sessions version**: Plan specified `0.13`, used `0.15` (0.13 uses axum-core 0.4, incompatible with axum 0.8)
3. **No CookieStore**: tower-sessions 0.15 removed CookieStore; using MemoryStore (acceptable for single-instance)
4. **OsRng source**: Used `argon2::password_hash::rand_core::OsRng` instead of `rand::rngs::OsRng` (rand_core version mismatch)
5. **openidconnect v4 API**: No `async_http_client` free function; uses `ClientBuilder::new().build()` struct instead
6. **OidcClient type alias**: Needed full type alias due to `set_redirect_uri` changing type parameters

## Tests Written

| Test File | Tests | Coverage |
|---|---|---|
| `auth/token.rs` | 4 | generate format, verify correct/wrong/malformed |
| `error.rs` | 1 (new) | Forbidden returns 403 |
| `models/user.rs` | 1 (ignored) | upsert, update, find_by_id, find_by_subject |
| `models/device_token.rs` | 1 (ignored) | create, list, revoke lifecycle |
| `main.rs` | 1 (existing) | health endpoint still passes |

## Next Steps
- [ ] Code review via `/code-review`
- [ ] Create PR via `/prp-pr`
