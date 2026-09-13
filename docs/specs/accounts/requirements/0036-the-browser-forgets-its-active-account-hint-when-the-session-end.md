---
type: REQ
profile-version: 1
id: "REV-REQ-0036"
title: "The browser forgets its active-account hint when the session ends"
governed-by:
  - "REV-ADR-0015"
---

# The browser forgets its active-account hint when the session ends

## Statement

The browser client MUST clear its per-device active-account hint whenever the session ends: on a lapse, before
navigating to sign-in, and on an explicit sign-out, whether or not the sign-out request itself succeeded.

## Rationale

The OWASP Session Management cheat sheet calls for clearing client-side state at logout; see the
[cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html). The hint keys
per-user client-side caches on a shared browser; a stale hint left behind after one account's session ends would seed
another account's first-paint presentation from the previous account's cached values. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- A dead session clears the active-account hint before the client's redirect fires. Checked by
  `` `a dead session forgets the browser's active user` `` in `frontend/src/lib/query/client.test.ts`.
- An explicit sign-out clears the active-account hint, whether or not the sign-out request to the server succeeded.
  Checked by `` `sign out forgets the browser's active account` `` in `frontend/src/components/shell/UserMenu.test.tsx`.
