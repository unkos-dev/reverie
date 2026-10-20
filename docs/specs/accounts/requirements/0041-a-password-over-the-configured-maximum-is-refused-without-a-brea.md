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

Reverie MUST refuse local-password inputs longer than 256 Unicode code points before password verification, strength
scoring, breach checks or password hashing, and MUST NOT truncate passwords. New passwords MUST also respect the
configured maximum, which MUST be between 64 and 256 code points inclusive.

## Rationale

Both strength scoring and a breach-check request carry a cost that grows with the length of the candidate password, and
at least one of the paths that applies this obligation is reachable without authentication, so an unbounded candidate is
a denial-of-service vector. The
[OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
calls for bounding the cost of attacker-supplied credential input, and
[NIST SP 800-63B-4 §3.1.1.2](https://pages.nist.gov/800-63-4/sp800-63b.html#passwordver) says a verifier SHOULD permit a
maximum password length of at least 64 characters. The 256-code-point ceiling is a product decision that bounds
processing while allowing long passphrases; it is not a standards-mandated value. See
[REV-ADR-0034](../../../adr/0034-password-policy-zxcvbn-floor-plus-a-fail-open-hibp-check.md) for the decision to check
the length cap first, ahead of any other policy work.

## Acceptance criteria

- A candidate password longer than the configured maximum is refused as too long. Checked by
  `enforce_rejects_over_max_length_before_any_other_work` in `backend/src/auth/password_policy.rs`, which asserts the
  returned variant.
- A 256-code-point password is accepted by the size check, including supplementary Unicode characters; 257 code points
  are refused. A deployment maximum of 64 accepts 64 and refuses 65. Checked by
  `password_ceiling_counts_unicode_and_keeps_deployment_bounds`.
- Oversized candidates cause no breach-check request, even if a caller constructs a policy with a larger maximum.
  Checked against a recording server by `password_ceiling_precedes_strength_and_breach_processing`.
- Sign-in and own-password change reject oversized input while the only hashing permit is held; valid 256-code-point
  input is verified in full. Checked by `password_ceiling_login_and_own_change_preserve_full_unicode_input`.
- Configuration accepts maxima of 64 and 256 and refuses 63, 257 and 300. Checked by
  `password_policy_default_and_configurable_floor`.
