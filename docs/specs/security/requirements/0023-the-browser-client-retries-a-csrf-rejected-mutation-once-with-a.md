---
type: REQ
profile-version: 1
id: "REV-REQ-0023"
title: "The browser client retries a CSRF-rejected mutation once with a refreshed token"
governed-by:
  - "REV-ADR-0011"
---

# The browser client retries a CSRF-rejected mutation once with a refreshed token

## Statement

WHEN a mutating request is answered `403` or `428` with the `csrf-mismatch` or `csrf-missing` problem type, the browser
client MUST fetch a fresh token from `GET /auth/me`, resend the request exactly once, and surface the retried response's
outcome. It MUST NOT retry a `403` or `428` of any other problem type and MUST NOT retry a second time.

## Rationale

[RFC 9110 §15.5.4](https://www.rfc-editor.org/rfc/rfc9110#section-15.5.4) says a client SHOULD NOT automatically repeat
a `403` request with the same credentials and MAY do so with new ones; a refreshed CSRF token is exactly such new
credentials, and the single retry follows that guidance without looping. The `type` field is what lets the client tell a
CSRF rejection apart from any other `403` or `428` without guessing from the status alone
([RFC 9457 §3.1.1](https://www.rfc-editor.org/rfc/rfc9457#section-3.1.1)). Two browser tabs sharing one session cookie
is the case that makes a cached token go stale without the client doing anything wrong: a second tab's re-authentication
mints a new token server-side while the first tab's cache still holds the old one, and only a refresh-and-retry recovers
that tab without a full page reload.

## Acceptance criteria

- A mutating request answered `403` or `428` with problem type `csrf-mismatch` or `csrf-missing` is followed by one
  `GET /auth/me` and one resend carrying the token that response supplied. Checked for each status and type by
  `"a %i %s rejection refreshes once and retries with the new token"` in `frontend/tests/browser/api/fetch.test.tsx`.
- A `403` or `428` whose problem type is neither `csrf-mismatch` nor `csrf-missing` is not retried, and the caller
  receives that response as an error. Checked for `428` by `"a non-CSRF 428 throws without a retry"`, and for `403` by
  `"403 with a NON-csrf-mismatch problem type does not retry"`, in `frontend/tests/browser/api/fetch.test.tsx`.
- The caller receives the resent request's outcome, including a successful response with no content. Checked for a `204`
  by `"204 on csrf-mismatch retry path returns undefined (does not call .json())"` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A CSRF rejection of the resent request is not retried: the mutation is sent at most twice, and the caller receives the
  second rejection as an error. Checked by
  `"a CSRF rejection of the retried request reaches the caller without another retry"` in
  `frontend/tests/browser/api/fetch.test.tsx`.

## More information

When the retry itself fails, the caller sees the retried response's own status and body, never the original rejection's:
this Requirement decides which request is the one that gets surfaced, and the surfacing itself is
[REV-REQ-0016](../../api/requirements/0016-the-browser-client-surfaces-every-non-2xx-response-as-an-error-w.md)'s
obligation.
