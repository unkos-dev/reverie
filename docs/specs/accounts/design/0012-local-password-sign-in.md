---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0012"
title: "Local password sign-in"
satisfies:
  - "REV-REQ-0039"
  - "REV-REQ-0040"
  - "REV-REQ-0041"
  - "REV-REQ-0042"
governed-by:
  - "REV-ADR-0029"
  - "REV-ADR-0034"
---

# Local password sign-in

This Design covers email-and-password sign-in for a local Reverie account: the `/auth/local/login` handler, the Argon2id
hashing and verification it calls, the two independent rate-limiting mechanisms that guard it, the
enumeration-resistance techniques that make a wrong password and an unknown email indistinguishable, the shared
password-strength policy that every credential-setting path applies (and the three bootstrap paths that do not), and the
`/auth/register` self-service path as a thin caller of that policy. It also covers the two client-side sign-in forms.

## Purpose and boundaries

This subject owns `local_login` and `register` (`backend/src/routes/auth.rs`), the `local_credentials` model
(`backend/src/models/local_credentials.rs`), Argon2id hashing and verification and the anti-enumeration dummy-hash
control (`backend/src/auth/password.rs`), the per-source login rate limiter and client-IP resolution
(`backend/src/auth/rate_limit.rs`), the per-account escalating backoff (`backend/src/models/login_throttle.rs`, the
`local_login_throttle` table), the shared password-strength policy (`backend/src/auth/password_policy.rs`), and the two
client forms: `frontend/src/routes/auth-login.tsx` (mounted) and `frontend/src/routes/auth-register.tsx` (present and
tested, but not mounted in `frontend/src/main.tsx`).

It does not own establishing or persisting the session a successful login produces: `crate::auth::session::login`,
cookie attributes, and rehydration on each following request belong to the Design "Sessions" (REV-DESIGN-0011). It does
not own minting the CSRF synchronizer token or seeding the theme cookie, the two calls `local_login` makes right after
`session::login` succeeds; those belong to the Design "CSRF protection" (REV-DESIGN-0008) and the theme-preference
subject, and this Design names only the call sites (`session.insert("csrf_token", …)` inline in `local_login`, and
`crate::auth::theme_cookie::set_theme_cookie`). It does not own how a following request becomes a resolved caller
identity, the Design "Request authentication" (REV-DESIGN-0010). It does not own account recovery: PIN issuance and
consumption (`backend/src/auth/recovery.rs`, `backend/src/models/password_reset_pin.rs`) and the `forgot_password` and
`reset_password` handlers in `backend/src/routes/auth.rs` are the account recovery subject, though `reset_password`
calls this subject's password policy as an interface. It does not own first-run bootstrap: the `setup_status` and
`setup` handlers in `backend/src/routes/auth.rs`, the `run_bootstrap`, `seed_admin_if_configured` and
`read_bootstrap_seed` functions in `backend/src/lib.rs`, and `user::create_first_admin` / `user::admin_exists` in
`backend/src/models/user.rs` are the first-run bootstrap subject; `setup` enforces only `password_min_length` on its
candidate password inline rather than calling this subject's policy, and the other two bootstrap entry points share that
narrower check through the same `seed_admin_if_configured` function. It does not own the account administration surface:
`create_user`, `admin_reset_password` and `change_own_password` in `backend/src/routes/users/mod.rs` mutate
`local_credentials` and are call sites of this subject's password policy, but the surface itself (role and child-status
mutation, session-version invalidation policy, the last-admin lock order) is the account administration subject.

Depends on: `crate::auth::session::login` to establish the session on success; `crate::models::user::find_by_email` and
`crate::models::user::User` to resolve the account; `crate::services::enrichment::http::api_client`, which supplies the
SSRF-resistant HTTP client the breach check sends its outbound request through; `Config`'s `login_rate_per_min`,
`login_throttle_base_secs`, `login_throttle_cap_secs`, `password_min_length`, `password_max_length`,
`password_min_zxcvbn_score`, `password_breach_check_enabled`, `password_breach_check_url`, `trusted_client_ip_header`,
`local_auth_enabled` and `self_registration_enabled` fields.

