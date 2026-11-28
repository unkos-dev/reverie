---
type: REQ
profile-version: 1
id: "REV-REQ-0101"
title: "A session never authenticates a request 30 days after its sign-in"
---

# A session never authenticates a request 30 days after its sign-in

## Statement

WHEN 30 days or more have passed since the sign-in that established a session, that session MUST be refused on the first
request that uses it for authentication and destroyed server-side, however recently it was used, so the same session
cookie never authenticates again and a fresh sign-in is required.

## Rationale

The 24-hour inactivity window renews on every request, so without an overall limit a session in regular use never
expires, and a leaked session id keeps authenticating for as long as someone keeps using it.
[NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html) section 2.1.3 requires a definite overall timeout
that SHOULD be no more than 30 days at AAL1, the level of single-factor sign-in, and section 5.2 requires the session to
be terminated when that timeout expires. The
[OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html#absolute-timeout)
calls for an absolute timeout regardless of session activity, and
[OWASP ASVS 5.0](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x16-V7-Session-Management.md) requirement 7.3.2
requires an absolute maximum session lifetime.

## Acceptance criteria

- A session whose recorded sign-in time is 30 days or more in the past is refused on the first request that uses it for
  authentication, even though it was used within the previous 24 hours. The refused request receives the same status,
  problem type and `WWW-Authenticate` header as a request that carries no session, and the session's server-side row is
  destroyed. Pinned by `session_past_absolute_lifetime_is_refused_and_destroyed` in `backend/src/routes/auth.rs`.
- Sign-in records the time it attaches an identity to the session, and a later request on that session leaves the
  recorded time unchanged. Pinned by the same test.
- A session that carries an identity but no recorded sign-in time is refused and destroyed in the same way. Pinned by
  the same test.
