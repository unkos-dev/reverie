# Reverie Security Reference

Reverie applies these security rules to implementation work in the areas they
cover. The project imports them verbatim from an external source under CC BY
4.0. Agents must follow the applicable rules when work touches user input,
authentication, authorization, sessions, secrets, file I/O, XML parsing,
outbound HTTP, response headers, or supply-chain controls.

These files complement (do not replace) the existing security tooling in this
repo: the `security-reviewer` agent, the `security-review` skill, the
`security-scan` skill, and the `prp-core:silent-failure-hunter` agent. Those
tools critique written code; these rules guide the initial draft.

## Source

- **Project:** [Project CodeGuard](https://github.com/cosai-oasis/project-codeguard) — Coalition for Secure AI (CoSAI), an OASIS Open Project
- **Upstream commit:** `00f6263a8fe992ad68222fd898551e5c7edbbaf9`
- **License:** [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) — attribution in each file's top-comment provenance header
- **Imported:** 2026-04-24

Files named `codeguard-*.md` belong to the upstream source. Do not edit them.
Refresh them through the documented import process only when the user requests
an upstream refresh. This README belongs to Reverie and records approved
project-specific deviations.

## File manifest

### Core (foundational categories)

| File | Relevance to reverie |
|---|---|
| `codeguard-0-input-validation-injection.md` | SQL injection (sqlx), OS command injection, XML/LDAP injection |
| `codeguard-0-authentication-mfa.md` | OIDC via Authentik, device-token path, MFA posture |
| `codeguard-0-authorization-access-control.md` | Row-level security, IDOR prevention |
| `codeguard-0-session-management-and-cookies.md` | tower-sessions, cookie flags, expiry discipline |
| `codeguard-0-supply-chain-security.md` | cargo deny, npm audit, Docker image pinning, SBOM |
| `codeguard-0-file-handling-and-uploads.md` | EPUB ingestion path, magic-byte validation, safe storage |
| `codeguard-0-xml-and-serialization.md` | OPDS feed generation, EPUB XML parsing, XXE prevention |
| `codeguard-0-logging.md` | tracing discipline, redaction, no-secret-leakage |
| `codeguard-0-client-side-web-security.md` | Foundation for frontend hardening |

### OWASP deep-dives (attack-specific)

| File | Relevance to reverie |
|---|---|
| `codeguard-0-content-security-policy.md` | CSP and header hardening input |
| `codeguard-0-http-headers.md` | CSP and header hardening input |
| `codeguard-0-http-strict-transport-security.md` | HSTS posture behind reverse proxy |
| `codeguard-0-clickjacking-defense.md` | Frame-ancestors / X-Frame-Options |

## How agents use this

Open the imported file that covers the code you are changing and follow every
applicable rule. Do not assess whether implementation work should amend an
imported rule file.

If a rule conflicts with required Reverie behavior, stop and ask the user to
approve a deviation. After approval, record the deviation in this README with
its rationale and compensating controls. Do not proceed on an unapproved
deviation. CodeGuard requires no routine task-summary statement.

For categories not covered here (e.g., mobile, k8s, C-language safety), use the
external canon pathway: WebFetch from the upstream repo at the pinned commit.

## Refresh policy

Re-pull from upstream when:

- A Reverie security review surfaces a gap that's since been covered upstream
- Upstream publishes a significant version update (watch releases)
- At minimum: **quarterly check** against upstream `main`

Refresh process:

1. Note the new upstream commit SHA
2. Run the import script (see commit message of the initial import for the shell
   command that fetched these files) with the new SHA
3. Review the diff against your local files — flag any content change that
   conflicts with reverie's existing rules
4. Update this README's "Upstream commit" field and "Imported" date
5. Commit as `chore(security): refresh codeguard rules (<short-sha-old>..<short-sha-new>)`

## Deviations

Log any reverie-specific override here with the overridden file + section,
reverie's position, rationale, and compensating controls. Prefer documenting
the deviation over editing the imported file.

### 1. Session cookie `Secure` flag omitted

**Override:** `codeguard-0-session-management-and-cookies.md` → "Cookie Security
Configuration" — mandates `Secure` on session cookies.

**Reverie's position:** `Secure` is omitted. See
`backend/src/lib.rs::build_router_with_session_store` (session setup).

**Rationale:** The backend runs behind a TLS-terminating reverse proxy and
sees plain HTTP. Setting `Secure` on the cookie would prevent delivery over
the plaintext hop between proxy and backend.

**Compensating controls:** TLS enforced at the reverse proxy boundary.
Deployments must ensure the proxy-to-backend hop is not routed over an
untrusted network.

### 2. `SameSite=Lax` instead of `Strict`

**Override:** `codeguard-0-session-management-and-cookies.md` → "Cookie Security
Configuration" — prefers `SameSite=Strict`; allows `Lax` "if necessary for
flows".

**Reverie's position:** `SameSite::Lax`.

**Rationale:** The OIDC authorization-code redirect from the IdP back to
reverie is a cross-site POST under Strict semantics and would be blocked.
`Lax` permits the redirect to complete while still blocking most CSRF vectors.

**Compensating controls:** PKCE + state parameter validation on the OIDC
flow; session regeneration on authentication.

### 3. 24-hour idle session expiry

**Override:** `codeguard-0-session-management-and-cookies.md` → "Expiration and
Logout" — prefers non-persistent cookies with 2–30 min idle timeouts.

**Reverie's position:** `Expiry::OnInactivity(24h)` via `tower-sessions`.

**Rationale:** Reverie is a personal-library application with long-running
read sessions, not a high-value admin surface. A 24-hour idle window reflects
the usage pattern; shorter timeouts would force repeated re-authentication
during a natural reading session.

**Compensating controls:** Sessions regenerate on authentication; logout
invalidates the server-side session immediately. Admin-equivalent operations
(if introduced) must use a stricter session context.

### 4. EPUB ingestion processes ZIP archives

**Override:** `codeguard-0-file-handling-and-uploads.md` → "File Content
Validation" — "Avoid ZIP files due to numerous attack vectors."

**Reverie's position:** EPUB format **is** a ZIP archive; reverie cannot
function without parsing them.

**Rationale:** Unavoidable — EPUB is the primary ingestion target.

**Compensating controls required:**

- Magic-byte validation (confirm ZIP signature before processing)
- Bounded decompression guards against zip-bomb patterns (max decompressed
  size, max entry count, max nesting depth)
- Generated filenames for extracted content; never trust manifest-provided
  paths
- Extracted content stored outside web root
- EPUB parser runs on the ingestion pool with scoped RLS
- Cleanup deletion bounded to the ingestion root: `cleanup_batch` rejects any
  caller-supplied path whose canonicalised parent (files) or canonicalised self
  (directories) resolves outside the ingestion tree — directory pruning landed
  in PR #387, file deletion in PR #388

Any of these currently missing is a security bug. Verification tracked
separately — see the conflict-check comment on PR #40.

### 5. First-party MFA deferred

**Override:** `codeguard-0-authentication-mfa.md` expects multi-factor
authentication on the authentication path.

**Reverie's position:** Reverie ships no first-party MFA (TOTP, WebAuthn, or
recovery codes). The OIDC path delegates step-up and second factors to the
external identity provider; the local password path is single-factor.

