---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0003"
title: "Authorization axes"
satisfies:
  - "REV-REQ-0005"
  - "REV-REQ-0006"
governed-by:
  - "REV-ADR-0028"
---

# Authorization axes

Every `/api/v1` operation is authorised on three separate axes: scope (what a credential may do), role and child status
(who the caller is), and ownership (which rows the caller may touch). This Design covers the scope hierarchy and the
role-derived ceiling on it (`backend/src/auth/scope.rs`), the test matrix that proves every operation declares a scope
and is gated at and one level below it (`backend/src/authz_matrix.rs`), and the map of which resources enforce ownership
through row-level security and which through a handler-level ownership predicate.

## Purpose and boundaries

This subject owns the `Scope` enum and its ordering; the role-to-scope ceiling that bounds what a token may be minted to
carry; the matrix that parses the in-process OpenAPI document and asserts, for every `/api/v1` operation, that a scope
requirement is declared and enforced one level below it; and the inventory of which resources enforce ownership through
Postgres row-level security, reached through `crate::db::acquire_with_rls`, and which through an explicit
`WHERE user_id = …` predicate in the handler.

It does not own how a request becomes a `CurrentUser`. Session-cookie rehydration, `Authorization: Basic` and `Bearer`
parsing, and JWT signature, issuer and audience validation live in `backend/src/auth/middleware.rs` and
`backend/src/auth/jwt.rs`, a neighbouring subject with no Design yet; this Design names them only as call sites. It does
not own device-token minting beyond the ceiling it supplies (`Scope::grantable_by`): the token row, the per-user cap and
revocation live in `backend/src/models/device_token.rs` and `backend/src/routes/tokens.rs`, also without a Design yet.
It does not own row-level security itself, meaning the `app.current_user_id` setting, the policies that read it and the
roles they apply to; that is the Design "Row-level security and database context". This Design states only which
resources rely on that mechanism for ownership.

Depends on: `CurrentUser` in `backend/src/auth/middleware.rs` for the `role`, `is_child` and `scopes` fields its
assertion methods read; `crate::openapi::spec_json` for the OpenAPI document the matrix parses; the `device_token` model
for minting a scoped credential in tests; `crate::db::acquire_with_rls` for the RLS half of the ownership map.

Depended on by: every `/api/v1` handler that needs more than `read` scope or an adult caller, each of which calls
`require_scope`, `require_admin` or `require_not_child` before touching its resource (a `read` operation may omit the
call, because authentication already refuses a credential with no scopes; `get_reading` and `get_preferences` make it
anyway); the mint handler in `backend/src/routes/tokens.rs`, which reads `may_grant_scope` to bound a requested token;
and the backend test suite, which runs the matrix as an ordinary `#[sqlx::test]` module.

## Structure

- `backend/src/auth/scope.rs` declares `Scope { Read, Write, Admin }` with a derived `Ord`, so the hierarchy
  `Read < Write < Admin` is a comparison rather than a lookup table. `Scope::for_role` gives every role `{Read, Write}`
  and gives `Role::Admin` `Admin` as well. `Scope::grantable_by` is the mint-time ceiling: any role may grant `Read` or
  `Write`, and only a caller holding `Role::Admin` may grant `Admin`.
- `CurrentUser` in `backend/src/auth/middleware.rs` holds `role`, `is_child` and `scopes` as private fields, reachable
  only through `require_admin`, `require_not_child`, `require_scope` and `may_grant_scope`. `resolve_device_token`
  builds a `CurrentUser` from two separate reads: the device-token row for its stored `scopes`, and the owning user's
  row for its current `role`. `resolve_jwt` does the same for a validated RFC 9068 access token: `resolve_jwt_scopes`
  clamps the claimed `scope` list to `Scope::for_role(role)` (unknown names are dropped, an absent claim carries the
  full role ceiling, and a claim that narrows to nothing is rejected), and `role` again comes from the current user row.
  The session path, `CurrentUser::from_request_parts`, derives `scopes` from `Scope::for_role(user.role)` on every
  request, because a session carries no scope of its own.
