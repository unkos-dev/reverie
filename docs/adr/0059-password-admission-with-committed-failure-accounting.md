---
type: ADR
profile-version: 1
id: "REV-ADR-0059"
title: "Password admission with committed failure accounting"
status: "proposed"
recorded-on: "2026-10-19"
decision-makers:
  - "John Unkovich"
---

# Password admission with committed failure accounting

## Context and problem statement

Account throttling protects against distributed password guessing only when admission precedes credential verification.
Concurrent requests and separate application instances need one authority for admission, completed failures and reset.
An in-memory account limiter cannot coordinate an operator's separate unlock process.

This record refines the password-throttling rationale in
[REV-ADR-0029](0029-unified-identity-with-pluggable-authentication-providers.md) and the source allowance described in
[REV-ADR-0058](0058-one-shot-sign-in-attempts-with-a-single-winner.md). Identity, sessions and single-winner arbitration
keep their existing owners.

## Decision drivers

- Refuse verification during an account block, including a correct password.
- Count completed credential failures and commit accounting before exposing their result.
- Coordinate application instances, operator unlock and cleanup through ordinary database transactions.
- Preserve independent recovery and OIDC source allowance during password traffic.
- Make cancellation and resource limitations explicit.

## Considered options

- Governor source limits with account rejection after verification.
- Governor source limits with PostgreSQL account admission held through verification and commit.
- Durable reservations with expiry and asynchronous completion accounting.
- Trusted-device cookies with a privileged password allowance.
- Permanent password disabling after excessive failures.
- A global request bucket shared across authentication methods.

## Decision outcome

Chosen option: **governor source limits with PostgreSQL account admission held through verification and commit**,
because row locks give every account-state writer the same ordering without a reservation lifecycle.

Retain at most ten completed-failure timestamps per lower-case email key in a rolling 15-minute window. Ten committed
failures start a 15-minute block. The tenth failure returns the generic credential failure; subsequent requests reject
before lookup or hashing. Rejections preserve the block deadline. A winning enabled-account login or CLI unlock clears
history and the block under the same row lock. Expiry applies at equality.

Cancellation before commit rolls back. After commit, the result remains charged even if its response is lost. An unknown
commit outcome is never replayed automatically. Account-result commit precedes session establishment, so a later
session-storage failure can leave history cleared without a delivered session.

Governor owns separate source allowances for password/setup/registration, recovery, and OIDC start. Native governor
retention and bounded, locked-row database cleanup run through the existing hourly sweep. Reclamation is eventual and
does not bound storage under an unbounded arrival rate. New transient throttle state starts empty; existing migration
files and checksum checks remain intact.

Reservations add expiry and late-completion ownership that this policy does not need. Trusted-device cookies add a
credential lifecycle and a privileged class. Permanent disabling adds recovery obligations. A shared global bucket lets
password traffic consume recovery or OIDC allowance. These options are rejected for this admission policy.

The ten-failure window and temporary block are product choices. They do not implement
[NIST SP 800-63B-4's rate-limiting rule](https://pages.nist.gov/800-63-4/sp800-63b.html#rate-limiting), which includes
consecutive-failure disabling and binding disabled credentials to the account again. This decision makes no
standards-compliance claim.

### Consequences

- Positive: separate application instances cannot verify through an already committed account block.
- Positive: cancellation, success, unlock and reclamation share database ordering without stale completion writers.
- Positive: password source traffic does not consume recovery or OIDC source allowance.
- Negative: a sustained targeted attack can repeatedly obstruct password-only users, including users with the correct
  password. Existing sessions remain valid.
- Negative: serialisation spans synchronous hashing and occupies one database connection; same-account callers can wait
  for that lock. Aggregate hashing and waiting limits require measurement and remain separate work.
- Negative: source limits are process-local and reset on restart. They do not establish availability during arbitrary
  floods.