**Rationale:** Reverie is a self-hosted personal-library application. The
local-account path exists so an operator can run it without an external IdP at
all; mandating a second factor there would block the out-of-the-box case. An
operator who needs MFA configures OIDC and enforces it at the provider.

**Compensating controls:** Argon2id password hashing (m=19456, t=2, p=1);
per-source rate limiting plus a DB-backed per-account backoff on login;
constant-work verification so login latency does not reveal account existence;
the bootstrap path is the only way to mint the first administrator. First-party
MFA is a planned later step.

### 6. Email-less PIN password recovery

**Override:** Standard recovery guidance assumes an email-based reset link.
Reverie has no outbound email and must be recoverable on an isolated host.

**Reverie's position:** Forgot-password writes a CSPRNG PIN to a per-user
operator-readable file (`<recovery_pin_dir>/<user_id>.pin`, mode 0600, in a
directory created mode 0700, outside any web-served directory) as
proof-of-host-access; the user completes the reset with that PIN. The per-user
path keeps concurrent recoveries for different accounts from colliding. See
`backend/src/auth/recovery.rs` and the `/auth/forgot-password` +
`/auth/reset-password` handlers.

**Rationale:** Recovery must work with no email provider and no external
dependency, while still requiring possession of a host-access secret rather than
just knowledge of an email address.

**Compensating controls:**

- PIN generated from the OS CSPRNG; stored at rest only as an Argon2id hash, never
  in clear in the database
- Single-use: consumption is a row-atomic guarded update, so a PIN cannot be
  redeemed twice
- Short expiry (`recovery_pin_ttl_secs`, default 15 minutes); at most one active
  PIN per user (a new request supersedes the prior one)
- DB-row-before-file on issue and consume-before-file-removal on reset, so a
  crash never leaves a usable cleartext PIN without a live row
- Per-source rate limiting on both recovery endpoints
- Generic responses on both endpoints and equivalent cryptographic work on the
  unknown-account path, so neither response nor timing enumerates accounts
- No auto-login after reset: the user must re-authenticate with the new password

### 7. Scope authorization verified by a generated-spec CI matrix, not by this list

**Override:** none. `codeguard-0-authorization-access-control.md`'s own "Testing
and Automation" section asks for an authorization matrix that iterates
endpoints and roles and drives CI, and this entry records where reverie keeps
that matrix, since it is enforcement infrastructure rather than a single
handler decision.

**Reverie's position:** `backend/src/authz_matrix.rs` parses the OpenAPI
document generated in-process (not the committed `docs/openapi.json`) and
asserts, for every `/api/v1` operation, that it declares a scope requirement
and that a credential one level below the required scope in the hierarchy
(`read` < `write` < `admin`) is rejected with 403, while a scopeless credential
is rejected at the auth seam. A companion check flags any mutating-verb
(`POST`/`PUT`/`PATCH`/`DELETE`) operation that does not require at least
`write`, with a small explicit allow-list, logged rather than silent, for the
rare side-effect-free exception (the enrichment dry-run, a `POST` that computes
a preview without persisting).
Mint requests for personal tokens go through a mass-assignment allow-list DTO
(`CreateTokenRequest` in `backend/src/routes/tokens.rs`: exactly `name`,
`scopes`, `expires_in_days`), so no other field on the token row is settable at
mint.

**Rationale:** A per-handler review cannot guarantee that a later PR adding or
renaming an operation keeps its scope gate. Parsing the in-process spec ties
the test to the same source the client-facing contract is generated from, so
an operation cannot drift from documented to undocumented, or from gated to
ungated, without failing the build.

**Compensating controls:** n/a — this entry documents reverie's mechanism for
satisfying the guard's own deny-by-default and testing-and-automation
principles, not a departure from them.
