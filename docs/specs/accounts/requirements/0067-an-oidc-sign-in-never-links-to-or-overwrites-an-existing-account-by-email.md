---
type: REQ
profile-version: 1
id: "REV-REQ-0067"
title: "An OIDC sign-in never links to or overwrites an existing account by email"
governed-by:
  - "REV-ADR-0029"
---

# An OIDC sign-in never links to or overwrites an existing account by email

## Statement

WHEN an OIDC sign-in presents a verified ID token, the system MUST resolve the account from the token's issuer and
subject alone and MUST NOT use the token's email claim to select, link to, or modify any account. WHEN the issuer and
subject are linked to no account and the email claim equals, ignoring case and surrounding whitespace, the email of an
existing account, the system MUST refuse the sign-in with HTTP status 401 and the same problem body as a sign-in by a
disabled account, and MUST NOT create a user, create an identity link, create a session, or modify the existing account.
WHEN the issuer and subject are already linked to an account, the system MUST leave that account's stored email
unchanged, whatever email the provider now releases.

## Rationale

The provider is trusted for the identity it asserts, not for the address it releases. OpenID Connect Core names the
issuer and subject together as the only claims a relying party can rely on as a stable identifier (see
[Claim stability and uniqueness](https://openid.net/specs/openid-connect-core-1_0.html#ClaimStability)). Reverie does
not read `email_verified`, so a provider that releases an unverified or attacker-chosen address would otherwise let its
holder sign in as, or be attached to, whichever account owns that address, including an administrator's. A refusal that
differs from the disabled-account outcome would also confirm that the address belongs to an account. Overwriting a
returning identity's stored email from the provider would let a later claim change which address the account holds, and
collide with another account's.
[REV-ADR-0029](../../../adr/0029-unified-identity-with-pluggable-authentication-providers.md) records the matching model
that this obligation binds.

## Acceptance criteria

- A first sign-in whose email differs only in case or surrounding whitespace from another account's is refused: the
  provisioning call returns the collision error, writes no user row or identity link, and leaves the other account's
  `updated_at` unchanged. Checked by `first_login_with_an_email_held_by_another_account_is_refused` in
  `backend/src/models/user.rs`.
- The same first sign-in through `GET /auth/callback` is answered with the 401 problem response, writes no session user
  or session version, and leaves `/auth/me` unauthenticated. It adds no user row and no identity link for the issuer and
  subject, and neither links the existing account nor changes its display name or `updated_at`. Checked by
  `callback_refuses_an_email_held_by_another_account_and_writes_nothing` in `backend/src/routes/auth.rs`.
- The collision refusal and the disabled-account refusal are indistinguishable to the caller. Both are checked through
  the shared `refused_callback` helper, which asserts the same status and problem type, in
  `callback_refuses_an_email_held_by_another_account_and_writes_nothing` and
  `callback_refuses_disabled_account_without_writing_profile` in `backend/src/routes/auth.rs`. That both outcomes return
  the same `AppError::Unauthorized` is verified by inspection of the `callback` handler in the same file.
- A first sign-in whose email no other account holds still provisions an account with that email. Checked by
  `first_login_without_a_colliding_email_still_provisions` in `backend/src/models/user.rs`.
- An account is selected by issuer and subject: one subject under two issuers resolves to two distinct accounts. Checked
  by `same_subject_distinct_issuers_are_distinct_users` in `backend/src/models/user.rs`.
- A returning identity whose email claim changes to another valid address, to an invalid value, or to absent signs in as
  the same account with its stored email unchanged. Checked by
  `returning_identity_keeps_its_stored_email_when_the_claim_changes` in `backend/src/models/user.rs`.
- A returning identity whose email claim now equals another account's email still signs in as its own account, with its
  own stored email. Checked by `returning_identity_signs_in_when_its_claim_matches_another_account` in
  `backend/src/models/user.rs`.
