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
  - "REV-ADR-0054"
  - "REV-ADR-0059"
---

# Local password sign-in

This Design covers email-and-password sign-in for a local Reverie account: the `/auth/local/login` handler, the Argon2id
hashing and verification it calls, the two independent rate-limiting mechanisms that guard it, the
enumeration-resistance techniques that make a wrong password and an unknown email indistinguishable, the shared
password-strength policy that every credential-setting path applies, including bootstrap, and the `/auth/register`
self-service path as a thin caller of that policy. It also covers the two client-side sign-in forms.

## Purpose and boundaries

This subject owns `local_login` and `register` (`backend/src/routes/auth.rs`), the `local_credentials` model
(`backend/src/models/local_credentials.rs`), Argon2id hashing and verification and the anti-enumeration dummy-hash
control (`backend/src/auth/password.rs`), the per-source login rate limiter and client-IP resolution
(`backend/src/auth/rate_limit.rs`), transactional account admission (`backend/src/models/login_throttle.rs`, the
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
`backend/src/models/user.rs` are the first-run bootstrap subject; all three entry points call this subject's shared
policy with the administrator's email and display name as context. It does not own the account administration surface:
`change_own_password` in `backend/src/routes/users/mod.rs` and `redeem_credential_link` in `backend/src/routes/auth.rs`
mutate `local_credentials` and call this subject's password policy, but the surface itself (role and child-status
mutation, session-version invalidation policy, the last-admin lock order) is the account administration subject.

Depends on: `crate::auth::session::login` to establish the session on success; `crate::models::user::find_by_email` and
`crate::models::user::User` to resolve the account; `crate::services::enrichment::http::api_client`, which supplies the
SSRF-resistant HTTP client the breach check sends its outbound request through; `Config`'s `login_rate_per_min`,
`auth_hash_concurrency`, `password_min_length`, `password_max_length`, `password_min_zxcvbn_score`,
`password_breach_check_enabled`, `trusted_client_ip_header`, `local_auth_enabled` and `self_registration_enabled`
fields.

Depended on by: `routes/users/mod.rs`'s `change_own_password`, and `routes/auth.rs`'s `reset_password`,
`redeem_credential_link` and `setup`, and `lib.rs`'s `seed_admin_if_configured`, each of which calls
`password_policy::enforce_from_config` before writing a credential; the `reverie unlock-account <email>` CLI subcommand
(`run_unlock_account` in `backend/src/lib.rs`), which clears account accounting out of band; and the sign-in page.

ADR-0029 fixes local password sign-in as a co-equal authentication mode alongside OIDC, resolving to the same identity
and session model with no path to administrator other than bootstrap; this Design is the mechanism that realises that
decision on the local-password side.

## Structure

The application-wide password ceiling is 256 Unicode code points. Deployment maxima are configurable from 64 to 256.
This ceiling is a product choice, not a value mandated by NIST or OWASP.

- `backend/src/auth/password.rs` holds `hash_password` and `verify_password`, thin wrappers over
  `argon2::Argon2::default()` (Argon2id, the pinned `argon2` crate's own defaults: `m_cost` 19456 KiB, `t_cost` 2,
  `p_cost` 1) that produce and check PHC strings. `DUMMY_PHC` is a `LazyLock<String>`, a process-stable PHC string
  computed once, at first use, from 32 random bytes generated at runtime (never a hard-coded value);
  `verify_against_dummy` verifies a candidate password against it and returns a `bool` that every caller discards,
  always `false`, so a caller can spend an Argon2id-verification-equivalent amount of work with nothing to verify
  against. `HashLimiter` shares one Tokio semaphore across every request-path password and recovery-PIN hash or
  verification. `HashPermit` moves owned inputs and an `Arc<OwnedSemaphorePermit>` clone into `spawn_blocking`; the
  closure holds no database handle or lock. The synchronous kernels remain available for CLI commands, environment
  seeding and test fixtures. First use of the lazy dummy hash runs inside a request's permit.
