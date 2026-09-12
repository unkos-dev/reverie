---
type: REQ
profile-version: 1
id: "REV-REQ-0024"
title: "A request authenticated without a browser session is not CSRF-gated"
governed-by:
  - "REV-ADR-0011"
---

# A request authenticated without a browser session is not CSRF-gated

## Statement

A request that presents no session naming a user, whatever its method, MUST NOT be refused for a missing or mismatched
CSRF token, including a request authenticated by a device token or HTTP Basic credentials.

## Rationale

The OWASP CSRF threat model rests on the browser automatically attaching cookies to a cross-site request; an
`Authorization` header is never attached automatically by a browser, so a request that carries only one cannot be forged
that way, and gating it on a token it was never asked to send would refuse every reader app and device-token consumer
for no security benefit. See the
[OWASP CSRF cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).
[REV-ADR-0028](../../../adr/0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) and
[REV-ADR-0029](../../../adr/0029-unified-identity-with-pluggable-authentication-providers.md) record the credential
types this exemption covers.

## Acceptance criteria

- A `POST` mutation authenticated by HTTP Basic credentials, carrying no `X-CSRF-Token` header, reaches its handler
  rather than being refused for a missing or mismatched token. Checked by `change_own_password_oidc_only_returns_422` in
  `backend/src/routes/users/tests.rs` and the `create_token_*` tests in `backend/src/routes/tokens.rs`.
- A `POST` mutation authenticated by a bearer device token or JWT, carrying no `X-CSRF-Token` header, reaches its
  handler rather than being refused for a missing or mismatched token. No automated check exercises this case directly;
  it is verified by inspection of `csrf_required` in `backend/src/security/csrf.rs`, whose exemption reads only the
  session's `user_id` claim and does not distinguish a Bearer-authenticated request from any other session-less request.
