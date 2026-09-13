---
type: REQ
profile-version: 1
id: "REV-REQ-0040"
title: "A failed sign-in does not reveal whether the account exists"
governed-by:
  - "REV-ADR-0029"
---

# A failed sign-in does not reveal whether the account exists

## Statement

WHEN a local sign-in attempt fails, whether because the submitted email matches no account, the password does not match
the account's stored credential, or the account is disabled, the system MUST respond with the same status code and the
same response body in every case, and MUST perform equivalent password-verification work regardless of which case
applies.

## Rationale

A response, or a response time, that differs between "no such account" and "wrong password" lets an attacker enumerate
valid accounts before ever guessing a credential, and a further distinct outcome for a disabled account extends that to
account state. See the
[OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)'s
guidance on generic failure messages and [CWE-204](https://cwe.mitre.org/data/definitions/204.html) (observable response
discrepancy). [REV-ADR-0029](../../../adr/0029-unified-identity-with-pluggable-authentication-providers.md) establishes
local password sign-in as a first-class, co-equal authentication mode, so it carries the same non-enumeration
expectation as the rest of Reverie's identity surface.

## Acceptance criteria

- An unknown email and a wrong password for an existing account produce byte-identical response status and body. Checked
  by `local_login_unknown_email_matches_wrong_password` in `backend/src/routes/auth.rs`.
- A wrong password for an existing, enabled account returns a generic `422`. Checked by
  `local_login_wrong_password_is_generic_422` in `backend/src/routes/auth.rs`.
- A disabled account presented with its correct password returns the same generic `422` and establishes no session.
  Checked by `local_login_disabled_account_is_generic_422` in `backend/src/routes/auth.rs`, which asserts the status and
  the absence of a session; that the response body is identical to the other two failure cases is verified by inspection
  of the shared failed-attempt code path all three cases fall into.
- An attempt whose email resolves to no stored credential still performs a full password verification against a
  well-formed dummy hash that never matches. Checked by `dummy_path_runs_a_verify_and_never_matches` in
  `backend/src/auth/password.rs`; that `local_login` calls that path on every attempt without a stored credential is
  verified by inspection of the handler in `backend/src/routes/auth.rs`.
- This obligation binds the response's status and body. It does not bind the account-lookup step that precedes password
  verification: an email that resolves to an account issues one further database read (its stored credential) that an
  unknown email does not, and no acceptance criterion here measures wall-clock timing.
