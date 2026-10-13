---
type: REQ
profile-version: 1
id: "REV-REQ-0069"
title: "A returning OIDC identity keeps its stored email"
governed-by:
  - "REV-ADR-0029"
---

# A returning OIDC identity keeps its stored email

## Statement

WHEN an OIDC sign-in presents an issuer and subject that are already linked to an account, the system MUST leave that
account's stored email unchanged, whatever email claim the provider now releases.

## Rationale

The stored email is the address the account holds. Overwriting it from the provider would let a later claim change which
address the account holds, and collide with the address of another account.

## Acceptance criteria

- A returning identity whose email claim changes to another valid address, to an invalid value, or to absent signs in as
  the same account with its stored email unchanged. Checked by
  `returning_identity_keeps_its_stored_email_when_the_claim_changes` in `backend/src/models/user.rs`.
- A returning identity whose email claim now equals another account's email signs in with its own stored email
  unchanged. Checked by `returning_identity_signs_in_when_its_claim_matches_another_account` in
  `backend/src/models/user.rs`.
