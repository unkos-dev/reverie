---
type: REQ
profile-version: 1
id: "REV-REQ-0039"
title: "A breach-check outage never blocks setting a password"
governed-by:
  - "REV-ADR-0034"
---

# A breach-check outage never blocks setting a password

## Statement

WHEN a password-breach check cannot complete, for any reason including a network or transport failure, a non-2xx
response from the breach-checking service, an unreadable response body, or a response whose matching entry cannot be
parsed as a count, the system MUST treat the candidate password as not found in the breach corpus and MUST proceed to
evaluate any remaining password-strength checks rather than refusing the request on that basis. A credential-setting
request MUST NOT be refused, and a password MUST NOT be rejected, solely because the breach-checking service is
unreachable or misbehaving.

## Rationale

A self-hosted instance may run with no outbound network at all, or the breach-checking service may itself be unreachable
or degraded; blocking a credential-setting request under those conditions makes such an instance unusable out of the
box. [NIST SP 800-63B §5.1.1.2](https://pages.nist.gov/800-63-3/sp800-63b.html#sec5) asks that a verifier screen
candidate passwords against known-breached corpora, but treats that as a strength control layered on top of the primary
length and complexity requirements, not as a hard dependency. See
[REV-ADR-0034](../../../adr/0034-password-policy-zxcvbn-floor-plus-a-fail-open-hibp-check.md) for the decision to make
the check fail-open rather than fail-closed.

## Acceptance criteria

- A transport failure while contacting the breach-checking service does not prevent an otherwise-acceptable password
  from being accepted. Checked by `check_breached_fails_open_when_unreachable` in `backend/src/auth/password_policy.rs`.
- A non-2xx response from the breach-checking service, including a rate-limited or unavailable response, is treated the
  same as a miss. Checked by `check_breached_fails_open_on_non_2xx` in `backend/src/auth/password_policy.rs`.
- A response whose entry for the candidate's hash suffix carries a count that cannot be parsed is treated the same as a
  miss, rather than as an error that blocks the request. Checked by
  `check_breached_malformed_count_for_matching_suffix_fails_open` in `backend/src/auth/password_policy.rs`.
- A strong candidate password is still accepted when the breach check itself has failed open. Checked by
  `enforce_allows_a_strong_password_when_breach_check_fails_open` in `backend/src/auth/password_policy.rs`.
