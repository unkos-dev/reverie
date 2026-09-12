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

- A `GET` request never carries the `X-CSRF-Token` header even when a token is cached. Checked by
  `"GET does NOT inject X-CSRF-Token even when token is cached"` in `frontend/src/api/fetch.test.ts`.
- A `POST` request carries the header when a token is cached. Checked by
  `"POST injects X-CSRF-Token when token is cached"` in `frontend/src/api/fetch.test.ts`.
- A `POST` issued with no cached token triggers exactly one hydration fetch before the request is sent, and the header
  is still omitted if hydration yields no token. Checked by
  `"POST with no cached token hydrates once; still omits the header when none is issued"` in
  `frontend/src/api/fetch.test.ts`.
- A token consisting of an empty string is treated as no token: the cache clears rather than holding the empty value.
  Checked by `"clears cache when csrf_token is empty string"` in `frontend/src/api/csrf.test.ts`.