Depended on by: `routes/users/mod.rs`'s `create_user`, `admin_reset_password` and `change_own_password`, and
`routes/auth.rs`'s `reset_password`, each of which calls `password_policy::enforce_from_config` before writing a
credential; the `reverie unlock-account <email>` CLI subcommand (`run_unlock_account` in `backend/src/lib.rs`), which
clears a per-account backoff out of band; and the sign-in page.

ADR-0029 fixes local password sign-in as a co-equal authentication mode alongside OIDC, resolving to the same identity
and session model with no path to administrator other than bootstrap; this Design is the mechanism that realises that
decision on the local-password side.

## Structure

- `backend/src/auth/password.rs` holds `hash_password` and `verify_password`, thin wrappers over
  `argon2::Argon2::default()` (Argon2id, the pinned `argon2` crate's own defaults: `m_cost` 19456 KiB, `t_cost` 2,
  `p_cost` 1) that produce and check PHC strings. `DUMMY_PHC` is a `LazyLock<String>`, a process-stable PHC string
  computed once, at first use, from 32 random bytes generated at runtime (never a hard-coded value);
  `verify_against_dummy` verifies a candidate password against it and returns a `bool` that every caller discards,
  always `false`, so a caller can spend an Argon2id-verification-equivalent amount of work with nothing to verify
  against.
- `backend/src/auth/password_policy.rs` holds `PasswordPolicy` (a config-derived view: `min_length`, `max_length`,
  `min_zxcvbn_score`, `breach_check_enabled`, `breach_check_url`), `PolicyError` (`TooShort`, `TooLong`, `TooWeak`,
  `Breached`), `check_strength` (zxcvbn, penalising any password containing one of the caller-supplied context words
  such as the account's email or display name), `check_breached` (the HIBP Pwned Passwords k-anonymity query), and
  `enforce`. Five credential-setting paths call `enforce` (directly or through `enforce_from_config`, the
  request-handler convenience that builds the `PasswordPolicy` and the breach client from `Config`): registration
  (`register`), admin create (`create_user`), admin reset (`admin_reset_password`), self-service change
  (`change_own_password`), and PIN reset (`reset_password`, the account recovery subject). Three further paths that also
  write a first credential check only the configured length floor and never call `enforce`: the HTTP `setup` handler's
  own inline check, and the CLI bootstrap subcommand and the server's own startup environment seed, both of which call
  the same `seed_admin_if_configured` function (`backend/src/lib.rs`) and its identical length-only check.
  `PolicyError`'s `Display` text becomes the RFC 9457 problem body's `detail` field for four of the five `enforce`
  callers (registration, admin create, admin reset, self-service change); the fifth, PIN reset, discards the
  `PolicyError` and answers its own fixed generic message instead, so a weak password there reads identically to a bad
  PIN.
- `backend/src/auth/rate_limit.rs` holds `PeerAddr` (a never-rejecting `FromRequestParts` wrapper around
  `ConnectInfo<SocketAddr>`, `None` when the test harness supplies no peer), the `LoginLimiter` type alias over
  `governor::DefaultKeyedRateLimiter<IpAddr>`, `build_login_limiter` (quota `per_min` per minute per key, burst equal to
  the rate), and `client_ip` (resolves the rate-limit key from the TCP peer and, only when the operator has named a
  `trusted_client_ip_header`, the leftmost token of that forwarded-for header).
- `backend/src/models/local_credentials.rs` holds `LocalCredential` (one row per `user_id`, the PK; `password_hash` is
  the Argon2id PHC string; the type hand-implements `Debug` to redact the hash and deliberately does not derive
  `Serialize`), `find_by_user_id` (`Ok(None)` for an OIDC-only account), and `set_password` (an
  `INSERT … ON CONFLICT (user_id) DO UPDATE`, callable against a pool or a transaction). `password_hash` also gets a
  first value written directly by `user::create_first_admin` and `user::create_local`, both of which insert into
  `local_credentials` inline inside their own transaction rather than calling `set_password`. `set_password` is not a
  replacement-only operation: `admin_reset_password`'s precondition is only that the target `users` row exists (a
  `FOR UPDATE` existence check on `users`, not on `local_credentials`), so it can write a first credential onto an
  OIDC-only account exactly as `create_first_admin`/`create_local` do; `reset_password`'s precondition (the account
  recovery subject) is only an active, unexpired PIN, so it too can write a first credential onto an OIDC-only account.
  Only `change_own_password` requires an existing `local_credentials` row up front: it calls `find_by_user_id` and
  answers a `422` ("this account has no password to change; it signs in through an identity provider") when there is
  none, before it ever reaches `set_password`.
