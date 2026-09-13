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

WHEN the browser client learns its session has lapsed, whether because a request answers `401 Unauthorized` or because
the shared identity query settles without an identity, the client MUST navigate the browser to sign-in exactly once,
however many requests fail together as a result of the same lapse.

## Rationale

A lapsed session typically fails every request in flight on the page at the same moment, not just one. Without a single
point of coordination, each failing request would independently attempt to redirect the browser, and concurrent
navigations race for the same destination and cannot be relied on to converge cleanly. See the
[OWASP Session Management cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
on handling session expiration client-side. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- Several requests that fail together for the same lapsed session produce exactly one navigation to sign-in, not one per
  failing request. Checked by `` `is guarded — repeated calls navigate once` `` in
  `frontend/src/lib/query/client.test.ts`.
- A freshly wired client is able to navigate again on a subsequent, independent lapse; the once-guard from a prior lapse
  does not suppress it permanently. Checked by `` `setUnauthenticatedHandler resets the once-guard` `` in
  `frontend/src/lib/query/client.test.ts`.
