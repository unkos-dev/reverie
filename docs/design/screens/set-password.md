# Set-password screen

A credential-link holder chooses a password for the account bound to that link. Copy in double quotes is exact. Layout
is approved.

## Entry point and route

Route: `/set-password`, inside AuthShell and outside the authenticated application shell. Read the token from the URL
fragment in the route loader and clear it with `history.replaceState` before the page mounts or lookup starts. Keep the
token only in router memory.

The page serves every password link: administrator links, links printed by the host `reset-password` command and
password reset emails. Each reaches the same states below; the page cannot tell them apart.

## Layout

Use the shared AuthShell: a centred column no wider than 24 rem, 24 px horizontal and 64 px vertical outer padding, and
the Reverie lockup above the card. Keep title, description, fields and primary action in that order. Stack form fields
at 16 px intervals. Use the existing policy hint beneath the new-password field. Short windows scroll vertically.

## Checking and invalid states

While lookup runs, show a skeleton with no title.

For an invalid, expired, used, disabled-account or no-email link, show:

Title: "This link can't be used".

Body: "It may have expired or already been used. Ask your administrator for a new link."

Link: "Go to sign in".

For a network failure, 429 or 5xx lookup response, show the error and a full-width "Try again" button 16 px below it.
Activation repeats only lookup using the token retained in memory. Disable the button while lookup is pending. Keep the
fragment cleared. Invalid or expired links have no retry button.

## Password form

For an account without a password:

Title: "Set your password".

Body: "Choose a password for {email}."

For an account with a password:

Title: "Reset your password".

Body: "Choose a new password for {email}. Saving it signs you out everywhere."

Both forms have "New password" and "Confirm password" fields, the existing policy hint and a "Set password" submit
button. Include a hidden field with `autocomplete="username"` holding {email}; both visible fields use
`autocomplete="new-password"`.

A mismatch shows "The passwords don't match." inline. Policy, 503 and 429 errors render their existing `detail`.
Submission does not sign the person in. Reconnect does not repeat lookup or replace the form; entered passwords stay in
their fields.

If password-policy loading fails, show "Could not load the password policy." beneath the new-password field, followed by
an existing small outline button labelled "Retry password policy". Retry in place without clearing either password field
or the captured token. Keep the action visible but disabled while retrying. Submission stays disabled until the policy
loads; do not direct the person to reload the page.

If the redemption response is lost to a network failure, retain the form and show this inline message:

"Your password may have changed. Try signing in with the password you entered before submitting again."

"Try signing in" links to `/login` on activation. Do not retry redemption automatically or show success without a
successful response.

## Saved

Title: "Password set".

Body: "You can now sign in with your new password."

Button: "Sign in", leading to `/login` only on activation.

Saving is terminal for this page. Stop lookup after success; reconnects and late lookup failures cannot replace the
saved screen.

## Keyboard and accessibility

Use labelled inputs, normal reading and Tab order, visible focus rings and keyboard submission. Permit paste and
password managers. Inline errors are announced by the existing FieldError primitive. Disable repeated submission while
pending and retain entered values after a refusal. The loading skeleton has an accessible status label.

## Rendered acceptance

- Compare new-account and reset forms at 1920 x 1080 in both themes.
- Confirm exact copy, form spacing, policy hint and password-manager attributes.
- Check checking, invalid, mismatch, policy, 429, 503 and saved states.
- Check lookup retry, its pending disabled state and saved-state stability after reconnect.
- Confirm the fragment clears and the page neither logs nor persists the token.
- Check keyboard completion and explicit navigation to sign in after saving.