- `backend/src/models/login_throttle.rs` holds `record_failure`, `reset` and `backoff_until`, all keyed on
  `email.to_lowercase()` rather than `user_id`, so the throttle exists independent of whether the email resolves to an
  account. `record_failure` upserts a capped-exponential-backoff window, `min(cap, base * 2^prior_failures)`, where
  `prior_failures` is the failure count recorded before the current one (the row's `fail_count` value read at the moment
  of the upsert, before it is incremented); the first failure therefore waits `min(cap, base)` and each further failure
  doubles the base, saturating at the cap.
- `backend/src/routes/auth.rs` holds `local_login` and `register`, and the shared `enforce_source_rate_limit` helper
  that `register`, `setup`, `forgot_password` and `reset_password` all call before any policy or hashing work.
  `local_login` does not call that helper: it inlines an equivalent `client_ip` resolution and `login_limiter.check_key`
  call directly, immediately after the `local_auth_enabled` gate. In every handler that carries a disabled-feature `404`
  gate (`local_login`, `register`, `forgot_password`, `reset_password`) that gate runs before the per-source limiter
  check; `setup` carries no such gate at all, so its limiter check is the first thing that runs. In every case the
  limiter check is skipped, not merely passed, when `client_ip` resolves to `None` (no TCP peer supplied and no
  configured or matching trusted forwarded-for header): the `check_key` call is never made.
- `frontend/src/routes/auth-login.tsx` is an uncontrolled `FormData` form that reads `GET /auth/setup/status` to decide
  whether to render the local form, an OIDC action, or both, and calls `loginLocal` on submit.
  `frontend/src/routes/auth-register.tsx` is the equivalent form for `register`, client-validated through
  `frontend/src/api/auth.schemas.ts`'s `displayNameField`, `emailField` and `newPasswordField` (a bare
  `z.string().min(8)`, a hard-coded floor rather than a mirror of the server's own configured minimum) before the
  request is sent; a rejection from the server surfaces through `ApiError.detail`.

## Interfaces and dependencies

- `POST /auth/local/login` (`LocalLoginRequest { email, password }`) returns `204` on success and sets the session
  cookie and (via `jar`) the theme cookie; `404` when `local_auth_enabled` is `false`; `422` with a generic message for
  any failed credential; `429` when a rate limit is active. It is unauthenticated (`security(())` in its OpenAPI
  annotation). Neither `429` source (the per-source limiter or the per-account backoff) sets a `Retry-After` header.
- `POST /auth/register` (`RegisterRequest { email, display_name, password }`) returns `201` with no session on success;
  `404` when `self_registration_enabled` or `local_auth_enabled` is `false`; `409` on a duplicate email; `422` on a
  malformed email or a policy rejection; `429` on the per-source limit. `RegisterRequest` declares no role field, and
  the `ApiJson` extractor does not reject unrecognised fields, so any role value a caller supplies in the request body
  is silently ignored rather than causing a rejection. `register` always calls `user::create_local` with `Role::Adult`
  as a literal argument.
