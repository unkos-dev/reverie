---
type: REQ
profile-version: 1
id: "REV-REQ-0022"
title: "The browser client sends its CSRF token on every mutating request and never an empty one"
governed-by:
  - "REV-ADR-0011"
---

# The browser client sends its CSRF token on every mutating request and never an empty one

## Statement

WHEN the browser client issues a `POST`, `PUT`, `PATCH` or `DELETE` request, it MUST send the `X-CSRF-Token` header
carrying the token it last obtained from `GET /auth/me`, fetching one first if it holds none, and MUST omit the header
entirely when no token was obtained.

## Rationale

The OWASP custom-request-header pattern relies on the browser client attaching the token on every state-changing
request; a request the client sends with no header at all is what lets the server distinguish "the client never obtained
a token" from "the client sent one that does not match" and answer each with a different status. The token travels only
in a `GET /auth/me` JSON response body, never in a URL, so it cannot leak through browser history, referrer headers or
server access logs. See the
[OWASP CSRF cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).

## Acceptance criteria

- A `POST`, `PUT`, `PATCH` or `DELETE` request sent while the client holds a token carries that token in the
  `X-CSRF-Token` header. Checked for each method by `"%s injects X-CSRF-Token when token is cached"` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A mutating request issued while the client holds no token is preceded by exactly one `GET /auth/me`, and is sent
  without the header when that response supplies no token. Checked by
  `"POST with no cached token hydrates once; still omits the header when none is issued"` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A `GET /auth/me` response whose `csrf_token` is an empty string supplies no token, so a mutating request whose token
  acquisition yields only that response is sent without the header. Checked by
  `"an empty refreshed token drops the stale header, and the retry's rejection reaches the caller"` in
  `frontend/tests/browser/api/fetch.test.tsx`.
