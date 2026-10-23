---
type: ADR
profile-version: 1
id: "REV-ADR-0063"
title: "Optional SMTP delivery through an encrypted durable mail queue"
status: accepted
recorded-on: "2026-10-21"
decided-on: "2026-10-23"
decision-makers:
  - "John Unkovich"
---

# Optional SMTP delivery through an encrypted durable mail queue

## Context and problem statement

Account holders need password reset by email, administrators want to email credential links rather than only copy them,
and password or address changes should reach the account holder. Many self-hosted instances run without a mail service,
and administrator and host recovery already work without one. How should Reverie send authentication mail without making
mail a prerequisite for recovery, and without storing redeemable links in clear text?

## Decision drivers

- Mail stays optional; administrator links and host recovery work when it is absent, misconfigured or down.
- Operators use any standard SMTP relay, including a hosted provider, without a provider SDK.
- A committed password replacement does not lose its notification when the process restarts.
- A database disclosure reveals no redeemable token and no message body.
- Public reset requests reveal nothing about whether an address has an account.
- The runtime stays single-process with the existing Postgres-backed job model and adds no C toolchain.

## Considered options

- Encrypted durable Postgres mail queue with generic SMTP.
- Direct SMTP submission from the request, with no queue.
- Durable Postgres mail queue storing message content in clear text.
- Provider HTTPS delivery API.

## Decision outcome

Chosen option: **Encrypted durable Postgres mail queue with generic SMTP**, because it keeps mail optional and
restart-safe while a database disclosure reveals no queued message contents or links, and it works with any SMTP relay.

Postgres keeps mail durable without adding a broker and lets notifications commit atomically with the account changes
that cause them. Encrypting queued recipients and messages limits what a database disclosure reveals, at the cost of an
externally supplied key that operators must manage separately.

Generic SMTP avoids a provider-specific integration. `lettre` with `rustls` supplies the transport, and RustCrypto
`aes-gcm` supplies authenticated AES-256-GCM encryption without adding a C toolchain. These libraries keep protocol and
cryptographic implementations outside Reverie while fitting its Rust runtime.

Direct submission is simpler but loses mail on restart and ties request latency to the relay. A clear-text queue is
restart-safe but turns a database backup into a set of usable reset links. A provider API narrows support to one
provider and adds a second delivery path if SMTP is also wanted.

### Consequences

- Positive: mail outages and missing configuration leave administrator links and host recovery fully usable.
- Positive: pending mail survives restart and stops on expiry, revocation or permanent refusal.
- Positive: a database disclosure without the queue key reveals no queued token or message body.
- Neutral: the encryption covers mail payloads only; account email addresses stay readable in the account tables.
- Negative: operators must back up the queue key separately; losing it loses undelivered mail and needs a documented
  manual recovery.
- Negative: AES-GCM with a random nonce per message bounds one key to about 2^32 encryption operations. Reverie keeps no
  usage counter, so replacing the key is the operator's responsibility.
- Negative: a lost SMTP acknowledgement can produce a duplicate message; retries reuse one Message-ID but promise no
  deduplication.
- Negative: a public reset request can be lost before it reaches the queue, because the response never waits for account
  lookup. The person requests another email.
- Negative: acceptance by the relay is the strongest outcome Reverie can report; inbox delivery is not observable.
- Neutral: one worker and one queue fit the expected volume; there is no multi-instance coordination.

## Pros and cons of the options

### Encrypted durable Postgres mail queue with generic SMTP

- Positive: reuses the existing durable-job and supervision model.
- Positive: `lettre` provides typed messages, address parsing that rejects header injection, TLS selection and test
  transports.
- Positive: RustCrypto `aes-gcm` provides standard AES-256-GCM in Rust without a C build requirement.
- Negative: two new direct dependencies and an external key to manage.

### Direct SMTP submission from the request, with no queue

- Positive: no stored mail state and no key.
- Negative: a restart or relay outage loses notifications for changes that already committed.
- Negative: request time depends on the relay, and timing can reveal whether a reset address is eligible.

### Durable Postgres mail queue storing message content in clear text

- Positive: restart-safe without key management.
- Negative: database backups and replicas hold live reset and verification links.

### Provider HTTPS delivery API

- Positive: provider-specific features such as idempotency keys.
- Negative: ties delivery to one provider; operators with another relay need SMTP anyway.

## More information

`mail-send` is an equally viable SMTP library; `lettre` was preferred for its API and test surface, not a measured
security or performance difference. `aws-lc-rs` and `ring` are already transitive dependencies, but `aws-lc-rs` needs a
C toolchain and `ring` leaves nonce management to the caller. `chacha20poly1305` offers a larger nonce, but nothing here
requires it over standard AES-GCM. Per-message key derivation and online key rotation were excluded.

[RFC 8314, section 3.3](https://www.rfc-editor.org/rfc/rfc8314.html#section-3.3) prefers implicit TLS for message
submission and treats required STARTTLS as comparable when TLS must succeed before submission.

This decision relies on the
[durable job queue](0018-durable-job-queue-postgres-backed-skip-locked-crash-only.md),
[externally supplied authentication material](0053-typed-credentials-and-externally-supplied-authentication-material.md)
and [non-serialising credential redaction](0054-non-serialising-credential-model-redaction.md) decisions, and extends
[account-bound credential links](0061-administrators-issue-account-bound-credential-links.md) to email delivery.
Reconsider it if Reverie runs more than one instance against one database or adds an administrator mail settings screen.
