---
status: accepted
date: 2026-06-30
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Password strength policy: zxcvbn floor plus a fail-open HIBP breach check

## Context and Problem Statement

Reverie accepts local passwords on five paths: self-registration, admin account
creation, admin-driven reset, a user changing their own password, and PIN-based
recovery. Without a strength gate, every one of these accepts trivially weak or
already-breached passwords, and a self-hosted instance with no security team is
exactly where a weak family password does the most damage.

A password policy has to satisfy two forces at once. It must be strong by current
guidance (NIST SP 800-63B and OWASP ASVS V2.1, which call for screening against
breach corpora and blocklists, forbid composition rules, and require allowing long
passwords). And it must not turn a self-hosted instance into something that cannot
set a password when the public internet is unreachable: an air-gapped deployment,
or one running while Have I Been Pwned is down, must still be able to create
accounts and rotate credentials.

How should Reverie screen passwords so that it rejects weak and breached
credentials by current standards, without making an external service a hard
dependency of account creation?

## Decision Drivers

- NIST SP 800-63B / OWASP ASVS V2.1: screen against known-breached passwords and
  context-specific words, no composition rules, allow long passphrases.
- Self-hosted posture: no third-party service may be a hard dependency of setting
  a password; offline and degraded-network instances must keep working.
- One gate, every caller: registration, admin create, admin reset, self-service
  change, and PIN recovery must apply identical rules, with no path able to skip
  it.
- Denial-of-service safety: strength scoring and hashing cost grows with input
  length, and one of the callers is reached unauthenticated.

## Considered Options

- Length-only minimum (status quo)
- Composition rules (require mixed case, digits, symbols)
- A zxcvbn strength floor plus an HIBP k-anonymity breach check, fail-open
- The same, but fail-closed (reject when HIBP is unreachable)

## Decision Outcome

Chosen option: a zxcvbn strength floor plus an HIBP breach check, fail-open,
behind a single `enforce` entry point.

Every credential-setting path calls one function that applies, in order: a length
floor and a maximum cap (the cap is a denial-of-service guard, checked before any
scoring or hashing, not a composition rule); a zxcvbn strength score (0..=4) with
the account's own email and display name fed in as context words; and an HIBP
Pwned Passwords range query using k-anonymity, so only a 5-character SHA-1 prefix
ever leaves the instance. The breach check is fail-open: any network, timeout, or
non-success response from HIBP is treated as "not found" so the password is
allowed on strength alone. Two dependencies are added: `zxcvbn` for scoring and
`sha1` for the k-anonymity prefix.

This satisfies the standards drivers (breach screening, no composition rules, long
passwords allowed) while keeping the breach screen advisory rather than load
bearing, so an offline or degraded instance still functions.

### Consequences

- Good, because weak and known-breached passwords are rejected on every path that
  sets a credential, matching current NIST/OWASP guidance.
- Good, because the instance keeps working with no internet and during an HIBP
  outage: account creation and password rotation never hard-fail on an external
  service.
- Good, because k-anonymity means a full password or its full hash never leaves
  the instance, only a 5-character prefix.
- Bad, because fail-open means that while HIBP is unreachable a breached-but-strong
  password can be accepted; the breach screen is best-effort, not a guarantee. Each
  fail-open event is logged so an operator can see when screening was degraded.
- Neutral, because two dependencies (`zxcvbn`, `sha1`) join the tree.

### Confirmation

Every credential-setting path routes through the single `enforce` seam in
`backend/src/auth/password_policy.rs`; none reimplements the checks. The length cap
is asserted before any scoring or hashing.

## Pros and Cons of the Options

### Length-only minimum

- Good, because it is trivial and has no dependencies.
- Bad, because it accepts breached and predictable passwords that meet the length,
  which is the main real-world compromise vector.

### Composition rules

- Good, because it is familiar to users from legacy systems.
- Bad, because NIST SP 800-63B explicitly advises against composition rules: they
  push users toward predictable patterns and measurably worsen both security and
  usability.

### zxcvbn floor plus HIBP, fail-open (chosen)

- Good, because it screens strength and breach exposure per current standards
  while staying functional offline.
- Bad, because breach screening is best-effort during an HIBP outage.

### zxcvbn floor plus HIBP, fail-closed

- Good, because breach screening would be a hard guarantee whenever it ran.
- Bad, because an HIBP outage or an air-gapped instance could no longer create
  accounts or rotate passwords, breaking the self-hosted posture for a
  best-effort signal. Rejected for that reason.

## More Information

Builds on the local-password mode established in
[2026-06-23-auth-identity-pluggable-providers.md](2026-06-23-auth-identity-pluggable-providers.md).
Revisit if HIBP changes its k-anonymity range API, or if a vendored offline
breach corpus becomes viable, which would let the breach screen run without any
external call and remove the fail-open tradeoff.
