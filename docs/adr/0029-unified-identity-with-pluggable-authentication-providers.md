---
type: ADR
profile-version: 1
id: "REV-ADR-0029"
title: "Unified identity with pluggable authentication providers"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-23"
decision-makers:
  - "John Unkovich"
---

# Unified identity with pluggable authentication providers

## Context and problem statement

Interactive login in Reverie is OIDC only (Authorization Code with PKCE). A human cannot sign in without an external
identity provider, which is a poor fit for a self-hosted product that should work out of the box. The `users` table
requires a non-null `oidc_subject`, provisioning is keyed on that subject, and the first user to complete OIDC login
is auto-promoted to administrator. Sessions are first-party (Postgres-backed through tower-sessions, 24 hour idle
expiry, server-side force-logout via `session_version`); device tokens (random 256 bit values, SHA-256 hashed,
presented over HTTP Basic) authenticate API and reader clients.

This decision adds local password login as a co-equal, first-class authentication mode without removing OIDC, and
defines how a fresh instance bootstraps and recovers its first administrator. Adding a second login mechanism touches
the authentication seam, the data model, and the security posture at once, so the direction is fixed here before
implementation.

Can Reverie support both a local-account self-hoster and an external-IdP operator through a single identity and
session model, where every provider resolves to the same in-process identity and establishes the session identically?

This decision extends the first-party session-layer decision (see More information); it reuses that layer without
changing it. API authorization (scopes, scoped tokens, resource-server token validation) is a separate decision and
is not settled here.

## Decision drivers

- A self-hosted product must be usable without mandating an external IdP, while keeping first-class IdP integration
  for operators who run one.
- Multiple authentication providers must resolve to one canonical identity and one session model, so identity gating
  and ownership are enforced once, not per provider.
- A fresh instance must reach a usable, administrator-owned state through a path that cannot be hijacked into
  granting an attacker administrator rights.
- An administrator who loses access must recover without email, consistent with NIST 800-63B disallowing email for
  out-of-band authentication.
- Password handling must follow current credential-security guidance (NIST 800-63B): hashing fit for low-entropy
  secrets, no composition or rotation theatre, and resistance to credential-stuffing without lockout-driven denial of
  service.

## Considered options

Identity schema:

- One canonical identity with separate identity and credential tables
- Widen the `users` row with nullable password columns

Password hashing:

- Argon2id PHC strings, additive and separate from the token path
- Reuse device-token SHA-256 hashing for passwords
- bcrypt

First-administrator bootstrap:

- First-run setup gated by uninitialised check, auto-promotion retired
- Keep OIDC first-user auto-promotion
- Gate first-run setup with a setup token

Account recovery (no email):

- Server-side PIN file recovery, no email
- Email-based self-service reset
- Recovery codes

Login rate limiting:

- Layered throttling over hard lockout
- Hard account lockout

## Decision outcome

Chosen option: **one canonical identity with separate identity and credential tables**, because it lets local
password login and OIDC login coexist as co-equal modes that both resolve to the same `users` row and establish the
same first-party session, so identity gating and ownership stay enforced once.

**Unified identity.** The `users` row remains the canonical identity. A new `user_identities` table (user, provider,
issuer, subject, verification state) holds external-provider links, keyed on `(issuer, subject)`: an OIDC `sub` is
unique only within its issuer (OIDC Core), so the issuer namespaces the subject and the schema stays correct across
multiple issuers and identity-provider migration. A new `local_credentials` table (user, password hash, timestamps)
holds local secrets. `users.oidc_subject` becomes nullable and ceases to be the identity key, so a credential-only
account is representable and a user can hold multiple identities without a later schema rework. Migrations are
consolidated before the first release, so no production back-compatibility step is required.

**Pluggable providers, one session.** `GET /auth/oidc/login` initiates OIDC; `POST /auth/local/login` accepts
credentials in the request body. Credentials are never placed in a URL. Both paths reuse the existing session login
routine, which regenerates the session id for fixation defence and records `session_version`.