- `backend/src/auth/password_policy.rs` holds `PasswordPolicy` (a config-derived view: `min_length`, `max_length`,
  `min_zxcvbn_score`, `breach_check_enabled`, `breach_check_url`), `PolicyError` (`TooShort`, `TooLong`, `TooWeak`,
  `Breached`), `check_strength` (zxcvbn, penalising any password containing one of the caller-supplied context words
  such as the account's email or display name), `check_breached` (the HIBP Pwned Passwords k-anonymity query), and
  `enforce`. Registration, credential-link redemption, self-service change, PIN recovery, API setup, CLI bootstrap and
  the startup environment seed all call `enforce_from_config` before hashing and database mutation. Setup and both
  headless variants supply the administrator's email and display name as context. Registration and credential-link
  redemption also supply both; self-service change uses the target's current name and optional email. Recovery supplies
  the submitted email before resolving an account to preserve enumeration resistance. HTTP callers return policy
  feedback in the RFC 9457 `detail` field, except PIN recovery, which returns its fixed generic reset error. Headless
  bootstrap reports a policy rejection without logging the password.
- `backend/src/auth/rate_limit.rs` holds `PeerAddr` (a never-rejecting `FromRequestParts` wrapper around
  `ConnectInfo<SocketAddr>`, `None` when the test harness supplies no peer), the `LoginLimiter` type alias over
  `governor::DefaultKeyedRateLimiter<IpAddr>`, `build_login_limiter` (quota `per_min` per minute per key, burst equal to
  the rate), and `client_ip` (resolves the rate-limit key from the TCP peer and, only when the operator has named a
  `trusted_client_ip_header`, the leftmost token of that forwarded-for header).
- `backend/src/models/local_credentials.rs` holds `LocalCredential` (one row per `user_id`, the PK; `password_hash` is
  the Argon2id PHC string; the type derives `redactable::SensitiveDisplay` with `Secret` on the hash and `not_sensitive`
  on the other fields, and deliberately does not derive `Serialize`), `find_by_user_id` (`Ok(None)` for an OIDC-only
  account), and `set_password` (an `INSERT … ON CONFLICT (user_id) DO UPDATE`, callable against a pool or a
  transaction). `password_hash` also gets a first value written directly by `user::create_first_admin` and
  `user::create_local`, both of which insert into `local_credentials` inline inside their own transaction rather than
  calling `set_password`. `create_local` accepts an optional hash; administrator creation supplies none. `set_password`
  can write a first credential through credential-link redemption or PIN recovery, including for an OIDC-only account.
  Only `change_own_password` requires an existing `local_credentials` row up front: it calls `find_by_user_id` and
  answers a `422` ("this account has no password to change; it signs in through an identity provider") when there is
  none. Its write uses `replace_verified_password`, an UPDATE conditional on the hash verified before the transaction,
  so it cannot overwrite a concurrent change or insert a missing credential.
- `backend/src/models/login_throttle.rs` owns `admit`, the locked `Attempt`, its `fail` and `succeed` commits, CLI
  `reset`, and bounded `delete_inactive`. All use the same lower-case email key, independent of account existence.
  Admission owns a PostgreSQL transaction until the credential outcome commits or cancellation rolls it back.
- `backend/src/routes/auth.rs` holds `local_login` and `register`, and the shared `enforce_source_rate_limit` helper
  that `local_login`, OIDC start, `register`, `setup`, `forgot_password` and `reset_password` call before provider,
  policy or hashing work. `local_login` calls it immediately after the `local_auth_enabled` gate. In every handler that
  carries a disabled-feature `404` gate (`local_login`, `register`, `forgot_password`, `reset_password`) that gate runs
  before the per-source limiter check; `setup` carries no such gate at all, so its limiter check is the first thing that
  runs. In every case the limiter check is skipped, not merely passed, when `client_ip` resolves to `None` (no TCP peer
  supplied and no configured or matching trusted forwarded-for header): the `check_key` call is never made.
- `frontend/src/routes/auth-login.tsx` is an uncontrolled `FormData` form that reads `GET /auth/setup/status` to decide
  whether to render the local form, an OIDC action, or both, and calls `loginLocal` on submit.
  `frontend/src/routes/auth-register.tsx` is the equivalent form for `register`, client-validated through
  `frontend/src/api/auth.schemas.ts`'s `displayNameField`, `emailField` and `newPasswordField(policy)` (configured
  Unicode scalar-value bounds from setup status) before the request is sent; a rejection from the server surfaces
  through `ApiError.detail`.

