---
type: ADR
profile-version: 1
id: "REV-ADR-0034"
title: "Password policy: zxcvbn floor plus a fail-open HIBP check"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-30"
decision-makers:
  - "John Unkovich"
---

# Password policy: zxcvbn floor plus a fail-open HIBP check

## Context and problem statement

Reverie accepts local passwords on five paths: self-registration, admin account creation, admin-driven reset, a user
changing their own password, and PIN-based recovery. Without a strength gate, every one of these accepts trivially
weak or already-breached passwords, and a self-hosted instance with no security team is exactly where a weak family
password does the most damage.

A password policy has to satisfy two forces at once. It must be strong by current guidance (NIST SP 800-63B and
OWASP ASVS V2.1, which call for screening against breach corpora and blocklists, forbid composition rules, and
require allowing long passwords). And it must not turn a self-hosted instance into something that cannot set a
password when the public internet is unreachable: an air-gapped deployment, or one running while Have I Been Pwned
is down, must still be able to create accounts and rotate credentials.

How should Reverie screen passwords so that it rejects weak and breached credentials by current standards, without
making an external service a hard dependency of account creation?

## Decision drivers

- NIST SP 800-63B / OWASP ASVS V2.1: screen against known-breached passwords and context-specific words, no
  composition rules, allow long passphrases.
- Self-hosted posture: no third-party service may be a hard dependency of setting a password; offline and
  degraded-network instances must keep working.
- One gate, every caller: registration, admin create, admin reset, self-service change, and PIN recovery must apply
  identical rules, with no path able to skip it.
- Denial-of-service safety: strength scoring and hashing cost grows with input length, and one of the callers is
  reached unauthenticated.

## Considered options

- Length-only minimum
- Composition rules
- zxcvbn floor plus HIBP, fail-open
- zxcvbn floor plus HIBP, fail-closed

## Decision outcome

Chosen option: **zxcvbn floor plus HIBP, fail-open**, behind a single `enforce` entry point, because it screens
strength and breach exposure per current standards while keeping the breach check advisory rather than load
bearing, so an offline or degraded instance still functions.

Every credential-setting path calls one function that applies, in order: a length floor and a maximum cap (the cap
is a denial-of-service guard, checked before any scoring or hashing, not a composition rule); a zxcvbn strength
score (0..=4) with the account's own email and display name fed in as context words; and an HIBP Pwned Passwords
range query using k-anonymity, so only a 5-character SHA-1 prefix ever leaves the instance. The breach check is
fail-open: any network, timeout, or non-success response from HIBP is treated as "not found" so the password is
allowed on strength alone. Two dependencies are added: `zxcvbn` for scoring and `sha1` for the k-anonymity prefix.

### Consequences

- Positive: weak and known-breached passwords are rejected on every path that sets a credential, which matches
  current NIST/OWASP guidance.
- Positive: the instance keeps working with no internet and during an HIBP outage: account creation and password
  rotation never hard-fail on an external service.
- Positive: k-anonymity means a full password or its full hash never leaves the instance, only a 5-character prefix.
- Negative: fail-open means that while HIBP is unreachable a breached-but-strong password can be accepted; the
  breach screen is best-effort, not a guarantee. Each fail-open event is logged so an operator can see when
  screening was degraded.
- Negative: two dependencies (`zxcvbn`, `sha1`) join the tree.

## Pros and cons of the options

### Length-only minimum

- Positive: trivial, no dependencies.
- Negative: accepts breached and predictable passwords that meet the length, which is the main real-world
  compromise vector.

### Composition rules

- Positive: familiar to users from legacy systems.
- Negative: NIST SP 800-63B explicitly advises against composition rules: they push users toward predictable
  patterns and measurably worsen both security and usability.

### zxcvbn floor plus HIBP, fail-open

- Positive: screens strength and breach exposure per current standards while staying functional offline.
- Negative: breach screening is best-effort during an HIBP outage.

### zxcvbn floor plus HIBP, fail-closed

- Positive: breach screening would be a hard guarantee whenever it ran.
- Negative: an HIBP outage or an air-gapped instance could no longer create accounts or rotate passwords, which
  breaks the self-hosted posture for a best-effort signal.

## More information

Builds on the local-password mode established in
[Unified identity with pluggable authentication providers](./0029-unified-identity-with-pluggable-authentication-providers.md).

Revisit if HIBP changes its k-anonymity range API, or if a vendored offline breach corpus becomes viable, which
would let the breach screen run without any external call and remove the fail-open trade-off.