**Password hashing.** Local passwords are hashed with Argon2id (the `argon2` crate) and stored as PHC strings, so
cost parameters can rise later without a schema change. This is additive and separate from the device-token path:
random high-entropy device tokens keep their SHA-256 hashing, which is correct for that input. Passwords are never
run through the token path.

**Bootstrap.** A first-run setup screen creates the first administrator, gated by a single predicate: no
administrator exists. The same predicate governs both the initial gate and the automatic recovery reopen, so the
setup path closes once an administrator exists and reopens whenever none does. The gate keys on
no-administrator-exists rather than an empty users table, because with self-registration enabled a non-admin user can
exist before the first administrator is created, or after every administrator is removed; an empty-table check would
then fail to reopen setup in exactly the recovery case it is meant to cover. An environment seed and a CLI command
are the headless and recovery alternatives. There is no setup token at this stage; the first-run exposure window is
handled as a documented configure-before-exposing note rather than a token gate.

**Recovery, without email.** A forgot-password flow writes a single-use, short-lived PIN to a server-side file, which
proves host access, and resets through the UI. The PIN is generated with a CSPRNG and stored hashed, not in clear, at
an operator-readable-only path outside any web-served directory, and is removed on consumption or on expiry, so it is
never world-readable and never lingers after use. A successful reset forces re-authentication and does not auto-log-in
the user, and the flow returns a generic response that does not reveal whether an account exists. Automatic first-run
reopen covers the no-administrator case, and a CLI command is the floor for when the UI cannot serve.

**Account linking.** Email is unique per instance. An OIDC login auto-links to an existing local account only when
the asserted email is verified; otherwise an administrator links the account manually.

**Account creation.** Administrators create accounts. Self-registration is a configurable option, off by default.
Child accounts are administrator-created only; they are never produced by self-registration or by an OIDC login.

**CSRF.** The CSRF synchronizer-token validating layer is enabled. Safe methods and HTTP Basic callers are exempt,
and pre-authentication flows (login, forgot-password) are exempt by design.

**Password policy.** NIST 800-63B aligned: a length minimum, no composition rules, no periodic rotation, and no
security questions. A compromised-credential check uses a range query that keeps the password on the server; it is
configurable and fails open when the service is unreachable, so an offline instance is not blocked. A strength
estimator provides feedback without imposing composition rules.

**Login rate limiting.** Layered throttling rather than hard account lockout, to avoid lockout-driven denial of
service against legitimate users. A per-source rate limit blocks an attacking origin; per-account exponential backoff
caps repeated failures without a permanent lock. The CLI unlock is a rare operator override, not the default
recovery.

**Configuration.** OIDC configuration becomes optional. New settings cover local-auth enablement, self-registration,
and the password policy. Secrets accept a file-based variant in addition to environment variables.

### Consequences

- Positive: a self-hosted instance is usable with no external IdP, while OIDC stays a first-class co-equal mode
  rather than the only door.
- Positive: every provider resolves to one `users` row and one session, so role, child status, and ownership gates
  are written and tested once.
- Positive: retiring OIDC first-user auto-promotion closes a privilege-escalation path: with a controlled bootstrap
  and multiple login routes, anyone reaching OIDC login before setup completes would otherwise become administrator.
- Positive: Argon2id with PHC strings fits low-entropy human secrets and lets cost parameters rise without a
  migration, while the device-token path keeps the hashing correct for high-entropy tokens.
- Positive: recovery never depends on email or an external mail service, which suits an air-gapped or mail-less
  self-hosted deployment and follows NIST 800-63B.
- Negative: the instance now owns two interactive credential paths and a bootstrap/recovery surface, all
  security-critical.
- Negative: the first-run window before setup completes is a real exposure surface, mitigated only by the documented
  configure-before-exposing guidance until a setup token is added later.
- Negative: the compromised-credential check is best-effort: it fails open offline, trading guaranteed enforcement for
  availability on an isolated instance.

## Pros and cons of the options

### One canonical identity with separate identity and credential tables

