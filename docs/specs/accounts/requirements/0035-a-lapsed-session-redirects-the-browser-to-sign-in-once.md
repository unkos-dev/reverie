---
type: REQ
profile-version: 1
id: "REV-REQ-0035"
title: "A lapsed session redirects the browser to sign-in once"
governed-by:
  - "REV-ADR-0015"
---

# A lapsed session redirects the browser to sign-in once

## Statement

WHEN the browser client, on a page that requires a signed-in reader, learns that its session is no longer authenticated,
because a request answers `401 Unauthorized` or because `GET /auth/me` answers `401` or `403`, the client MUST navigate
the browser to sign-in exactly once for that lapse, however many requests discover it together.

## Rationale

A lapsed session typically fails every request in flight on the page at the same moment, not just one. Without a single
point of coordination, each failing request would independently attempt to redirect the browser, and concurrent
navigations race for the same destination and cannot be relied on to converge cleanly. A page whose only request is the
identity check learns of the lapse from that check alone, and without recovery would leave the reader on a degraded
page. An identity check that fails for any other reason has not established that the session lapsed. See the
[OWASP Session Management cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
on handling session expiration client-side. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- Several requests that fail together for the same lapsed session produce exactly one navigation to sign-in, not one per
  failing request. Checked by `` `is guarded — repeated calls navigate once` `` in
  `frontend/tests/browser/lib/query/client.test.tsx`.
- `GET /auth/me` answering `401` or `403`, with no other request failing, navigates the browser to sign-in once.
- `GET /auth/me` failing with a network error or with a status other than `401` or `403` does not navigate the browser
  to sign-in.
