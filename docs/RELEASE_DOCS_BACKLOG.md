# Release Documentation Backlog

Operator- and user-facing documentation deferred until the relevant
product surface is built out. Each item names the decision or feature
whose rationale needs a proper Starlight page before a public release,
rather than a half-built page written ahead of the surface it documents.

This file is the holding area; items graduate into `docs/` Starlight
pages when their surface lands.

## Items

### `validation_status` operator semantics

**Source:** [`adr/2026-05-28-validation-status-vocabulary.md`](../adr/2026-05-28-validation-status-vocabulary.md) (UNK-276)

The `validation_status` enum is `pending | clean | repaired | degraded`.
The distinction is not self-evident to an operator reading the value:

- `pending` — the manifestation row exists but structural validation has
  not run yet.
- `clean` — validation found no issues.
- `repaired` — validation found issues that were automatically repaired;
  the file is ingested, stored, and served.
- `degraded` — validation found issues that are tolerated; the file is
  still served.

The load-bearing point operators need: `clean`, `repaired`, and
`degraded` are **all** stored-and-served outcomes on one quality tier —
`clean` means _no issues found_, not _the only valid state_. A
quarantined file is never represented here because quarantine deletes the
file and writes no row.

Write an operator-facing Starlight page covering these states (and how
quarantine differs) when the library/validation UI surface that exposes
them lands. The dev-facing reference in
[`docs/schema.md`](./schema.md) is already corrected.

### OIDC `email` claim: addr-spec validation and degrade-to-NULL

**Source:** `backend/src/models/user.rs`
(`is_addr_spec`, `upsert_from_oidc_and_maybe_promote`) — UNK-309

The OIDC `email` claim is signature-verified but not format-checked
upstream. Reverie validates it against RFC 5322 _addr-spec_ rules before
persisting. Two operator-visible behaviours:

- **Invalid format degrades to NULL, not a login failure.** A malformed
  claim (display-name form `Alice <alice@example.com>`, domain-literal
  `alice@[127.0.0.1]`, or a non-email string) is discarded and
  `users.email` stored as `NULL`. Login still succeeds — identity is the
  OIDC `sub`, not the email claim (OIDC Core §5.7: email is optional and
  non-identifying).
- **Malformed claim on re-login overwrites a previously-stored valid
  email to NULL.** If an IdP changes from a valid to an invalid claim, the
  stored email is cleared on next login. The rejection is logged at `warn`
  with a `had_prior_email` field so operators can tell a known-good value
  being wiped (IdP misconfiguration) apart from a first-login carrying junk.

Write an operator-facing Starlight page covering email-claim validation
behaviour when the admin user-management surface lands.

### Admin `PATCH /api/users/{id}`: addr-spec email validation

**Source:** `backend/src/routes/users/mod.rs` — UNK-309

The admin `PATCH /api/users/{id}` endpoint validates the email field
against the same RFC 5322 _addr-spec_ rules as the OIDC path
(`is_addr_spec`). This tightens the prior `EmailAddress::is_valid` check,
which accepted display-name (`Alice <alice@example.com>`) and
domain-literal (`alice@[127.0.0.1]`) forms — both now rejected with 422.
Email changes and clears do **not** bump `session_version`: email is not
an access-control input (login identity is the OIDC `sub`, RLS keys on
user id/role/`is_child`), so no active session needs invalidating.

Write an operator-facing Starlight page documenting these constraints when
the admin user-management UI lands.