## Interfaces and dependencies

- `POST /auth/local/login` (`LocalLoginRequest { email, password }`) returns `204` on success and sets the session
  cookie and (via `jar`) the theme cookie; `404` when `local_auth_enabled` is `false`; `422` with a generic message for
  any failed credential; `409 sign-in-inactive` for an authenticated session or a losing cohort; `429` when a rate limit
  is active; `503 auth-busy` when hashing capacity remains unavailable for 30 seconds. It is unauthenticated
  (`security(())` in its OpenAPI annotation). Each `429` source (the per-source limiter or account admission) supplies a
  positive, rounded-up `Retry-After` delay in seconds.
- `POST /auth/register` (`RegisterRequest { email, display_name, password }`) returns `201` with no session on success;
  `404` when `self_registration_enabled` or `local_auth_enabled` is `false`; `409` on a duplicate email; `422` on a
  malformed email or a policy rejection; `429` on the per-source limit. `RegisterRequest` declares no role field, and
  the `ApiJson` extractor does not reject unrecognised fields, so any role value a caller supplies in the request body
  is silently ignored rather than causing a rejection. `register` always calls `user::create_local` with `Role::Adult`
  as a literal argument.
- `password_policy::enforce_from_config(config, password, user_inputs)` is the interface every other credential-setting
  handler calls: `routes/users/mod.rs`'s `change_own_password` (through a local `enforce_password_policy` wrapper that
  maps `PolicyError` to `AppError::Validation`), and `routes/auth.rs`'s `reset_password`, `redeem_credential_link` and
  `setup`, plus the environment/CLI bootstrap helper. Each caller supplies its own `user_inputs` (typically the target's
  email and display name) so zxcvbn penalises a password that echoes them.
- The HIBP Pwned Passwords range API (`https://api.pwnedpasswords.com/range`, the only supported endpoint) is queried
  with only a 5-character SHA-1 prefix of the candidate password, and with an `Add-Padding` header so the response size
  does not itself reveal whether the prefix had real hits. The request runs against the shared client's own 10-second
  timeout (`crate::services::enrichment::http::api_client`); `enforce` applies no timeout of its own.
- `frontend/src/api/auth.ts`'s `loginLocal` and `register` are the client entry points; both parse their body through a
  Zod schema in `auth.schemas.ts` before sending it, and `loginLocal` calls `refreshCsrfToken()` immediately after a
  successful login so the client's cached CSRF token matches the new session.
- `reverie unlock-account <email>` (`run_unlock_account`, `backend/src/lib.rs`) opens a one-connection pool and calls
  `login_throttle::reset` directly; it does not go through the HTTP surface at all.

## Data and state

- **`local_credentials.password_hash`.** One Argon2id PHC per user, never serialised or logged; only the application
  role can access it. `user::create_first_admin` inserts for API setup, CLI bootstrap and environment seed.
  `user::create_local` inserts credentials for registration; administrator creation supplies no password.
  `local_credentials::set_password` upserts for credential-link redemption and PIN recovery, including a first
  credential for an OIDC-only account. `replace_verified_password` updates for self-service change only when the stored
  hash still matches the verified hash. Existing-account writers lock the user with `FOR NO KEY UPDATE` before touching
  credentials; recovery then consumes its PIN before setting the password. The non-key lock permits PIN issuance's
  foreign-key key-share lock. A successful change increments `session_version` in the same transaction. A stale
  self-service write returns 422 without changing the credential or session version. Policy checks and hashing run
  before any locked transaction.
