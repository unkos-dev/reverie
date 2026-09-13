---
type: REQ
profile-version: 1
id: "REV-REQ-0030"
title: "A disabled account is refused on every credential type"
governed-by:
  - "REV-ADR-0028"
---

# A disabled account is refused on every credential type

## Statement

A disabled account MUST be refused whichever credential type presents it: a browser session, HTTP Basic credentials, a
bearer device token, or a bearer access token. The refusal MUST answer with HTTP status 401.

## Rationale

Disabling an account is an administrative action, and it must take effect on every transport at once: a disable that
stopped a browser session but left a cached Basic credential or a bearer token still working would leave the very
capability an administrator meant to withdraw reachable through a side door. See the
[OWASP Authentication cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html).

## Acceptance criteria

- A disabled account's live browser session is refused on its next request. Checked by
  `disabled_user_live_session_is_rejected` in `backend/src/routes/auth.rs`.
- A disabled account's device token is refused. Checked over HTTP Basic by `disabled_user_device_token_is_rejected` in
  `backend/src/routes/auth.rs`; a bearer device token resolves through the identical lookup, so this one test covers
  both transports.
- A disabled account's bearer access token (RFC 9068 resource-server JWT) is refused. Checked by
  `disabled_account_rejected_via_jwt` in `backend/src/auth/middleware.rs`.
