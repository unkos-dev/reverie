# Users screen

Administrators create accounts and give people a single-use link to choose their own password. Copy in double quotes is
exact. Layout is approved.

## Entry point and route

Route: `/admin/users`, inside the authenticated application shell. Non-administrators return to `/library`.

## Layout

Use a centred, 72 rem content column with 24 px padding. Place the "Users" heading and "Create user" action in one row,
24 px above the table. Retain the Name, Email, Role, Status and Actions columns.

Status badges sit together in the Status cell. Row actions remain right-aligned. Dialogs use the existing shared Dialog
primitive, with a title, explanatory text, vertically stacked fields at 16 px intervals and right-aligned footer
actions. The link result uses the same dialog width and a read-only link field. Use existing theme tokens and controls.

## Create account

The dialog title and submit button say "Create user". Its description is:

"Create the account, then send the person the link it gives you. They choose their own password."

Fields, in order: "Display name", "Email", "Role". There is no password input. Successful creation opens the link
result. Submission errors retain the fields and render the returned detail.

## Link result

Title: "Send this link to {name}".

For a new account:

"{name} uses this link to choose a password. It expires {expiry}. You won't see it again. If it's lost, create a new
link from the user list."

For a reset:

"{name} uses this link to choose a new password. It expires {expiry}. You won't see it again."

Show the read-only link, then "Copy link" and "Done". Copying succeeds with the toast "Link copied.". Build the URL from
the browser's origin as `/set-password#{token}` when the account has no password and `/reset-password#{token}` when it
has one. Closing the result clears the displayed link. Format {expiry} with the existing date-time formatter. If
clipboard access is unavailable or denied, keep the link visible and show the error toast "Couldn't copy the link.
Select the link and copy it manually.".

## Reset and cancel

The row action "Reset password" opens a confirmation titled "Reset password for {name}?" with this body:

"This creates a link {name} can use to choose a new password. Their current password keeps working until the link is
used. Using it signs them out everywhere."

When a link is active, add "This replaces the link created earlier.". Buttons: "Cancel" and "Create link". Successful
issuance opens the reset link result. A no-email refusal shows "Add an email address to {name} before creating a link."

An active link adds an "invited" badge for an account without a password, or "link pending" for an account with one. Its
accessible name appends "expires {expiry}". "Cancel link" appears while a link is active; success shows the toast "Link
cancelled.". Cancelling does not change the password.

Hide "Reset password" on the administrator's own row. Hide "Create user", "Reset password" and "Cancel link" when local
authentication is disabled. Retain the existing account and role controls, loading skeletons, empty state and list-error
state.

While authentication settings load, show "Loading authentication settings…" beneath the heading. If loading fails, show
"Could not load authentication settings." with a small outline "Retry authentication settings" button. Retry in place,
keeping the button disabled during the request. Keep the user list and existing account and role controls available.
Credential actions appear once settings confirm local authentication is enabled; a failed background refresh retains the
previously loaded settings.

## Send link by email

This section is target design.

When mail is configured, the link result adds "Send email" before "Copy link". The button is absent when mail is not
configured; copying works the same either way. Sending uses the link currently shown and the account's stored address.
The administrator enters no address. Nothing is sent when the account is created; sending happens only on activation.

After activation, a status line appears 16 px beneath the link field. While the dialog is open, the page refreshes that
delivery's status every 3 seconds and stops at Accepted, Failed or when the dialog closes.

| Status | Text | Actions |
| ------ | ---- | ------- |
| Queued | "Sending to {email}…" | "Send email" disabled |
| Accepted | "The mail server accepted the email for {email}. This doesn't confirm it reached the inbox." | "Send email" hidden |
| Failed | "Sending failed: {reason}. Copy the link instead, or try again." | "Try again" replaces "Send email" |

| Failure category     | {reason}                                          |
| -------------------- | ------------------------------------------------- |
| `connection`         | "couldn't connect to the mail server"             |
| `tls`                | "the secure connection to the mail server failed" |
| `authentication`     | "the mail server rejected Reverie's sign-in"      |
| `temporary_refusal`  | "the mail server refused it for now"              |
| `permanent_refusal`  | "the mail server refused it"                      |
| `timeout`            | "the mail server didn't respond in time"          |
| `payload_unreadable` | "the queued message couldn't be read"             |

"Try again" sends the same link as a new delivery. A 503 response shows "Email isn't available right now. Copy the link
instead." A refusal because the link was replaced, cancelled or expired shows "This link is no longer active. Create a
new link to send it." A network failure shows "Couldn't confirm the request. Try again." In every case the link and
"Copy link" stay available. Closing the dialog stops status refresh and discards the link.

Link email, with {origin} the configured public URL, {expiry} the link's expiry in the email date format of
[authentication mail delivery](../../specs/operations/design/0029-authentication-mail-delivery.md) and {support} the
configured support address, also the Reply-To. For an account without a password:

Subject: "Set your Reverie password"

```text
An administrator invited you to set a password for the Reverie account {email} at {origin}.

To choose your password, open this link:
{origin}/set-password#{token}

The link works once and expires on {expiry}.

If you weren't expecting this, ignore this email or write to {support}.
```

For an account with a password:

Subject: "Reset your Reverie password"

```text
An administrator started a password reset for the Reverie account {email} at {origin}.

To choose a new password, open this link:
{origin}/reset-password#{token}

The link works once and expires on {expiry}. Your current password keeps working until the link is used. Using it
signs the account out everywhere.

If you weren't expecting this, write to {support}.
```

## Keyboard and accessibility

Retain table headers, labelled fields, visible focus rings and keyboard activation. Dialogs trap focus, close with
Escape and return focus to their trigger. Submission cannot repeat while pending. The read-only link participates in Tab
order and supports text selection. Badges express state in text and expose their expiry to assistive technology.

## Rendered acceptance

- Review the table, create dialog, reset confirmation and both link results at 1920 x 1080 in both themes.
- Confirm exact copy, field order, badge text and expiry formatting.
- Confirm own-row and local-auth-disabled action hiding, and the no-email refusal.
- Check dialog focus, Tab order, Escape, clipboard feedback and pending submission.
- Confirm administrators never enter a password and links are displayed only in the result.
- With mail configured, check each send status and failure message, refresh stopping at a final status or on close, and
  "Send email" absent when mail is not configured.