- **`local_login_throttle`.** Each `email_lower` row stores at most ten `completed_failures`, `locked_until` and
  `updated_at`. A no-op conflict update creates or locks the row through commit. The handler reads database wall time
  after acquiring the lock, so lock waiting does not make admission use a stale transaction-start timestamp. At
  completion it reads wall time again, retains failures in `(t - 15 minutes, t]`, appends this failure and starts a
  15-minute block at ten. Equality expires both failures and blocks. A slow failure counts at completion.
- **Account-state writers.** Failed local login commits history; a winning enabled-account login and CLI unlock clear
  history and the block under the same row lock. They retain an empty row. The sweep alone physically deletes rows,
  locking current eligible rows with `FOR UPDATE SKIP LOCKED` in deterministic batches of at most 1,000. A row is
  eligible only when its block and every retained failure are absent or expired. Active blocks survive aged-out history.
  Password changes and recovery do not reset accounting. Request processing enforces expiry independently of cleanup.
- **Source-state writers.** Startup builds distinct `login_limiter`, `recovery_limiter` and `oidc_limiter` governor
  maps. Password login, setup and registration spend the first; forgot/reset-password and credential-link
  lookup/redemption spend the second; OIDC start spends the third. Every map uses `login_rate_per_min` (default 10/min
  per source), with the existing peer/trusted-header rule. The hourly sweep calls native `retain_recent` on each map,
  preserving active allowance. Restart clears these process-local maps; account state persists in PostgreSQL.
- **Hashing capacity.** Startup constructs one `HashLimiter` from `auth_hash_concurrency`
  (`REVERIE_AUTH_HASH_CONCURRENCY`, default 1, minimum 1, no configured maximum). Each permit allows one more parallel
  Argon2 operation of about 19 MiB. Request handlers acquire and release this shared capacity; each blocking closure
  retains a permit clone until it returns, including after caller cancellation. The fixed `HASH_WAIT_LIMIT` is 30
  seconds; Tokio queues acquisitions in FIFO order.
- **Configuration.** Account admission uses fixed ten-failure, 15-minute observation and 15-minute block values.
  `password_min_length` (default 15, validated to at least 15 and no greater than the maximum), `password_max_length`
  (default 256, validated within 64–256 Unicode code points), `password_min_zxcvbn_score` (default 2),
  `password_breach_check_enabled` (default `true`) shape the policy; `self_registration_enabled` (default `false`) gates
  `register`; `trusted_client_ip_header` (default unset) opts a deployment into trusting a forwarded-for header. None of
  these reload at runtime; each is read once from `Config` at the point of use.

`GET /auth/setup/status` exposes the configured minimum and maximum alongside provider/bootstrap state.
`usePasswordPolicy` shares the setup-status query key with the setup and sign-in screens. Setup, registration, recovery,
self-service change and credential-link forms count Unicode scalar values and show those bounds. They disable password
submission until an effective policy is available, and show a reload instruction when the initial fetch fails. A failed
background refetch retains the cached effective policy and permits submission under those bounds. The shared hint and
validator use the same policy; the Set password page owns its policy observer. Request-body schemas validate presence
and format; the server always enforces strength and breach policy. Login and current-password verification accept
credentials independently of new-password minimum and strength policy, including passwords shorter than 15, while
enforcing the application ceiling of 256 Unicode code points. The shared `enforce_max_length` check runs before
verification or strength, breach and hashing work. Passwords are never truncated.

A local attempt with a live anonymous session joins `sign_in_cohorts` before account lookup and password verification.
An attempt without a cookie creates no cohort or session merely to fail. The sign-in model owns this table for both
local and OIDC callers; only its conditional `claim_win` update can authorise sign-in. Rotation deletes the session's
old cohort through its foreign key.

## Runtime behaviour

**Admission and arbitration.** After the local-auth and per-source checks, `signed_in_user` refuses an already signed-in
browser with `409 sign-in-inactive`. Stale, deleted and disabled sessions are flushed and treated as anonymous. A live
anonymous session joins its cohort: an already-won cohort returns the same 409 without changing the session. A pending
cohort's id is retained until verification. The existing CSRF gate still requires the session token on a mutation
carrying `user_id`.