- `password_policy::enforce_from_config(config, password, user_inputs)` is the interface every other credential-setting
  handler calls: `routes/users/mod.rs`'s `create_user`, `admin_reset_password` and `change_own_password` (each through a
  local `enforce_password_policy` wrapper that maps `PolicyError` to `AppError::Validation`), and `routes/auth.rs`'s
  `reset_password`. Each caller supplies its own `user_inputs` (typically the target's email and display name) so zxcvbn
  penalises a password that echoes them.
- The HIBP Pwned Passwords range API (`https://api.pwnedpasswords.com/range` by default, operator-overridable via
  `password_breach_check_url`) is queried with only a 5-character SHA-1 prefix of the candidate password, and with an
  `Add-Padding` header so the response size does not itself reveal whether the prefix had real hits. The request runs
  against the shared client's own 10-second timeout (`crate::services::enrichment::http::api_client`); `enforce` applies
  no timeout of its own.
- `frontend/src/api/auth.ts`'s `loginLocal` and `register` are the client entry points; both parse their body through a
  Zod schema in `auth.schemas.ts` before sending it, and `loginLocal` calls `refreshCsrfToken()` immediately after a
  successful login so the client's cached CSRF token matches the new session.
- `reverie unlock-account <email>` (`run_unlock_account`, `backend/src/lib.rs`) opens a one-connection pool and calls
  `login_throttle::reset` directly; it does not go through the HTTP surface at all.

## Data and state

- **`local_credentials.password_hash`.** One row per `user_id`; an Argon2id PHC string; never serialised, never logged,
  and the table is granted to `reverie_app` only (no `reverie_readonly` grant, matching `device_tokens`). It has three
  writer functions across four subjects: `user::create_first_admin` (first-run bootstrap) and `user::create_local`
  (`register`, in this Design; `create_user`, in the account administration subject) both insert the first value
  directly; `local_credentials::set_password` (in this Design's model) writes for `admin_reset_password` and
  `change_own_password` (the account administration subject) and for `reset_password` (the account recovery subject). As
  the Structure section describes, only `change_own_password` requires an existing row first; `admin_reset_password` and
  `reset_password` each write a first credential onto an OIDC-only account exactly as the two `create_*` functions do.
  No writer here reads the row back to compare against the incoming value; every write is unconditional.
- **`local_login_throttle`.** Keyed on `email_lower`; a row exists only once a failure has been recorded for that key,
  and `reset` deletes it outright rather than zeroing a counter. `record_failure` runs only for a failed `local_login`
  attempt whose email is not already inside an active backoff window: `local_login`'s failed-attempt branch checks
  `backoff_until` first, and when it returns an active window the handler answers `429` immediately without calling
  `record_failure`, so that attempt is not recorded and the window does not extend. Only a failure that arrives after
  the prior window has elapsed reaches `record_failure` and escalates it. `reset` is called on a successful login and by
  the CLI unlock command; no other code path writes or deletes this table. `backoff_until` is the sole reader, called
  only from `local_login`'s failed-attempt branch; a successful, enabled-account login never reaches it.
- **The per-source limiter.** `state.login_limiter` is a single in-process `governor` keyed map built once at startup
  from `login_rate_per_min` (validated `>= 1` before `build_login_limiter` is called); it holds no database row, so a
  process restart clears every key's accrued state, unlike `local_login_throttle`, which survives one.
- **Configuration.** `login_rate_per_min` (default 10/min), `login_throttle_base_secs` (default 2) and
  `login_throttle_cap_secs` (default 900) shape the two throttles; `password_min_length` (default 8, validated to at
  least 8), `password_max_length` (default 256, validated to at least 64), `password_min_zxcvbn_score` (default 2),
  `password_breach_check_enabled` (default `true`) and `password_breach_check_url` shape the policy;
  `self_registration_enabled` (default `false`) gates `register`; `trusted_client_ip_header` (default unset) opts a
  deployment into trusting a forwarded-for header. None of these reload at runtime; each is read once from `Config` at
  the point of use.

## Runtime behaviour

**A login attempt for a known email with the wrong password:**

1. `local_login` checks `state.config.local_auth_enabled`, then resolves the rate-limit key via `client_ip` and calls
   `state.login_limiter.check_key` inline (skipped entirely when `client_ip` resolves to `None`); over quota returns
   `AppError::RateLimited` before any database work.
2. `user::find_by_email` resolves the account. Because it is `Some`, `local_credentials::find_by_user_id` runs a second
   query to fetch the stored PHC string.
3. `password::verify_password` runs the real Argon2id verification against that PHC string and returns an error;
   `verified` is `false`.
4. `session_user` resolves to `None` (the match requires `verified && user.disabled_at.is_none()`), so the function
   falls into the failed-attempt branch: `login_throttle::backoff_until` is checked (no active window yet on a first
   failure), then `login_throttle::record_failure` upserts a new window keyed on the submitted email.
5. The handler returns `AppError::Validation("invalid email or password")`, a generic `422` whose message is the
   response's `detail` field.

**A login attempt for an email with no account:**

1. Steps 1 and 4 to 5 above run identically, keyed on the submitted (non-existent) email.
2. `user::find_by_email` returns `None`, so step 2's second query never runs: `credential` is set to `None` directly
   rather than through a database round trip.
3. Because `credential` is `None`, `password::verify_against_dummy` runs instead of `verify_password`: one Argon2id
   verification against the process-stable `DUMMY_PHC`, equalising the CPU-bound Argon2 cost between this path and the
   one above. `verified` stays `false`.
4. `local_login_wrong_password_is_generic_422`, `local_login_unknown_email_matches_wrong_password` and
   `local_login_disabled_account_is_generic_422` each assert the resulting status and, for the second, the
   byte-identical response body against the known-account case.

**A correct password submitted during an active per-account backoff window:**

1. Steps 1 to 3 above run, and `verify_password` succeeds. `local_login` checks `login_throttle::backoff_until` only
   inside the failed-attempt branch, so a verified login never reaches it: the `session_user` match succeeds,
   `login_throttle::reset` deletes the row, and `session::login` establishes the session in the same request that clears
   the backoff.
2. A wrong attempt arriving during that same window instead returns `429` from the `backoff_until` check itself, before
   `record_failure` runs, so it is not recorded and the window does not extend. The window changes the outcome of a
   wrong guess only, never a correct one; the per-source limiter is what bounds how many attempts, correct or wrong, can
   be made in a given period.

**A soft-disabled account presenting its correct password:** `verified` is `true`, but the `session_user` match also
requires `user.disabled_at.is_none()`, which fails; the request falls into the same failed-attempt branch as a wrong
password, subject to the same active-window check above, with the same generic `422`. No distinct "account disabled"
response exists on this path.

**Self-registration reaching the shared policy:** after the `self_registration_enabled` and `local_auth_enabled` gates
and `enforce_source_rate_limit`, `register` validates the email shape, then calls `password_policy::enforce_from_config`
with `[&body.email, &body.display_name]` as context words before hashing. `enforce` checks `max_length` first (a DoS
guard: rejecting an oversized candidate before any zxcvbn or HIBP work runs), then `min_length`, then the zxcvbn score,
then, only if the first three pass and the check is enabled, the HIBP query.
`enforce_rejects_over_max_length_before_any_other_work` asserts the returned `TooLong` variant for a candidate over the
configured maximum with a breach-check URL pointing nowhere reachable; that no breach request is actually sent for that
candidate follows from `enforce`'s early return on the length check, confirmed by inspection of the function's check
order rather than by the test itself, which observes only the returned variant.

## Failure and recovery

- **`local_auth_enabled` is `false`.** `local_login` and `register` both return `AppError::NotFound` (`404`) before any
  other work (`register` is additionally gated on `self_registration_enabled`); the client's setup-status query is what
  normally keeps the local form from rendering at all. `setup` carries no such gate.
- **Rate limiting.** Two independent mechanisms can each produce `AppError::RateLimited` (`429`), and neither response
  carries a `Retry-After` header: the per-source `governor` limiter, checked first (ahead of the disabled-feature gate
  only on `setup`) whenever a client IP resolves; and the per-account backoff, checked only inside `local_login`'s
  failed-attempt branch, which a verified login bypasses entirely regardless of the window's state.
- **Password policy rejection.** `enforce`'s `PolicyError` variants map to `AppError::Validation` (`422`) with the
  error's `Display` text as the response's `detail` field, for four of its five callers (registration, admin create,
  admin reset, self-service change): `TooShort`/`TooLong` name the configured bound, `TooWeak` carries zxcvbn's feedback
  (or a generic message when zxcvbn returns none), and `Breached` returns the fixed sentence "this password has appeared
  in a known data breach; choose a password you have not used elsewhere". The fifth caller, PIN reset (the account
  recovery subject), discards the `PolicyError` entirely and answers its own fixed generic message instead, so there a
  weak password reads identically to a bad PIN. `register_weak_password_returns_422` and
  `register_invalid_email_returns_422` cover the client-visible shape of a registration rejection.
- **HIBP unreachable, slow, or erroring.** `check_breached` treats a transport error, any non-2xx status (including a
  `429` or `503` from HIBP itself), an unreadable response body, and a count that cannot be parsed on the one matching
  suffix line, all identically: log a `warn` and return `false` (not breached). `enforce` then proceeds on strength
  alone. `check_breached_fails_open_when_unreachable`, `check_breached_fails_open_on_non_2xx` and
  `check_breached_malformed_count_for_matching_suffix_fails_open` each pin one of those cases, and
  `enforce_allows_a_strong_password_when_breach_check_fails_open` confirms a strong candidate is still accepted when the
  check itself fails open.
- **A duplicate email on `register`.** `user::create_local` returns `CreateUserError::EmailExists`, mapped to
  `AppError::EmailConflict` (`409`); `register_duplicate_email_returns_409` covers it.

## Security and operations

Argon2id verification work is spent identically on the two paths that reach it with nothing to authenticate: a
credential that fails to verify against a real stored hash, and `verify_against_dummy`'s check against the
process-stable `DUMMY_PHC` on the no-account and no-local-credential paths. The account and credential lookups that
precede that step are not equalised the same way: a known email always issues a second query
(`local_credentials::find_by_user_id`) that an unknown email never reaches, so this equalises Argon2 cost rather than
overall request latency; no test in this module measures wall-clock timing.

Three techniques together keep an unknown email and a wrong password from being distinguished on the response itself:
both return the identical generic `422` body; a soft-disabled account presenting its correct password folds into the
same branch rather than surfacing a distinct disabled-account error; and the per-account throttle, when it escalates at
all, escalates identically for either case, keyed on the submitted email regardless of whether it resolves to an
account. `local_login_unknown_email_matches_wrong_password` asserts the status and body are byte-identical between the
two.

`local_login` verifies the password before consulting the per-account backoff window, and consults the window only on
the failed branch: a correct password succeeds during an active window and clears it, and a wrong attempt during the
window answers `429` from the `backoff_until` check without ever reaching `record_failure`, so it is not recorded and
the window does not extend. The window's escalating length changes the outcome of a wrong guess only, never a correct
one. The per-source limiter is what bounds how many attempts a given client can make in a period; `client_ip` trusts a
forwarded-for header only when the operator has explicitly named one via `trusted_client_ip_header`, so with none
configured every client behind a shared front end shares one rate-limit key. The per-account backoff survives a process
restart (Postgres-backed); the per-source limiter does not (an in-process `governor` map), and an operator can clear it
per email, out of band, only through `reverie unlock-account`.

`enforce_from_config` builds its breach-check HTTP client through `crate::services::enrichment::http::api_client`, which
resolves every hostname it dials, including each redirect hop it follows, through the SSRF-filtering `ssrf_resolver`
shared with the enrichment pipeline's `cover_client`. Unlike `cover_client`, `api_client` carries no per-redirect-hop
URL revalidation (`cover_client`'s `validate_hop`); the case that gap leaves open, and that `cover_client` closes, is a
redirect whose target is a bare IP address literal rather than a hostname, since a literal never triggers a resolver
lookup at all and so is never checked against the denied-range list by this client.

Registration is config-gated (`self_registration_enabled`, default `false`) and the route is not mounted in the shipped
client (`frontend/src/main.tsx` mounts no `/register` route); reaching `POST /auth/register` today requires calling the
API directly. `RegisterRequest` declares no role field, and the JSON extractor does not reject unrecognised fields, so a
role value supplied in the request body is silently ignored rather than causing a rejection; `register` always calls
`create_local` with `Role::Adult` as a literal argument, so privilege escalation through this path would require a
source change to that literal, not a crafted request body.

`enforce` is the strength-and-breach gate for five callers (registration, admin create, admin reset, self-service
change, PIN reset). Three further paths bypass it and check only the configured length floor: the HTTP `setup` handler's
own inline check, and the CLI bootstrap subcommand and the server's own startup environment seed, both of which call the
same `seed_admin_if_configured` function and its identical length-only check. The first, highest-privilege account on an
instance is consequently the one local credential that `enforce`'s zxcvbn and breach-check legs never screen.

CodeGuard deviation 5 in `docs/security/codeguard/README.md` records that Reverie ships no first-party MFA and that the
local password path is single-factor, listing Argon2id hashing, the two rate-limiting mechanisms and constant-work
verification among its compensating controls; this Design is the mechanism behind that entry.

## More information

- [Reference configuration](../../../../website/src/content/docs/reference/configuration.mdx): the generated
  `REVERIE_LOGIN_RATE_PER_MIN`, `REVERIE_LOGIN_THROTTLE_*` and `REVERIE_PASSWORD_*` field reference.
- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 5: Reverie's single-factor local
  authentication posture and its compensating controls.
