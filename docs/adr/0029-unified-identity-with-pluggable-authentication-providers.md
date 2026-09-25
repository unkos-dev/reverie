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
requires a non-null `oidc_subject`, provisioning is keyed on that subject, and the first user to complete OIDC login is
auto-promoted to administrator. Sessions are first-party (Postgres-backed through tower-sessions, 24 hour idle expiry,
server-side force-logout via `session_version`); device tokens (random 256 bit values, SHA-256 hashed, presented over
HTTP Basic) authenticate API and reader clients.

This decision adds local password login as a co-equal, first-class authentication mode without removing OIDC, and
defines how a fresh instance bootstraps and recovers its first administrator. Adding a second login mechanism touches
the authentication seam, the data model, and the security posture at once, so the direction is fixed here before
implementation.

Can Reverie support both a local-account self-hoster and an external-IdP operator through a single identity and session
model, where every provider resolves to the same in-process identity and establishes the session identically?

This decision extends the first-party session-layer decision (see More information); it reuses that layer without
changing it. API authorization (scopes, scoped tokens, resource-server token validation) is a separate decision and is
not settled here.

## Decision drivers

- A self-hosted product must be usable without mandating an external IdP, while keeping first-class IdP integration for
  operators who run one.
- Multiple authentication providers must resolve to one canonical identity and one session model, so identity gating and
  ownership are enforced once, not per provider.
- A fresh instance must reach a usable, administrator-owned state through a path that cannot be hijacked into granting
  an attacker administrator rights.
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

Chosen option: **one canonical identity with separate identity and credential tables**, because it lets local password
login and OIDC login coexist as co-equal modes that both resolve to the same `users` row and establish the same
first-party session, so identity gating and ownership stay enforced once.

A user may hold local credentials and external-provider identities, all resolving to one first-party session. External
subjects are namespaced by issuer. Local passwords use Argon2id; random device tokens retain their separate hashing
path. The chosen model also provides first-administrator bootstrap, host-verified password recovery, verified-email
account linking, optional self-registration, synchronizer-token CSRF protection, password-strength and breach checks,
and throttled login without permanent lockout.

### Consequences

- Positive: a self-hosted instance is usable with no external IdP, while OIDC stays a first-class co-equal mode rather
  than the only door.
- Positive: every provider resolves to one `users` row and one session, so role, child status, and ownership gates are
  written and tested once.
- Positive: retiring OIDC first-user auto-promotion closes a privilege-escalation path: with a controlled bootstrap and
  multiple login routes, anyone reaching OIDC login before setup completes would otherwise become administrator.
- Positive: Argon2id with PHC strings fits low-entropy human secrets and lets cost parameters rise without a migration,
  while the device-token path keeps the hashing correct for high-entropy tokens.
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
- Positive: multiple identities per user and credential-only accounts are representable without a later schema rework.
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

- Negative: a fast unsalted-style hash is wrong for low-entropy human secrets and invites offline cracking; the token
  path's correctness depends on the input being high-entropy.

### bcrypt

- Positive: bcrypt is an accepted password hash that also slows offline cracking.
- Negative: Argon2id is the current memory-hard default; bcrypt's 72-byte input truncation and weaker memory-hardness
  make it the lesser pick when the project carries no legacy bcrypt hashes to stay compatible with.

### First-run setup gated by uninitialised check, auto-promotion retired

- Positive: the first administrator is created through a single controlled path that an external login cannot hijack.
- Neutral: the pre-setup window is an exposure surface handled by operator guidance rather than a token.

### Keep OIDC first-user auto-promotion

- Negative: with multiple login routes any caller reaching OIDC login before setup completes is promoted to
  administrator, a privilege-escalation bypass.

### Server-side PIN file recovery, no email

- Positive: it proves host access, needs no mail service, and aligns with NIST 800-63B disallowing email for out-of-band
  authentication.
- Neutral: it requires filesystem access to the host, which is the intended proof.

### Email-based self-service reset

- Negative: it mandates a mail service unsuitable for air-gapped self-hosting and is disallowed by NIST 800-63B for
  out-of-band authentication.

### Layered throttling over hard lockout

- Positive: per-source rate limiting plus per-account backoff resist credential stuffing without handing an attacker a
  lockout-driven denial-of-service lever against legitimate users.
- Neutral: a rare operator CLI unlock remains for the stuck case.

### Hard account lockout

- Negative: an attacker can deliberately lock out legitimate accounts, turning the defence into a denial-of-service
  vector.

## More information

- Extends the first-party session-layer decision
  ([First-party session layer on the tower-sessions core](./0015-first-party-session-layer-on-the-tower-sessions-core.md)):
  both providers reuse that layer's session login routine (`cycle_id` on login, `session_version` force-logout)
  unchanged.
- API authorization (scopes, scoped tokens, and resource-server token validation) is decided separately in the
  [API authorization model ADR](./0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) and is out of
  scope here.
- Deferred to later work, named here so the boundary is explicit: multi-factor authentication, email-based self-service
  reset, recovery codes, a first-run setup token, trusting edge-asserted identity, administrator impersonation,
  WebAuthn/FIDO2 passkeys, and a multi-issuer runtime. Passkeys land in their own credentials table (one user to many),
  never in `user_identities` (federated links) or `local_credentials` (one password hash); the canonical `users` design
  absorbs them with no identity rework. The multi-issuer runtime (multiple configured issuers, provider selection) is
  future work; the schema is already issuer-keyed, but only one issuer is configured at runtime.
- Standards basis: NIST 800-63B (password length over composition, no rotation, no email for out-of-band
  authentication), OWASP Session Management (fixation defence and server-side invalidation, carried by the session-layer
  decision), and the OWASP synchronizer-token pattern for CSRF.