**Hashing and account admission.** After source/session/cohort admission, login acquires a hashing permit before `admit`
begins its transaction and locks the normalised-email row, including the initially absent-row race. The lock order is
permit, pool connection, then account row. A request waiting for capacity holds no pool connection. The cohort join in
`join_pending` precedes capacity acquisition, as it precedes account `429` and credential `422`. Capacity rejection
writes no throttle, credential or PIN state and claims no cohort win.

Login retains its permit until `Attempt::succeed` or `Attempt::fail` returns, releasing it before session login. Every
other request-path caller acquires immediately before computation and releases immediately afterwards; password policy
checks and database work stay outside those permits. Password change and PIN reset each use two separate permits for
verification and new-password hashing.

**Account admission and credential outcome.** An unexpired block rolls back acquisition and returns generic `429` plus
`Retry-After`, without account lookup or verification. Rejection changes no protection state, even for a correct
password.

An admitted request resolves the account and credential on that same connection. A known credential performs real
Argon2id verification; unknown email or missing credentials perform dummy verification. Wrong passwords and disabled
accounts share the failed-result path. `Attempt::fail` records the completed failure and commits before the generic
`422` response. The tenth committed failure still returns `422`; the next request receives `429` without hashing.

A valid enabled account claims its retained cohort through the same connection and the existing conditional `claim_win`
query. A loser returns `409` and rolls back without resetting history. A winner clears accounting and commits, releasing
the connection before session login, CSRF or cookie work. A request without a live cohort uses the same account
transaction without a cohort claim. The verified user and session-version snapshot remain unchanged.

Any error after a winning claim follows the existing session-flush rule. Accounting commits before session storage: if
storage subsequently fails, history can already be clear without a delivered authenticated session. Cancellation before
account commit rolls back; cancellation after commit does not undo the result. An unknown commit outcome is never
replayed automatically. No worker or asynchronous completion callback can write an old result after that transaction
ends. A cancelled hash caller cannot release computation capacity early: the blocking closure retains its permit clone
until Argon2 finishes, and its late result cannot write accounting.

`auth_hash_census_` route tests count twelve hash and verification sites across the seven hashing endpoints.
Administrator creation and link issuance do not hash; redemption performs one computation.
`auth_hash_login_pool_progress` holds the only permit during login, queues four other logins and checks that the
pool-backed `GET /health/ready` completes using a two-connection application pool. The `auth_hash_` unit tests cover the
active bound, wait expiry, cancellation, panic and kernel-equivalent results.

`password_admission_route_two_pools_ninth_failure_race` checks that nine failures plus two simultaneous requests from
separate application pools and sources yield one verification/`422` and one account `429`.
`password_admission_route_parity_blocked_correct_and_no_verification` checks equivalent admitted and blocked states for
known, unknown, disabled and passwordless accounts. The other `password_admission_` tests cover reset ordering,
cancellation, lost commit acknowledgement, cohort loss, storage failure and the verified version snapshot.

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
- **An inactive sign-in.** `409 sign-in-inactive` has title "Conflict" and detail "This sign-in is no longer active.
  Reload the page and sign in again." It covers an already signed-in browser and lost arbitration, including an
  already-won anonymous cohort. Switching accounts requires sign-out. When flushing succeeds after a post-win failure,
  the next attempt starts fresh. A failed flush leaves the cohort won until the session expires.
- **Rate limiting.** Source and account rejection each produce generic `429` with positive integer `Retry-After`. Source
  rejection precedes account-state creation. Account rejection precedes both real and dummy hashing. OIDC start retains
  its `303 /login?outcome=throttled` navigation and writes no attempt on rejection.
- **Hashing capacity exhausted.** After a FIFO wait of at most 30 seconds, all seven hashing credential endpoints return
  `503 auth-busy`, title "Service Unavailable" and detail "The server is busy. Try again in a moment." The response has
  no `Retry-After`. Credential forms render that detail. For login, local-auth `404`, source `429` and signed-in or
  cohort `409` precede capacity `503`; account `429` follows it. Blocking-task failures map to the generic internal
  error response.