- `backend/src/authz_matrix.rs` is the enforcement grid. `parse_operations` reads the `/api/v1` operations from the spec
  that `crate::openapi::spec_json()` renders in-process, not from the committed `backend/openapi.json`, so the grid
  tests the annotations the running server serves. `required_scope` takes the highest scope in an operation's declared
  array; the array is the same for every credential transport in a `security(...)` annotation, so reading the first
  alternative is enough. `call_with_scoped_token` mints a token holding exactly the scopes under test, sends one request
  and revokes the token straight away, which keeps a full sweep under the ten-active-token cap in
  `device_token::create_with_limit`. `substitute_path_params` replaces every `{param}` segment with a fixed nil UUID;
  because every gated handler checks scope before any database lookup, a non-existent id still reaches the gate instead
  of returning `404` first. `body_for` supplies a minimal valid JSON body for each mutating operation, since the `Json`
  and `ApiJson` extractors reject a missing or malformed body before the handler runs and would otherwise put their own
  status where the matrix expects the gate's `403`.
- `METHOD_LINT_ALLOWLIST` in `authz_matrix.rs` exempts one operation from the mutating-verb lint
  (`mutating_verb_ops_require_write_scope`): `POST /api/v1/manifestations/{id}/enrichment/dry-run`, which declares
  `read`. It previews an enrichment run: it calls the metadata providers and stores their responses in `api_cache`, but
  changes no manifestation, metadata version or writeback job. The lint records which allow-list entries matched a
  parsed operation (`allowlist_seen`) and fails if an entry matches nothing, so a renamed or removed route cannot leave
  a stale exemption behind.
- `create_token` in `backend/src/routes/tokens.rs` is the only place `Scope::grantable_by` is read at mint time. It
  checks each requested scope in `CreateTokenRequest.scopes` against `current_user.may_grant_scope(scope)` and rejects
  the whole request with `Forbidden`, logged, at the first one that exceeds the ceiling. `CreateTokenRequest` accepts
  exactly `name`, `scopes` and `expires_in_days`; nothing else on the `device_tokens` row can be set at mint.
- Twelve handlers make up the admin surface, and each calls both `current_user.require_scope(Scope::Admin)` and
  `current_user.require_admin()` before doing anything else: in `routes::users`, `list_users`, `update_role`,
  `update_child_status`, `create_user`, `update_account_status`, `admin_reset_password` and `update_user`; in
  `routes::settings`, `get_settings` and `put_settings`; in `routes::dashboard`, `stats` and `activity`; and
  `routes::ingestion::scan`. The two calls read different `CurrentUser` fields (`scopes` and `role`) and fail
  independently; Runtime behaviour shows why both are needed.
- **Ownership map.** `backend/src/routes/shelves/mod.rs` documents its own boundary: `shelves` and `shelf_items` have no
  row-level-security policy, so every mutating handler in that module enforces ownership with an explicit
  `WHERE id = $1 AND user_id = $2` predicate, and the reorder endpoint takes a `FOR UPDATE` lock under the same
  predicate against a concurrent write. A mismatched id resolves to `AppError::NotFound`, never to a response that says
  the shelf exists but belongs to someone else. The one exception inside that module is the manifestation probe in
  `add_shelf_item`, which opens an RLS-scoped transaction through `crate::db::acquire_with_rls` so a child account
  cannot use a shelf-add attempt to learn about a manifestation its visibility rules hide. Every other resource with a
  per-user ownership rule reaches its tables through `crate::db::acquire_with_rls`, so ownership there is a database
  policy: books and manifestations (`routes::library::mod`, `routes::library::search`, `routes::suggest`), reading state
  (`routes::reading`), metadata (`routes::metadata`), enrichment (`routes::enrichment`), account preferences
  (`routes::preferences::mod`), series (`routes::series::mod`) and the OPDS surface
  (`routes::opds::{shelves, download, library, root, opensearch}`). `routes::dashboard::mod` also opens its transaction
  through `acquire_with_rls`, but there the scoping enforces nothing: the handler is admin-gated and reads every user's
  data.

## Interfaces and dependencies