- Positive: the `users` row stays the single identity all providers resolve to, so gating and ownership are enforced
  once.
- Positive: multiple identities per user and credential-only accounts are representable without a later schema
  rework.
- Neutral: it adds two tables and makes `oidc_subject` vestigial.

### Widen the `users` row with nullable password columns

- Positive: it is the smallest schema change for password-only accounts.
- Negative: it cannot represent multiple external identities per user and forces a rework once a second provider or
  linking model is needed.

### Argon2id PHC strings, additive and separate from the token path

- Positive: Argon2id is the current recommendation for low-entropy human secrets and PHC strings let cost parameters
  rise without a schema change.
- Positive: keeping it separate preserves the correct SHA-256 hashing for high-entropy random device tokens.
- Neutral: it adds the `argon2` dependency.

### Reuse device-token SHA-256 hashing for passwords

- Negative: a fast unsalted-style hash is wrong for low-entropy human secrets and invites offline cracking; the
  token path's correctness depends on the input being high-entropy.

### bcrypt

- Positive: bcrypt is an accepted password hash that also slows offline cracking.
- Negative: Argon2id is the current memory-hard default; bcrypt's 72-byte input truncation and weaker
  memory-hardness make it the lesser pick when the project carries no legacy bcrypt hashes to stay compatible with.

### First-run setup gated by uninitialised check, auto-promotion retired

- Positive: the first administrator is created through a single controlled path that an external login cannot
  hijack.
- Neutral: the pre-setup window is an exposure surface handled by operator guidance rather than a token.

### Keep OIDC first-user auto-promotion

- Negative: with multiple login routes any caller reaching OIDC login before setup completes is promoted to
  administrator, a privilege-escalation bypass.

### Server-side PIN file recovery, no email

- Positive: it proves host access, needs no mail service, and aligns with NIST 800-63B disallowing email for
  out-of-band authentication.
- Neutral: it requires filesystem access to the host, which is the intended proof.

### Email-based self-service reset

- Negative: it mandates a mail service unsuitable for air-gapped self-hosting and is disallowed by NIST 800-63B for
  out-of-band authentication.

### Layered throttling over hard lockout

- Positive: per-source rate limiting plus per-account backoff resist credential stuffing without handing an attacker
  a lockout-driven denial-of-service lever against legitimate users.
- Neutral: a rare operator CLI unlock remains for the stuck case.

### Hard account lockout

- Negative: an attacker can deliberately lock out legitimate accounts, turning the defence into a
  denial-of-service vector.

## More information

- Extends the first-party session-layer decision
  ([First-party session layer on the tower-sessions core](./0015-first-party-session-layer-on-the-tower-sessions-core.md)):
  both providers reuse that layer's session login routine (`cycle_id` on login, `session_version` force-logout)
  unchanged.
- API authorization (scopes, scoped tokens, and resource-server token validation) is decided separately in the
  [API authorization model ADR](./0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) and is out of
  scope here.
- Deferred to later work, named here so the boundary is explicit: multi-factor authentication, email-based
  self-service reset, recovery codes, a first-run setup token, trusting edge-asserted identity, administrator
  impersonation, WebAuthn/FIDO2 passkeys, and a multi-issuer runtime. Passkeys land in their own credentials table
  (one user to many), never in `user_identities` (federated links) or `local_credentials` (one password hash); the
  canonical `users` design absorbs them with no identity rework. The multi-issuer runtime (multiple configured
  issuers, provider selection) is future work; the schema is already issuer-keyed, but only one issuer is configured
  at runtime.
- The authentication changes (retirement of first-user auto-promotion, the bootstrap and recovery model, account
  linking) require updates to the authentication entries in the security control set as part of the implementation,
  not as a follow-up.
- Standards basis: NIST 800-63B (password length over composition, no rotation, no email for out-of-band
  authentication), OWASP Session Management (fixation defence and server-side invalidation, carried by the
  session-layer decision), and the OWASP synchronizer-token pattern for CSRF.