- **Password policy rejection.** `enforce`'s `PolicyError` variants map to `AppError::Validation` (`422`) with the
  error's `Display` text as the response's `detail` field for HTTP setup, registration, credential-link redemption and
  self-service change: `TooShort`/`TooLong` name the configured bound, `TooWeak` carries zxcvbn's feedback (or a generic
  message when zxcvbn returns none), and `Breached` returns the fixed sentence "this password has appeared in a known
  data breach; choose a password you have not used elsewhere". PIN reset (the account recovery subject), discards the
  `PolicyError` entirely and answers its own fixed generic message instead, so there a weak password reads identically
  to a bad PIN. CLI bootstrap and startup seeding propagate policy rejection as a process error before creating the
  administrator. `register_weak_password_returns_422` and `register_invalid_email_returns_422` cover the client-visible
  shape of a registration rejection.
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

Equivalent admitted failures spend real or dummy Argon2id verification and return the same generic `422`. Equivalent
throttled states return generic `429` and perform neither lookup nor verification. The first dummy use initialises its
hash; known accounts require an additional credential lookup. This is equivalent verification work, not an exact total
latency guarantee.

A sustained targeted attack can repeatedly obstruct password-only sign-in, including correct credentials. Existing
sessions remain valid. Login holds at most the configured number of shared-pool connections for credential work per
process; replicas multiply that bound. With a limit above one, up to N−1 permits can wait on one email's row lock. Once
the account block is committed, later requests for that email reject without hashing. Other endpoints can still exhaust
the pool while login holds a permit, queuing all request-path hashing behind it until the 30-second wait expires.
Recovery shares the same hashing capacity. The hourly sweep provides eventual reclamation, not a hard storage bound
under unbounded arrivals. Cleanup errors log and retry later without clearing protection.

`client_ip` honours a forwarded header only when `trusted_client_ip_header` names it. Without that opt-in, clients
behind a shared proxy use its peer address as one source key. Source state is process-local; account accounting survives
restarts. `reverie unlock-account` clears an email's accounting through the same row ordering as login.

`enforce_from_config` builds its breach-check HTTP client through `crate::services::enrichment::http::api_client`, which
resolves every hostname it dials, including each redirect hop it follows, through the SSRF-filtering `ssrf_resolver`
shared with the enrichment pipeline's `cover_client`, and validates every redirect hop it follows by host
(`validate_hop`), so a redirect whose target is a bare IP address literal, which never triggers a resolver lookup, is
checked against the denied-range list before it is dialled. The breach-check URL is the public range service and the
only request content an outside party influences is the five-character hash prefix in the path.

Registration is config-gated (`self_registration_enabled`, default `false`) and the route is not mounted in the shipped
client (`frontend/src/main.tsx` mounts no `/register` route); reaching `POST /auth/register` requires calling the API
directly. `RegisterRequest` declares no role field, and the JSON extractor does not reject unrecognised fields, so a
role value supplied in the request body is silently ignored rather than causing a rejection; `register` always calls
`create_local` with `Role::Adult` as a literal argument, so privilege escalation through this path would require a
source change to that literal, not a crafted request body.

`enforce` screens every new local credential, including the first administrator created through HTTP setup, CLI
bootstrap or the startup environment seed. Policy rejection precedes hashing and credential mutation; existing
credentials remain usable without a forced reset.

CodeGuard deviation 5 in `docs/security/codeguard/README.md` records that Reverie ships no first-party MFA and that the
local password path is single-factor, listing Argon2id hashing, the two rate-limiting mechanisms and constant-work
verification among its compensating controls; this Design is the mechanism behind that entry.

## More information

- [Reference configuration](../../../../website/src/content/docs/reference/configuration.mdx): the generated
  `REVERIE_LOGIN_RATE_PER_MIN`, `REVERIE_AUTH_HASH_CONCURRENCY` and `REVERIE_PASSWORD_*` field reference.
- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 5: Reverie's single-factor local
  authentication posture and its compensating controls.
