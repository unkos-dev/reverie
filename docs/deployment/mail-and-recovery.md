# Mail and account recovery

Reverie can send authentication mail through an SMTP relay. Mail is optional: administrators can copy credential links,
and an operator can issue a short-lived link from the host even when SMTP is unavailable.

## Configure a relay

Supply these settings to the backend process or container, substituting your own host, addresses and mounted secret
files. The public URL is the browser-facing HTTP(S) origin, without a path, query, fragment or credentials.

```dotenv
REVERIE_SMTP_ENABLED=true
REVERIE_SMTP_HOST=smtp.example.test
REVERIE_SMTP_SECURITY=implicit_tls
REVERIE_SMTP_PORT=465
REVERIE_SMTP_FROM=library@example.test
REVERIE_SMTP_REPLY_TO=support@example.test
REVERIE_SMTP_USERNAME=relay-user
REVERIE_SMTP_PASSWORD_FILE=/run/secrets/smtp-password
REVERIE_MAIL_QUEUE_KEY_FILE=/run/secrets/mail-queue-key
REVERIE_PUBLIC_URL=https://library.example.test
```

The support address is also the Reply-To on authentication mail. Relay authentication is optional; omit both username
and password for an unauthenticated relay. Supply a password directly with `REVERIE_SMTP_PASSWORD` or through its file
alias, never both. The queue key is 32 cryptographically random bytes encoded as base64url without padding (43
characters). Supply it with `REVERIE_MAIL_QUEUE_KEY` or its file alias, never both. Back up the key separately from the
database. Do not put either secret in source control, browser configuration or logs.

Secret files must be readable inside the backend container. The reader removes trailing LF and CRLF terminators but
preserves other characters. Errors identify the setting without disclosing its value or file path.

| Security mode  | Default port | Behaviour                                              |
| -------------- | ------------ | ------------------------------------------------------ |
| `implicit_tls` | 465          | TLS starts with the connection                         |
| `starttls`     | 587          | Submission requires a successful TLS upgrade           |
| `plaintext`    | 25           | No encryption, including optional relay authentication |

TLS uses the platform certificate verifier. Install private certificate authorities in the host or container trust
store; there is no certificate-verification bypass. Explicit plaintext exposes credentials, recipients and messages to
the network path. Choose it only for a trusted connection. Publicly reachable browser deployments should use HTTPS and
`REVERIE_BEHIND_HTTPS=true`.

Invalid enabled mail configuration stops startup. With mail disabled, unused mail settings are ignored. A running relay
outage does not affect HTTP readiness.

## Check submission

Choose a recipient you control and run the host command with the relay settings above:

```sh
reverie-api send-test-email recipient@example.test
```

It sends subject "Reverie test email" and body "This is a test email from Reverie." The command reads transport, sender
and support settings only; it needs no database, queue key, storage or identity-provider configuration. It makes one
attempt, bounded to 30 seconds. Success prints `Accepted by mail server.` and exits zero. Failure exits nonzero with
`The mail server did not confirm the test email: {reason}.` The reason is a safe failure category, never a raw relay
response. This command also checks private-CA setup. Acceptance confirms the relay's response, not inbox delivery.

Automated tests use controlled relays. A real-provider test is a separate operator action with a deliberately chosen
recipient.

## Account recovery

An account holder uses **Email address** in the user menu to request verification. They must open the emailed challenge
while signed in to the same account and explicitly confirm it. Verification lasts until the stored address changes or is
removed. OIDC claims and administrator-issued links do not verify an address; an OIDC-only account can verify its
mailbox but cannot reset a password it does not have.

Email password reset requires local authentication, an enabled account, a verified current address and an existing local
password. The public request always gives the same response for valid input. It can be lost before durable enqueue,
including when background capacity is full or the process stops. The person requests another email.

Recovery proofs last 15 minutes, verification challenges 24 hours. Requested recovery deliveries are at least 60 seconds
apart, at most five per rolling 24 hours per account, and at most three for one proof. Pending deliveries absorb repeat
requests. Automatic retries do not consume another requested delivery. Failed and cancelled jobs still count; deleting a
proof or restarting does not reset the allowance. Verification requests reuse a live challenge and coalesce within 60
seconds.

In **Users**, administrators create or reset an account and copy its once-displayed link. **Send email** explicitly
queues that same link for the stored address; creating an account sends nothing automatically. Administrator links last
72 hours. A replacement or cancellation invalidates the earlier link. Forwarding a link delegates the ability to set
that account's password.

For host recovery, provide only the runtime database credential (`DATABASE_URL` or `DATABASE_URL_FILE`) and a valid
`REVERIE_PUBLIC_URL`, then run:

```sh
reverie-api reset-password reader@example.test
```

For an account with a password, stdout begins `Password reset link for {email} (expires in 10 minutes):`, followed by
the link. Without a password it begins `Password setup link for {email} (expires in 10 minutes):`. The link uses
`/reset-password` or `/set-password` respectively, replaces the administrator/host link slot and expires after ten
minutes. The command refuses an unknown or disabled account and a missing or invalid public URL before mutation. It does
not load SMTP, storage or identity-provider settings, enable local authentication, clear login backoff or log the link.
Protect the terminal output as a credential.

