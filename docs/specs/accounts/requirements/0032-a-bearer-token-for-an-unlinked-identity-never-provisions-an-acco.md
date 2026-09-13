---
type: REQ
profile-version: 1
id: "REV-REQ-0032"
title: "A bearer token for an unlinked identity never provisions an account"
governed-by:
  - "REV-ADR-0028"
---

# A bearer token for an unlinked identity never provisions an account

## Statement

A bearer access token whose issuer and subject are not linked to an existing account MUST be refused with HTTP status
401 and MUST NOT create an account or an identity link. Only the interactive OIDC sign-in flow provisions an account.

## Rationale

A resource-server access token proves that its issuer authenticated the subject; it does not prove that the operator has
decided to admit that person to this instance. Provisioning an account from an unattended bearer token would let anyone
who can obtain a token from a trusted issuer create themselves an account without an administrator or the interactive
sign-in flow ever making that decision. See the
[OWASP Authentication cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html).
[REV-ADR-0028](../../../adr/0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) and
[REV-ADR-0029](../../../adr/0029-unified-identity-with-pluggable-authentication-providers.md) record the identity and
authorisation model this obligation keeps honest.

## Acceptance criteria

- A well-formed, validly signed bearer access token whose `(issuer, subject)` pair has no linked account is refused with
  HTTP status 401 and creates no account or identity link. Checked by
  `unknown_oidc_identity_rejected_and_not_provisioned` in `backend/src/auth/middleware.rs`, which re-checks that no link
  exists after the request completes.
