---
type: REQ
profile-version: 1
id: "REV-REQ-0067"
title: "An OIDC identity resolves to an account on issuer and subject, never on email"
governed-by:
  - "REV-ADR-0029"
---

# An OIDC identity resolves to an account on issuer and subject, never on email

## Statement

WHEN an OIDC sign-in presents a verified ID token, the system MUST resolve the account from the token's issuer and
subject alone and MUST NOT use the token's email claim to select, link to, or modify any account.

## Rationale

The provider is trusted for the identity it asserts, not for the address it releases. OpenID Connect Core names the
issuer and subject together as the only claims a relying party can rely on as a stable identifier (see
[Claim stability and uniqueness](https://openid.net/specs/openid-connect-core-1_0.html#ClaimStability)). Reverie does
not read `email_verified`, so a provider that releases an unverified or attacker-chosen address would otherwise let its
holder sign in as, or be attached to, whichever account owns that address, including an administrator's.

## Acceptance criteria

- An account is selected by issuer and subject: one subject under two issuers resolves to two distinct accounts. Checked
  by `same_subject_distinct_issuers_are_distinct_users` in `backend/src/models/user.rs`.
- A returning identity whose email claim equals another account's email signs in as its own account, and the other
  account is neither selected nor linked. Checked by
  `returning_identity_signs_in_when_its_claim_matches_another_account` in `backend/src/models/user.rs`.
