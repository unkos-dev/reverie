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
result. A blank display name shows "Enter a display name." and an invalid email "Enter a valid email address." beneath
the field; a submission they block sends nothing and focuses the first failing field. Server errors retain the fields
and render the returned detail as a general alert.

## Link result

Title: "Send this link to {name}".

For a new account:

"{name} uses this link to choose a password. It expires {expiry}. You won't see it again. If it's lost, create a new
link from the user list."

For a reset:

"{name} uses this link to choose a new password. It expires {expiry}. You won't see it again."

Show the read-only link, then "Copy link" and "Done". Copying succeeds with the toast "Link copied.". Build the URL from
the browser's origin as `/set-password#{token}`. Closing the result clears the displayed link. Format {expiry} with the
existing date-time formatter. If clipboard access is unavailable or denied, keep the link visible and show the error
toast "Couldn't copy the link. Select the link and copy it manually.".

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
