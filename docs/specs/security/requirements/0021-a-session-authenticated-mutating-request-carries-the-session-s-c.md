---
type: REQ
profile-version: 1
id: "REV-REQ-0021"
title: "A session-authenticated mutating request carries the session's CSRF token"
governed-by:
  - "REV-ADR-0011"
---

# A session-authenticated mutating request carries the session's CSRF token

## Statement

WHEN a request whose method is not `GET`, `HEAD`, `OPTIONS` or `TRACE` is authenticated by a browser session, the server
MUST answer `428 Precondition Required` with the `csrf-missing` problem type if the request carries no `X-CSRF-Token`
header, MUST answer `403 Forbidden` with the `csrf-mismatch` problem type if the header's value is not identical to the
token the session holds, and MUST NOT run the operation in either case.

## Rationale

The OWASP synchronizer token pattern requires the check on every state-changing request a browser session can issue;
[RFC 9110 §9.2.1](https://www.rfc-editor.org/rfc/rfc9110#section-9.2.1) defines `GET`, `HEAD`, `OPTIONS` and `TRACE` as
the safe methods, and every other method is treated as a mutation. The two problem types are how a client tells the
rejection apart without parsing prose: RFC 9457's `type` field is the primary identifier of a Problem Details response
([§3.1.1](https://www.rfc-editor.org/rfc/rfc9457#section-3.1.1)), and a client that cannot distinguish "no token was
ever sent" from "the token sent does not match" cannot decide whether a fresh token alone will fix the request. See the
[OWASP CSRF cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).

## Acceptance criteria

- A session-authenticated `POST` with no `X-CSRF-Token` header answers `428`; the same request with a wrong token
  answers `403`; the same request with the session's own token passes the check. Checked by
  `local_login_session_is_csrf_enforced` in `backend/src/routes/auth.rs`.
- The `428` response's problem type is `csrf-missing` and the `403` response's problem type is `csrf-mismatch`. Checked
  by `csrf_missing_returns_428_problem` and `csrf_mismatch_returns_403_problem` in `backend/src/error/mod.rs`.
- A `GET` request from a session-authenticated caller with no `X-CSRF-Token` header is not refused on CSRF grounds. No
  automated check pins this in isolation: every session-authenticated `GET` in the backend suite runs without the
  header, and `HEAD`, `OPTIONS` and `TRACE` share the same exemption in `csrf_required`'s method match with no test of
  their own. It is verified by inspection of that match.