- The generated OpenAPI document is the contract this Design keeps honest. Each `/api/v1` operation's
  `#[utoipa::path(... security(...) ...)]` annotation lists one scope array shared by its `session_cookie`,
  `device_token_bearer`, `oidc_jwt_bearer` and `opds_basic` alternatives. The transports are alternative ways to present
  one identity, not different capability levels, and `authz_matrix::parse_operations` relies on that when it reads only
  the first alternative.
- `CurrentUser`'s four assertion methods (`require_admin`, `require_not_child`, `require_scope`, `may_grant_scope`) are
  the only way a handler reaches `role`, `is_child` or `scopes`. The fields are private so that a handler cannot bypass
  the enforcement point by matching on `role` directly.
- `Scope` maps through `sqlx::Type` to the Postgres `scope` enum created in
  `backend/migrations/20260810000000_initial_schema.up.sql`, and through `serde` to a lowercase wire string (`"read"`,
  `"write"`, `"admin"`), so the same type serialises identically in the database column, the OpenAPI schema and the
  `CreateTokenRequest` and `CreateTokenResponse` bodies.
- `crate::db::acquire_with_rls(pool, user_id)` is the call every RLS-owned resource in the map above makes to open its
  transaction. Its settings and policies are covered by "Row-level security and database context".

## Data and state

- **Scope values** are stored only on a `device_tokens` row (`scopes: Vec<Scope>`), written once at mint by
  `create_token` and never rewritten; no endpoint edits an existing token's scopes. A session or a validated JWT stores
  no scope set; both derive one on each request from the caller's current role (`Scope::for_role`, or
  `resolve_jwt_scopes` clamping a claimed set to that ceiling).
- **`role` and `is_child`** are read from the `users` row on every request, whatever the credential.
  `resolve_device_token` and `resolve_jwt` both look the owning user up separately from what the credential carries, and
  the session path re-reads the user row on every request as part of its `session_version` check. Nothing in this
  subject caches a role or child flag inside a credential; only a device token's `scopes` are fixed at mint.
- **`METHOD_LINT_ALLOWLIST`** is the one setting that changes the matrix's behaviour: adding a path exempts a
  mutating-verb operation from the write-floor lint. It is a compiled constant, so a new exemption is a reviewed code
  change, and `allowlist_seen` fails the build when an entry stops matching a real operation.

## Runtime behaviour

**A credential one scope level below an operation's requirement**, as the matrix exercises it for
`POST /api/v1/shelves`, which requires `write`:

1. `call_with_scoped_token` mints a device token carrying only `Scope::Read` for the test admin user through
   `device_token::create_with_limit`.
2. The request reaches `create_shelf`, which calls `current_user.require_scope(Scope::Write)` before touching the
   database.
3. `require_scope` checks `self.scopes.iter().any(|held| *held >= Scope::Write)`. The credential holds only `Read`, so
   the check fails.
4. A `tracing::warn!` records the user id and the scope needed, and the method returns `AppError::Forbidden`, which the
   handler returns as `403`.
5. `call_with_scoped_token` revokes the token whatever the response, so the test user's active-token count stays near
   zero across the grid.

**A demoted owner's admin-scoped device token** reaching an admin handler such as `list_users`:

1. The token was minted while its owner held `Role::Admin`, so its stored `scopes` include `Scope::Admin`. Nothing
   rewrites them when the owner's role changes.
2. On a later request, `resolve_device_token` reads the token row, including `scopes` as stored, and then reads the
   owning user's current row. The resulting `CurrentUser` carries the current `role`, for example `Role::Adult`, not the
   role the owner held at mint.
3. `list_users` calls `current_user.require_scope(Scope::Admin)` first. The stored scopes still include `Admin`, so this
   check passes.
4. `list_users` then calls `current_user.require_admin()`, which compares the current `role` with `Role::Admin`. The
   demoted role fails, and the handler returns `403`.
5. The role axis, not the scope axis, refused the request. The scope check alone would have let the old token through,
   which is why every admin handler makes both calls.

**A JWT whose scope claim is narrower than the caller's role**, as `jwt_read_scope_claim_blocked_from_every_mutation`
exercises it: an admin-role identity presents an access token whose `scope` claim names only `read`.
`resolve_jwt_scopes` intersects that claim with `Scope::for_role(Role::Admin)` (`Read`, `Write`, `Admin`) and keeps
`{Read}`. The resulting `CurrentUser` carries `scopes: [Read]` and `role: Admin` together, so a handler's
`require_scope(Scope::Write)` fails even though `require_admin()` on the same request would pass. For this credential
the claimed scope, not the role, decides whether a mutation is allowed.

