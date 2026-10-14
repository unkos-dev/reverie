---
type: REQ
profile-version: 1
id: "REV-REQ-0032"
title: "External bearer tokens never authenticate API requests"
governed-by:
  - "REV-ADR-0056"
---

# External bearer tokens never authenticate API requests

## Statement

An externally issued bearer token MUST NOT authenticate an API request or create an account or identity link.

## Rationale

Interactive OIDC sign-in establishes the local account and browser session through which a provider identity uses
Reverie. A provider's access token does not grant direct API access, even when its subject already has a linked account.
OIDC account provisioning belongs to that interactive flow; local registration and administrator setup remain separate
account-creation paths.

## Acceptance criteria

- An external bearer token for a linked identity cannot authenticate an operation requiring authentication and leaves
  the account and identity-link counts unchanged.
- An external bearer token for an unlinked identity cannot authenticate an operation requiring authentication and leaves
  the account and identity-link counts unchanged.
- Both cases are checked by `external_bearer_rejected_without_identity_writes` in `backend/src/auth/middleware.rs`.
  Authentication-failure status and challenges are governed by REV-REQ-0005 and REV-REQ-0033; public operations do not
  acquire an authentication requirement from this obligation.
