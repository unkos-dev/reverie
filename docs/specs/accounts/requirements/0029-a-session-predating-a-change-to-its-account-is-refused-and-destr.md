---
type: REQ
profile-version: 1
id: "REV-REQ-0029"
title: "A session predating a change to its account is refused and destroyed"
governed-by:
  - "REV-ADR-0028"
---

# A session predating a change to its account is refused and destroyed

## Statement

WHEN an account's role, child status, or password is changed, or the account is disabled, every session established
before that change MUST be refused on its next request and destroyed server-side, so the same session cookie never
authenticates again and a fresh sign-in is required to obtain a session that reflects the change.

## Rationale

A session that survives a privilege, credential, or status change on its own account is a stale grant: the
[OWASP Session Management cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
calls for a session to be renewed or invalidated whenever the privilege level it carries changes. Every writer of this
change is an administrative or self-service recovery action (a role or child-status change, an account disable, an
administrator's password reset, a self-service password change, or a PIN-based recovery reset), so a session that
outlives one of them would let a caller keep acting under a role, scope, or credential the account no longer has.
[REV-ADR-0028](../../../adr/0028-api-authorization-orthogonal-scope-role-and-ownership-axes.md) records the
authorisation model this obligation keeps honest across a change to the underlying account.

## Acceptance criteria

- A session opened before its account's session version changes is refused on its next request and its server-side row
  is destroyed. Pinned by `session_version_bump_forces_logout` in `backend/src/routes/auth.rs`, which bumps the version
  directly.
- A session opened before a PIN-based password reset is refused afterwards. Pinned by
  `reset_password_invalidates_existing_sessions` in `backend/src/routes/auth.rs`.
- A role change, a child-status change, an administrator's password reset and a self-service password change each bump
  the session version in the same transaction as the change. Verified by inspection of `update_role`,
  `update_child_status`, `admin_reset_password` and `change_own_password` in `backend/src/routes/users/mod.rs`.
- A session established before an account is disabled remains refused once the account is re-enabled; re-enabling does
  not restore the pre-disable session's validity. Verified by inspection of `disable_account` and `enable_account` in
  `backend/src/models/user.rs` and the version comparison they feed in `backend/src/auth/middleware.rs`.
