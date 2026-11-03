---
type: REQ
profile-version: 1
id: "REV-REQ-0074"
title: "Every path that sets a password refuses one the password policy rejects"
governed-by:
  - "REV-ADR-0034"
---

# Every path that sets a password refuses one the password policy rejects

## Statement

WHEN Reverie is asked to set a local-account password, through first-run setup (`POST /auth/setup`), self-registration
(`POST /auth/register`), credential-link redemption (`POST /auth/credential-links/redemptions`), an account holder
changing their own password (`POST /api/v1/account/password`) or the environment seed of the first administrator, it
MUST refuse a candidate that is shorter than `REVERIE_PASSWORD_MIN_LENGTH` Unicode code points, that scores below
`REVERIE_PASSWORD_MIN_ZXCVBN_SCORE`, or that the breach check finds in the breach corpus while
`REVERIE_PASSWORD_BREACH_CHECK_ENABLED` is `true`, and MUST NOT store a credential derived from a refused candidate.

## Rationale

A password guessed from a list of common or breached values lets an attacker sign in as the account holder, so a weak
password chosen by its own owner still weakens authentication for everyone that account can reach.
[NIST SP 800-63B-4 §3.1.1.2](https://pages.nist.gov/800-63-4/sp800-63b.html#passwordver) requires a comparison against a
blocklist of common, expected or compromised passwords "when processing a request to establish or change a password",
and describes that blocklist as a defence against brute-force attacks;
[OWASP ASVS 5.0](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x15-V6-Authentication.md) requirements 6.2.4 and
6.2.12 ask for the same check at both registration and password change. The policy is only as strong as its weakest
entry point: one path that skips it is enough for a weak password to reach the database.

## Acceptance criteria

- The shared policy refuses a candidate shorter than the minimum length, a candidate below the minimum zxcvbn score and
  a candidate the breach check reports as breached. Checked by `enforce_rejects_too_short`,
  `enforce_rejects_weak_password_with_feedback` and `enforce_rejects_a_breached_password` in
  `backend/src/auth/password_policy.rs`.
- First-run setup answers `422` for a refused candidate and creates no administrator. Checked by
  `setup_refuses_a_password_the_policy_rejects` in `backend/src/routes/auth.rs`.
- Self-registration answers `422` for a refused candidate and creates no account. Checked by
  `register_refuses_a_password_the_policy_rejects` in `backend/src/routes/auth.rs`.
- Credential-link redemption answers `422` for a refused candidate, leaves the account without a password and leaves the
  link redeemable. Checked by `credential_link_policy_rejection_and_concurrent_redemption` in
  `backend/src/routes/users/tests.rs`.
- An own-password change answers `422` for a refused candidate and the existing password still signs in. Checked by
  `change_own_password_weak_new_returns_422` in `backend/src/routes/users/tests.rs`.
- The environment seed fails with a refused candidate and creates no administrator. Checked by
  `bootstrap_seed_refuses_a_password_the_policy_rejects` in `backend/src/lib.rs`.
