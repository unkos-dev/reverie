---
type: REQ
profile-version: 1
id: "REV-REQ-0041"
title: "A password over the configured maximum is refused without a breach-check request"
governed-by:
  - "REV-ADR-0034"
---

# A password over the configured maximum is refused without a breach-check request

## Statement

WHEN a candidate password's length exceeds the configured maximum, the system MUST refuse it before performing any
password-strength scoring, and MUST NOT send a breach-check request for it.

## Rationale

Both strength scoring and a breach-check request carry a cost that grows with the length of the candidate password, and
at least one of the paths that applies this obligation is reachable without authentication, so an unbounded candidate is
a denial-of-service vector. The
[OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
calls for bounding the cost of attacker-supplied credential input, and
[NIST SP 800-63B-4 §3.1.1.2](https://pages.nist.gov/800-63-4/sp800-63b.html#passwordver) says a verifier SHOULD permit a
maximum password length of at least 64 characters, so the configured maximum can be raised but never set below that
floor. See [REV-ADR-0034](../../../adr/0034-password-policy-zxcvbn-floor-plus-a-fail-open-hibp-check.md) for the
decision to check the length cap first, ahead of any other policy work.

## Acceptance criteria

- A candidate password longer than the configured maximum is refused as too long. Checked by
  `enforce_rejects_over_max_length_before_any_other_work` in `backend/src/auth/password_policy.rs`, which asserts the
  returned variant.
- No breach-check request is sent for a candidate refused this way. The test above points the breach-check URL at an
  address that cannot be reached, so it cannot itself distinguish "no request sent" from "a request sent and failed
  open"; this criterion is verified by inspection of the check order in the enforcement function, which returns before
  the breach check is reached.
