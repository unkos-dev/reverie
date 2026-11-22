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
  the account and identity-link counts unchanged. Checked by `external_bearer_rejected_without_identity_writes` in
  `backend/src/auth/middleware.rs`.
- An external bearer token for an unlinked identity cannot authenticate an operation requiring authentication and leaves
  the account and identity-link counts unchanged. Checked by `external_bearer_rejected_without_identity_writes`.

## More information

The status and challenge of an authentication failure are separate obligations: REV-REQ-0005 and REV-REQ-0033 for
`/api/v1`, and REV-REQ-0084 for GraphQL. An operation that requires no authentication does not acquire one from this
Requirement.
