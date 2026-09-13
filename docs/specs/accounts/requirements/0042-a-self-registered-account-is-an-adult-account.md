---
type: REQ
profile-version: 1
id: "REV-REQ-0042"
title: "A self-registered account is an adult account"
governed-by:
  - "REV-ADR-0029"
---

# A self-registered account is an adult account

## Statement

WHEN an account is created through self-registration, the system MUST create it with the adult role, MUST NOT create it
as an administrator or a child account, and MUST ignore any role value supplied in the registration request rather than
acting on it.

## Rationale

Self-registration is reachable without an existing administrator's involvement, so a registration path that honoured a
caller-supplied role would let any requester mint an administrator or bypass child-content restrictions.
[REV-ADR-0029](../../../adr/0029-unified-identity-with-pluggable-authentication-providers.md) fixes self-registration as
adult-only and off by default, with every other role reserved to an existing administrator acting through the account
administration surface.

## Acceptance criteria

- A registration request creates an account with the adult role. Checked by `register_creates_adult_when_enabled` in
  `backend/src/routes/auth.rs`.
- A role value supplied in the registration request body has no effect on the created account's role. Verified by
  inspection of the registration request type in `backend/src/routes/auth.rs`, which declares no role field, and of the
  registration handler, which passes a fixed adult role literal to account creation regardless of the request body's
  contents.
