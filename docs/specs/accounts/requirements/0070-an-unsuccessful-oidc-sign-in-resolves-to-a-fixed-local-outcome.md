---
type: REQ
profile-version: 1
id: "REV-REQ-0070"
title: "An unsuccessful OIDC sign-in resolves to a fixed local outcome"
---

# An unsuccessful OIDC sign-in resolves to a fixed local outcome

## Statement

For the OIDC sign-in conditions in the acceptance table, Reverie MUST redirect the browser with HTTP status 303 to
`/login?outcome=<outcome>`, using exactly the outcome assigned to that condition. The closed vocabulary is `denied`,
`expired`, `unavailable`, `keys`, `account` and `throttled`. A callback when OIDC is not configured remains 404. A
failure of the session middleware's subsequent save may still return an empty 500; inability to produce the fixed
response may also return 500.

## Rationale

A browser needs a local recovery page when an external sign-in cannot finish. Stable outcome names let that page explain
the next action without exposing provider text or distinguishing an email collision from a disabled account.

## Acceptance criteria

| Condition | Outcome |
| --------- | ------- |
| Provider `access_denied` with valid state | `denied` |
| Other provider error with valid state | `unavailable` |
| Missing, mismatched, expired or used state; malformed or incomplete callback; provider error without valid state | `expired` |
| Token endpoint `invalid_grant`; valid attempt that loses arbitration | `expired` |
| Other token endpoint error; transport, server or parse failure; missing ID token; invalid claims or signature | `unavailable` |
| No matching key after the permitted refresh; ambiguous key; failed key fetch | `keys` |
| Discovery pending, runtime unavailable, or callback storage/provisioning/session-completion failure | `unavailable` |
| Email held by another account; disabled account | `account` |
| OIDC start over the per-source limit | `throttled` |
| OIDC start with a won cohort | `expired` |
| OIDC start while discovery is pending | `unavailable` |

- Each row answers with the exact 303 destination and no reflected provider description. Provider error classification
  requires a live, browser-bound state claim; both success and error responses consume it at most once.
- A signed-in browser at start or callback goes 303 to `/` without provider exchange or state validation. The callback's
  unconfigured-OIDC 404 check precedes this short-circuit.
- The login page renders fixed copy for each recognised outcome and requires a click or keyboard activation to start a
  fresh provider attempt. Mounting, refreshing and browser back do not retry an authorization response.
- An unknown, absent, empty or repeated outcome renders ordinary login without displaying the supplied value.
- Callback route tests named `oidc_outcome_`, the existing callback/key-refresh/account-refusal tests in
  `backend/src/routes/auth.rs`, and the per-outcome tests in `frontend/tests/browser/auth-login.test.tsx` exercise the
  condition-to-destination and recovery contracts. Rendered acceptance uses the login screen specification.