**The deny-by-default sweep** (`every_api_v1_op_declares_a_scope`): `parse_operations` walks every path and method the
generated spec lists under `/api/v1`, and the test asserts each operation's scope array is non-empty. An operation added
without a `security(...)` scope array fails the build instead of shipping as an endpoint nothing gates.

## Failure and recovery

- **A credential below the required scope.** `CurrentUser::require_scope` logs a warning naming the caller and the scope
  needed, then returns `AppError::Forbidden` (`403`). The response does not say which scope was missing.
- **A credential carrying no scope.** The device-token and JWT paths both reject it at authentication, before any
  handler runs, with `AppError::InvalidCredential` (`401`). `resolve_device_token` checks `dt.scopes.is_empty()`
  explicitly, as defence in depth against a row the mint guard should have prevented or one written some other way, such
  as directly in the database. `resolve_jwt_scopes` returns `None` for a claim that narrows to nothing (every named
  scope unknown or above the role ceiling), and `resolve_jwt` maps that to the same rejection instead of falling back to
  the role's full set.
- **A non-admin minting an admin-scoped token.** `create_token` checks each requested scope with `may_grant_scope`
  before inserting anything. A refused grant is logged with the requested scope and the caller, and the whole request
  fails with `403`, so a partly granted token is never stored.
- **A stale `METHOD_LINT_ALLOWLIST` entry.** If a listed method and path no longer match any parsed operation, the
  `allowlist_seen` assertion in `mutating_verb_ops_require_write_scope` fails the build.
- **Unset user context on an RLS-owned resource.** That failure belongs to "Row-level security and database context".
  The boundary matters here: `shelves` and `shelf_items` carry no policy, so a row-level-security failure cannot affect
  shelf ownership, and a handler query that loses its `WHERE user_id = …` clause has no database-level fallback of the
  kind the RLS-owned resources have.

## Security and operations

Every `/api/v1` operation is deny-by-default on the scope axis: `every_api_v1_op_declares_a_scope` fails the build if an
operation is added without a declared scope requirement. The hierarchy grid (`scope_grid_enforces_the_hierarchy`, and
`jwt_scope_grid_enforces_the_hierarchy` for JWTs) then shows each declared requirement has a working gate behind it:
across every operation in the spec, for both device-token and JWT credentials, a credential one level below is refused
and one at or above the requirement is not.

The scope and role axes are separate controls. A device token's scopes are fixed at mint and never re-derived from the
owner's current role, so a demoted administrator's token can still pass `require_scope(Scope::Admin)`; each of the
twelve admin handlers pairs that call with `require_admin()`, which reads the caller's current role on every request, so
the demotion is enforced there. The matrix's admin tests (`write_token_blocked_from_every_admin_op`,
`jwt_write_scope_claim_blocked_from_every_admin_op`) catch a missing or weakened scope gate, but nothing in the matrix
tests the role gate the same way: a handler that kept `require_scope(Scope::Admin)` and dropped `require_admin()` would
still pass. The admin surface is a fixed set of twelve reviewed call sites, and review is what guards that pairing.

A non-admin can never mint an admin-scoped token (`Scope::grantable_by`, enforced in `create_token` and tested by
`routes::tokens::tests::create_token_rejects_admin_scope_ceiling`), so the scope ceiling and the role ceiling agree at
the one point where a person chooses a scope instead of the system deriving it from role.

Ownership is enforced by two mechanisms, and this Design records which resource uses which. Row-level security covers
`reading_state` and `user_preferences`. `shelves`, `shelf_items` and `device_tokens` rely on a `user_id` predicate in
each query instead, and `local_credentials` and `user_identities`, which also have no policy, are read by user ID or by
the presented issuer and subject. The authorization matrix itself is recorded as approved enforcement infrastructure in
deviation 7 of the CodeGuard deviation register.

## More information

- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 7: where Reverie keeps its
  authorization matrix.