Redemption consumes the link once, invalidates other password-replacement proofs and signs out existing sessions. It
does not sign the person in or revoke device tokens. An ordinary own-password change signs out sessions while preserving
existing replacement proofs. A lost response can leave the outcome uncertain; try signing in before submitting again.

## Delivery and retention

One supervised worker submits encrypted Postgres jobs. Each SMTP attempt has a 30-second bound. Connection, TLS,
authentication, temporary refusal and timeout failures retry after 30 seconds, two minutes, five minutes and fifteen
minutes: five attempts in total. Permanent refusal and unreadable payloads stop immediately. Restart recovers an
interrupted attempt, which still counts against that budget. A lost acknowledgement can produce a duplicate; the stable
Message-ID does not guarantee deduplication.

Bearer mail stops when its proof expires or is revoked, the account is disabled, or its stored address changes. Security
notices retain their event's recipient and survive later address changes or disablement. Notices expire after 24 hours.
Administrator-link and notice payloads are removed at a final outcome. Verification and recovery payloads can remain
until proof expiry or revocation so a requested repeat can reuse the token. Final job metadata stays for seven days; the
hourly sweep removes expired payloads, proofs and old terminal rows even when mail is disabled.

Logs and administrator delivery status report only safe categories: `connection`, `tls`, `authentication`,
`temporary_refusal`, `permanent_refusal`, `timeout` and `payload_unreadable`. They expose no message bodies, tokens,
recipients, relay credentials or raw relay replies. The Users dialog reports relay acceptance without promising inbox
delivery. Mail queued in the password or address transaction commits with it; an enqueue failure rolls the change back,
but a later submission failure never reverses it.

## Disable and re-enable mail

Set `REVERIE_SMTP_ENABLED=false` and restart the process. This stops new creating mail jobs and the worker. Email reset
shows its unavailable state; administrator copying and host recovery remain available. Stored jobs wait. Previously
issued proofs keep their authority until expiry or revocation. Restore the setting and restart with the same queue key
to resume; expired or revoked jobs are discarded before submission. Disabling mail alone does not cancel links.

## Queue-key loss or replacement

Without the key, queued payloads cannot be recovered. A wrong key produces `payload_unreadable`, with no clear-text
fallback. Account email addresses remain readable in the account table: encryption covers mail payloads, not the whole
database. AES-GCM with random nonce values supports about 2^32 encryption operations per key. Reverie keeps no usage
counter; database restores do not reset cumulative key use.

For planned replacement, disable mail, stop the process and allow existing email proofs to expire. For key loss, stop
the process immediately. Back up the database and retained key material separately before maintenance. With the process
stopped, connect to the resolved instance database as its migrator and run:

```sql
BEGIN;
UPDATE mail_jobs
SET state = CASE WHEN state IN ('pending', 'in_progress') THEN 'cancelled' ELSE state END,
    terminal_at = COALESCE(terminal_at, clock_timestamp()),
    nonce = NULL,
    payload = NULL;
DELETE FROM account_email_proofs;
COMMIT;
```

This discards all queued messages and email proofs while preserving job timestamps and allowance metadata. It leaves
mailbox verification, passwords and administrator/host credential links intact. Install and separately back up a new
random key, then re-enable mail and restart. Request fresh verification or recovery mail as needed; administrators must
supply a still-live token again to email a copied link. Manual and host recovery remain available throughout. Restoring
an old database backup with its matching old key can restore old queued authority, so treat both as credential material.

## Security notices

Notices use `{origin}` from the public URL and `{support}` from Reply-To. `{time}` is written in full in UTC, for
example "Saturday 25 October at 2:00 pm UTC", without a year. They do not automatically lock accounts or reverse
changes.

### Your Reverie password was changed

The password for the Reverie account {email} at {origin} was changed on {time}. All sessions were signed out. If you
didn't make this change, write to {support} straight away.

### Your Reverie password is set

A password was set for the Reverie account {email} at {origin} on {time}. If you didn't do this, write to {support}
straight away.

### Your email address is verified

{email} is now verified for your Reverie account at {origin}. If you didn't do this, write to {support}.

### Your Reverie email address was changed

An administrator changed the email address for your Reverie account at {origin} on {time}. This address is no longer
associated with your account. Previously queued security notices may still arrive. If you didn't expect this, write to
{support}.

### Your email address was removed from Reverie

An administrator removed the email address from your Reverie account at {origin} on {time}. This address is no longer
associated with your account. Previously queued security notices may still arrive. If you didn't expect this, write to
{support}.

## Local development

Keep mail credentials in backend-only environment or secret files. Do not use `VITE_*` variables: those are browser
configuration. The Vite development proxy forwards API requests to the backend; it never needs SMTP credentials or the
queue key. Use a controlled relay and disposable accounts for automated verification, and keep one owner for any shared
backend or development database.
