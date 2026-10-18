---
type: REQ
profile-version: 1
id: "REV-REQ-0068"
title: "A first OIDC sign-in whose email another account holds is refused and writes nothing"
governed-by:
  - "REV-ADR-0029"
---

# A first OIDC sign-in whose email another account holds is refused and writes nothing

## Statement

WHEN an OIDC sign-in presents an issuer and subject that are linked to no account and an email claim equal, ignoring
case and surrounding whitespace, to the email of an existing account, the system MUST refuse the sign-in with HTTP
status 303 to `/login?outcome=account` and the same generic response as a sign-in by a disabled account, and MUST NOT
create a user, create an identity link, create a session, or modify the existing account.

## Rationale

A provider can release an address it has not verified. Attaching the new identity to the account that holds the address,
or provisioning a second account with it, would hand the address's owner to whoever controls the provider's claim,
including an administrator's account. A refusal that differs from the disabled-account outcome would also confirm to the
caller that the address belongs to an account.

## Acceptance criteria

- A first sign-in whose email differs only in case or surrounding whitespace from another account's is refused: the
  provisioning call returns the collision error, writes no user row or identity link, and leaves the other account's
  `updated_at` unchanged. Checked by `first_login_with_an_email_held_by_another_account_is_refused` in
  `backend/src/models/user.rs`.
- The same first sign-in through `GET /auth/callback` is answered with the 303 `/login?outcome=account` redirect, writes
  no session user or session version, and leaves `/auth/me` unauthenticated. It adds no user row and no identity link
  for the issuer and subject, and neither links the existing account nor changes its display name or `updated_at`.
  Checked by `callback_refuses_an_email_held_by_another_account_and_writes_nothing` in `backend/src/routes/auth.rs`.
- The collision refusal is indistinguishable to the caller from the disabled-account refusal. Both are driven through
  `GET /auth/callback`, and their status, Location, empty body, response headers and `Set-Cookie` attributes are equal.
  Checked by `email_collision_refusal_is_indistinguishable_from_a_disabled_account` in `backend/src/routes/auth.rs`.
- A first sign-in whose email no other account holds still provisions an account with that email. Checked by
  `first_login_without_a_colliding_email_still_provisions` in `backend/src/models/user.rs`.
