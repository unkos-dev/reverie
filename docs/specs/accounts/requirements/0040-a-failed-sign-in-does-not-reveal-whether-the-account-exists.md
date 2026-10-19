---
type: REQ
profile-version: 1
id: "REV-REQ-0040"
title: "A failed sign-in does not reveal whether the account exists"
governed-by:
  - "REV-ADR-0029"
  - "REV-ADR-0059"
---

# A failed sign-in does not reveal whether the account exists

## Statement

WHEN a local password sign-in fails because the email has no account, the password is wrong, the account is disabled, or
the account has no local credential, the system MUST return the same status and response body for equivalent source and
account admission state. Admitted failures MUST perform equivalent real or dummy password verification. Throttled
requests MUST perform neither real nor dummy verification, including when the submitted password is correct.

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

- With equivalent admitted state, wrong-password, unknown-email, disabled-account and no-local-credential requests
  return byte-identical generic `422` responses. Each reaches real or dummy verification exactly once.
- With equivalent blocked state, all four cases and a correct password return byte-identical generic `429` responses
  with positive integer `Retry-After` and no verification or authenticated session.
- Rejected requests preserve history and the block deadline. Their retry delay decreases as the deadline approaches.
- `password_admission_route_parity_blocked_correct_and_no_verification` and
  `auth_allowance_account_retry_decreases_without_extending_block` in `backend/src/routes/auth.rs` check these responses
  and verifier invocation counts. `dummy_path_runs_a_verify_and_never_matches` in `backend/src/auth/password.rs` checks
  the dummy verification path.
- This obligation does not promise equal total request latency: a known account requires a credential lookup that an
  unknown email does not, and the first dummy use also initialises its hash.
