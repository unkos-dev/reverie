---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0029"
title: "Authentication mail delivery"
satisfies:
  - "REV-REQ-0060"
  - "REV-REQ-0061"
governed-by:
  - "REV-ADR-0063"
---

# Authentication mail delivery

Optional SMTP delivery sends credential links, mailbox challenges, password reset links and security notices. Mail jobs
are stored encrypted in Postgres and submitted by one supervised worker. Without mail configuration, Reverie runs
normally and every non-email recovery path stays available.

## Purpose and boundaries

This subject owns mail configuration, the SMTP transport, payload encryption, the `mail_jobs` table, the submission
worker, the `send-test-email` host command and mail-job cleanup. It renders and seals messages that account transactions
hand to it.

[Mailbox verification and email recovery](../../accounts/design/0028-account-mailbox-verification-and-email-recovery.md)
and [credential links](../../accounts/design/0027-credential-links.md) decide when mail is queued, which recipient it
uses and which proof it carries. [Configuration loading](0022-configuration-loading.md) resolves the settings and secret
files. [Application runtime](0024-application-runtime-startup-workers-and-shutdown.md) supervises the worker.

## Structure

- `backend/src/config/mail.rs` parses and validates mail settings.
- `backend/src/services/mail/transport.rs` builds the `lettre` transport and performs one bounded submission, shared by
  the worker and `send-test-email`.
- `backend/src/services/mail/crypto.rs` seals and opens payloads with RustCrypto `aes-gcm`.
- `backend/src/services/mail/queue.rs` enqueues jobs, claims due jobs, records outcomes and runs the worker loop.
- `backend/src/models/mail_job.rs` holds the row model, which never serialises its payload, nonce or digest.

## Interfaces and dependencies

| Setting                            | Meaning                                                             |
| ---------------------------------- | ------------------------------------------------------------------- |
| `REVERIE_SMTP_ENABLED`             | Turns mail on; false by default                                     |
| `REVERIE_SMTP_HOST`                | Relay host name, required when enabled                              |
| `REVERIE_SMTP_PORT`                | Optional; defaults to 465, 587 or 25 by security mode               |
| `REVERIE_SMTP_SECURITY`            | `implicit_tls` (default), `starttls` or `plaintext`                 |
| `REVERIE_SMTP_USERNAME`            | Optional relay login, paired with the password                      |
| `REVERIE_SMTP_PASSWORD` / `_FILE`  | Optional relay password                                             |
| `REVERIE_SMTP_FROM`                | Sender address, required when enabled                               |
| `REVERIE_SMTP_REPLY_TO`            | Support address for Reply-To and notices, required when enabled     |
| `REVERIE_MAIL_QUEUE_KEY` / `_FILE` | 32 random bytes as base64url without padding, required when enabled |
| `REVERIE_PUBLIC_URL`               | HTTP(S) origin used for every emailed link, required when enabled   |

`lettre` uses Tokio and `rustls` with the platform certificate verifier. `starttls` fails if the relay does not offer
it. `plaintext` is used only when selected and still allows relay authentication. Addresses parse into typed mailboxes,
so header injection is refused. Links come only from `REVERIE_PUBLIC_URL`, never from Host or forwarded headers.

Every rendered message writes its dates out in full in UTC as `{weekday} {day} {month} at {hour}:{minute} {am|pm} UTC`,
with full weekday and month names, no leading zero on the day or hour, two-digit minutes, a 12-hour clock with lowercase
"am" or "pm", no year, and the time zone named once. For example: "Saturday 25 October at 2:00 pm UTC".

`reverie send-test-email <address>` loads only the transport, sender and support settings. It makes one submission
bounded to 30 seconds, prints `Accepted by mail server.` and exits zero only on positive SMTP completion, and otherwise
exits nonzero with a safe failure category. It needs no database, queue key or account.

## Data and state

A `mail_jobs` row records the job UUID, account UUID, purpose, optional proof digest and authority kind, original
enqueue time, next attempt time, attempt count, expiry, state, terminal time, safe failure category, nonce and encrypted
payload. Purposes distinguish administrator links, verification, email recovery and each notice. States are `pending`,
`in_progress`, `accepted`, `failed`, `cancelled` and `expired`. Deleting the account deletes its jobs; consuming or
deleting a proof leaves the job row and its accounting fields in place.

The payload holds the recipient and the rendered message, sealed together with a fresh random nonce. The job UUID and
purpose are authenticated data. Payload lifetime depends on purpose:

| Purpose                         | Payload removed                                             |
| ------------------------------- | ----------------------------------------------------------- |
| Verification and email recovery | When the proof is used, replaced, cancelled or expires      |
| Administrator link              | At any final state, or earlier when the link is invalidated |
| Notice                          | At any final state                                          |

Final rows keep their metadata for seven days. The hourly session sweep removes expired payloads and old final rows. It
takes account locks before changing jobs, skips locked accounts and retries on the next sweep.

## Runtime behaviour

The worker orders due notices before other due mail, then by enqueue time and job UUID. It claims one job with
`FOR UPDATE SKIP LOCKED`, records the attempt, and commits before doing anything else. It then checks the job again:

| Purpose | Checks before submission |
| ------- | ------------------------ |
| Administrator link, verification, email recovery | Account enabled, proof live, bound recipient equals current address |
| Notice | Account exists and the job has not expired |

A failed check cancels or expires the job. Otherwise the worker opens the payload and submits, with the whole SMTP
exchange bounded to 30 seconds. No database transaction or lock is held during submission. Each message carries a
Message-ID derived from the job UUID.

A temporary failure is retried 30 seconds, two minutes, five minutes and fifteen minutes after the preceding failure:
five attempts in total. A permanent refusal stops at once. Proof mail expires with its proof; notices expire 24 hours
after enqueue. Retries never change the original enqueue time.

On startup, including a supervised restart, `in_progress` jobs return to `pending` and the interrupted attempt counts.
Shutdown stops new claims and cancels unfinished work within the worker stop budget. When mail is disabled, no job is
queued and no worker runs; stored jobs wait, and on re-enable expired or revoked ones are discarded before submission.

## Failure and recovery

Failures are reported as `connection`, `tls`, `authentication`, `temporary_refusal`, `permanent_refusal`, `timeout` or
`payload_unreadable`. A relay outage never affects HTTP readiness. Invalid mail settings stop startup with the setting
name; missing settings with mail disabled are ignored. The `reset-password` host command never reads mail settings.

An authentication failure when opening a payload is a hard refusal recorded as `payload_unreadable`; there is no
fallback. A lost SMTP acknowledgement can lead to a duplicate. Acceptance by the relay is the last observable outcome.

With the queue key lost, undelivered mail cannot be recovered. The operator cancels the affected jobs and clears their
payloads, invalidates email proofs and supplies a new key; administrator links and host recovery keep working.

## Security and operations

Logs and status carry only job IDs, purposes and failure categories, never relay responses, bodies, recipients, tokens
or credentials. The relay password and queue key are typed secrets with the shared file conflict, newline and redaction
rules, and generated configuration artefacts leave them empty.

The queue key is backed up apart from the database. A key supports about 2^32 encryption operations; Reverie keeps no
counter, and a database restore does not reset usage. Planned replacement disables mail, stops the process, lets proofs
expire, cancels remaining jobs and clears payloads with documented SQL, then restarts with the new key.

Plaintext mode exposes credentials and messages to anyone on the network path. It suits only a connection the operator
trusts. Private certificate authorities are installed in the host or container trust store.
