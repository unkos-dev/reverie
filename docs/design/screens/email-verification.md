# Email verification screens

A signed-in account holder confirms that they control the email address already stored on their account. Verification
enables password reset by email and security notices. It never changes the address or a password and never signs anyone
in. Copy in double quotes is exact. The whole screen pair is target design.

## Entry points and routes

`/account/email` shows the address and its state and sends a verification email. It is inside the authenticated
application, reached from the user menu item "Email address", which appears for every signed-in account.

`/verify-email` confirms a mailed challenge. It is outside the authenticated application shell, so a person who is not
signed in still reaches it. The route loader reads the token from the URL fragment and clears it with
`history.replaceState` before the page mounts or any request starts. The token stays only in router memory. It is never
written to local or session storage or added to a sign-in return URL.

## Layout

Both routes use the shared AuthShell: a centred column no wider than 24 rem, 24 px horizontal and 64 px vertical outer
padding, and the Reverie lockup above the card. Keep title, description, content and primary action in that order, at 16
px intervals. `/account/email` has the footer link "Back to library"; `/verify-email` has none. Short windows scroll
vertically.

On `/account/email`, the address sits in a read-only row beneath the description: the address in normal foreground text,
followed by a status badge, "Verified" or "Not verified". The badge uses the existing badge component and states its
meaning in text.

## Email address page

While the verification status loads, show a skeleton with no title and an accessible status label. If loading fails,
show "Could not load your email settings." and a full-width "Try again" button 16 px below it. Disable the button while
the request is pending.

All loaded states have the title "Email address".

| State | Condition | Body | Action |
| ----- | --------- | ---- | ------ |
| No address | The account has no email | "Your account has no email address. Ask your administrator to add one." | None |
| Verified | The current address is verified | "Your email address is verified." | None |
| Mail unavailable | Unverified, mail not configured | "Email isn't set up on this server, so this address can't be verified yet." | None |
| Not verified | Unverified, mail configured | "Verify that you control this email address." | "Send verification email" |
| Sent | A send request succeeded on this page | "Verification email requested. Open the link in it while signed in to this account. The email says when the link expires." | "Send again", outline |

The address row appears in every state except No address. The Sent state lasts until the person leaves the page or the
status refetches as verified. "Send again" repeats the request; within 60 seconds of the previous send, the server
reuses the pending message rather than sending another.

Disable the action while its request is pending. A 503 response shows "Email isn't available right now. Try again
later." inline and returns to the Not verified state. A network failure or other 5xx response shows "Could not confirm
the request. Try again." inline and keeps the action enabled.

## Verification email

{origin} is the configured public URL, {expiry} the challenge's expiry and {support} the configured support address,
which is also the Reply-To.

Subject: "Verify your email address for Reverie"

Body:

```text
Someone asked to verify {email} for a Reverie account at {origin}.

To confirm it, sign in to that account and open this link:
{origin}/verify-email#{token}

The link works once and expires on {expiry}.

If you didn't ask for this, ignore this email. To report a problem, reply to this email or write to {support}.
```

{expiry} is written out in full in UTC as `{weekday} {day} {month} at {hour}:{minute} {am|pm} UTC`, with the same rules
as the Users screen's link emails. Proposed example, awaiting approval: "Saturday 25 October at 2:00 pm UTC".

## Confirmation page

While the session check runs, show a skeleton with no title and an accessible status label. A missing or malformed
fragment token goes straight to the invalid state.

| State | Title | Body | Action |
| ----- | ----- | ---- | ------ |
| Signed out | "Sign in to verify your email" | "Sign in to the account this email was sent to, then open the link in the email again." | Link "Go to sign in" to `/login` |
| Ready | "Verify your email address" | "Confirm {email} as the email address for your account." | Button "Verify email" |
| Verified | "Email address verified" | "Your email address is verified." | Link "Go to email settings" to `/account/email` |
| Invalid | "This link can't be used" | "It may have expired, already been used, or belong to a different account. Send a new verification email from your email settings." | Link "Go to email settings" to `/account/email` |

{email} is the signed-in account's current address. Verification happens only when the person activates "Verify email";
mounting, reloading or reconnecting never sends the confirmation. Disable the button while the request is pending.

An expired, used, replaced or wrong-account challenge, or one for an address the account no longer holds, all show the
same Invalid state. A 429 response renders its existing `detail` inline and keeps the button. If the confirmation
response is lost to a network failure, keep the Ready state and show:

"Your email address may already be verified. Check your email settings before trying again."

"Check your email settings" links to `/account/email`. The page does not retry automatically or show Verified without a
successful response. Verified is terminal: later session or query updates cannot replace it.

## Keyboard and accessibility

Use normal reading and Tab order, visible focus rings and keyboard activation. Inline errors are announced by the
existing FieldError primitive. When a state change replaces the card content, move focus to its title. Status badges
carry their meaning in text. Loading skeletons have accessible status labels.

## Rendered acceptance

- Compare each Email address state and each confirmation state at 1920 x 1080 in both themes and at 390 x 844.
- Confirm exact copy, badge text, the user menu item and both footers.
- Check loading, load failure and retry, pending, 503, 429, network failure and lost-response states.
- Confirm the fragment clears before any request and the token never reaches storage, logs or a sign-in return URL.
- Confirm the confirmation request is sent only on activation, and Verified stays after reconnect.
- Confirm an OIDC-only account can verify its address and a different signed-in account sees the Invalid state.
