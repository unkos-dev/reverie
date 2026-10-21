---
type: ADR
profile-version: 1
id: "REV-ADR-0061"
title: "Administrators issue account-bound credential links"
status: proposed
recorded-on: "2026-10-20"
decision-makers:
  - "John Unkovich"
---

# Administrators issue account-bound credential links

## Context and problem statement

Account creation and administrator-initiated password recovery need a way to give the account holder a credential
without disclosing that password to the administrator. The unified identity model permits administrators to create
accounts, including accounts without a local password. This decision refines that account-administration boundary.

## Decision drivers

- The account holder chooses the password; the administrator never enters or sees it.
- A reset applies to one account, expires and succeeds at most once.
- Existing password policy, hashing limits and session invalidation apply to redemption.
- Administrators can deliver links out of band without an email service.

## Considered options

- Account-bound credential links.
- Administrator-chosen passwords delivered out of band.
- Temporary passwords that the account holder must change.

## Decision outcome

Chosen option: **Account-bound credential links**, because they let administrators initiate account creation and
recovery without knowing the resulting password or adding a temporary-password sign-in state.

A browser-authenticated administrator issues or cancels a link. Account creation and its initial link are atomic.
Issuance returns a random 256-bit token once; storage holds its SHA-256 digest, bound to one account for 72 hours.
Reissuing replaces the account's link. The browser constructs the URL from its own origin and carries the token in the
fragment. Only an enabled account with an email can redeem a link. Redemption applies password policy and bounded
hashing, then atomically consumes the unexpired link, writes the password and increments the session version. It does
not sign the person in or reset login accounting. PIN recovery remains independent.

Administrator-chosen passwords keep delivery simple, but disclose the credential to someone other than its holder.
Temporary passwords shorten that exposure only if changed and require a restricted sign-in state until then.

### Consequences

- Positive: account holders choose initial and replacement passwords without sharing them with administrators.
- Positive: expiry, replacement, cancellation and atomic consumption bound a link's lifetime and use.
- Negative: anyone holding a forwarded or leaked link can set that account's password before expiry or cancellation.
- Negative: administrators must copy and deliver each link; closing the result requires another link if it was not kept.
- Negative: recovery notifications are absent until a mail channel is provided. NIST SP 800-63B-4's account-recovery
  notification requirement is unmet.
- Neutral: device tokens remain valid after redemption; PIN recovery and first-run setup retain their own flows.

## More information

This decision refines
[Unified identity with pluggable authentication providers](0029-unified-identity-with-pluggable-authentication-providers.md).
