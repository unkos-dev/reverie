# Forgot-password screen

A person with a local password asks for a reset link by email. The page never reveals whether an address belongs to an
account or can receive recovery email. Copy in double quotes is exact. The whole screen is target design.

## Entry point and route

Route: `/forgot-password`, inside AuthShell and outside the authenticated application shell. The Login screen's "Forgot
password?" link opens it. The page reads `local_auth_enabled` and `email_recovery_enabled` from the existing
setup-status query. When local authentication is disabled, the page redirects to `/login`.

## Layout

Use the shared AuthShell: a centred column no wider than 24 rem, 24 px horizontal and 64 px vertical outer padding, and
the Reverie lockup above the card. Keep title, description, field and primary action in that order, with 4 px between
the title and description and 16 px between the field, its error and the button. The footer holds the "Back to sign in"
link in every state. Short windows scroll vertically.

## Loading and settings failure

While setup status loads, show a skeleton with no title and an accessible status label.

If setup status fails to load, show "Could not load sign-in settings." and a full-width "Try again" button 16 px below
it. Activation repeats only the setup-status request. Disable the button while the request is pending.

## Email recovery unavailable

When `email_recovery_enabled` is false, show no form.

Title: "Forgot password".

Body: "Password reset by email isn't available on this server. Ask your administrator for a link to reset your
password."

## Request form

Title: "Forgot password".

Body: "Enter your account's email address."

One "Email" field with `type="email"` and `autocomplete="email"`, then a full-width "Send reset link" button. A value
that fails the shared email pattern shows "Enter a valid email address." beneath the field, sends nothing and focuses
the field. Disable the button while the request is pending and keep the entered value.

A 429 response renders its existing `detail` inline beneath the field. A network failure or 5xx response shows "Could
not send the request. Try again." inline. Both keep the entered value and re-enable the button.

## Request sent

Every accepted request shows the same state, whether or not the address belongs to an account, the account is enabled,
its address is verified, or a message is sent.

Title: "Check your email".

Body: "Check {email} for a password reset link. The email says when the link expires."

Second paragraph: "No email after a few minutes? Check your spam folder, or ask your administrator for a link."

Below the text, a full-width outline button "Use a different email" returns to the request form with an empty field.
{email} is the address the person entered. The page does not retry the request or poll for delivery.

## Recovery email

Only an eligible account receives this message. {origin} is the configured public URL, {expiry} the proof's expiry in
the email date format of
[authentication mail delivery](../../specs/operations/design/0029-authentication-mail-delivery.md) and {support} the
configured support address, which is also the Reply-To.

Subject: "Reset your Reverie password"

Body:

```text
Someone asked to reset the password for the Reverie account {email} at {origin}.

To choose a new password, open this link:
{origin}/reset-password#{token}

The link works once and expires on {expiry}. Using it signs the account out everywhere.

If you didn't ask for this, ignore this email. This request did not change your password. To report a problem, reply to this email or write to {support}.
```

The link opens the Set password screen at `/reset-password`.

## Keyboard and accessibility

Use a labelled input, normal reading and Tab order, visible focus rings and keyboard submission. Inline errors are
announced by the existing FieldError primitive. When the request-sent state replaces the form, move focus to its title.
The loading skeleton has an accessible status label.

## Rendered acceptance

- Compare the request form, request-sent and unavailable states at 1920 x 1080 in both themes and at 390 x 844.
- Confirm exact copy, field attributes and footer link in every state.
- Check loading, settings failure and retry, invalid email, 429, network failure and pending states.
- Confirm an unknown address and a known address produce the same request-sent state.
- Check keyboard completion and focus placement after sending.
