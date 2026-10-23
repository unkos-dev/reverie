# Login screen

The login page offers the sign-in methods enabled on the instance and explains an unsuccessful OIDC attempt without
showing provider text. Copy in double quotes is exact.

## Entry point and route

Route: `/login`. OIDC start and callback failures navigate here with one of six values in `outcome`. The page parses the
value through a closed Zod enum. A missing, empty, unknown or repeated outcome renders ordinary login, without echoing
any supplied value. A recognised outcome remains on refresh and browser back; the page never rewrites it.

An instance requiring first-administrator setup redirects to `/setup`, including when the URL carries an outcome.

## Layout

Retain the shared AuthShell: a centred column no wider than 24 rem, on the theme canvas, with 24 px horizontal and 64 px
vertical outer padding. The Reverie lockup sits above the card. The card title is "Sign in". Short browser windows
scroll vertically; content must remain reachable without horizontal scrolling.

From top to bottom, the card content holds:

1. A neutral outcome notice when the URL carries one recognised outcome.
2. The local email/password form when local authentication is enabled.
3. The "or" divider when both methods are enabled.
4. The "Sign in with OIDC" button when OIDC is configured.

Use 16 px between the notice and controls. The notice uses the existing surface and border tokens, normal foreground
text and a title at font weight 600. It has no danger colour, modal or toast. The notice heading is an `h2`.

The local form keeps its "Email" and "Password" labels, password-manager attributes and "Sign in" submission button.
Field checks show beneath their field: "Enter a valid email address." and "Enter your password." A submission they block
sends nothing and focuses the first failing field. A credential refusal stays a general alert and marks no field.
"Forgot password?" appears below the card when local authentication is enabled. OIDC-only instances show the provider
button; local-only instances keep their form even if someone enters an outcome URL directly. No duplicate retry button
appears.

## Outcome copy

| Outcome | Title | Body |
| ------- | ----- | ---- |
| `denied` | "Sign-in cancelled" | "Sign-in was cancelled at your provider. You can try again when you're ready." |
| `expired` | "Sign-in expired" | "Start a new sign-in to continue." |
| `unavailable` | "Sign-in unavailable" | "Could not complete sign-in. Try again later." |
| `keys` | "Sign-in could not be verified" | "Try again later. If this keeps happening, contact your administrator." |
| `account` | "Unable to sign in" | "Could not sign in with this account. Contact your administrator for help." |
| `throttled` | "Too many sign-in attempts" | "Wait a moment, then try again." |

Provider `error_description`, error codes, callback state, tokens and other supplied text never appear in a notice. The
account notice does not distinguish an email collision from a disabled account.

## Interaction and focus

The notice is a region labelled by its heading. Focus the heading once when a recognised outcome renders or a new
navigation selects that outcome. Query-cache updates must not take focus back from a control. The next Tab reaches the
first enabled sign-in control. Keep the existing visible focus rings, normal reading order and keyboard activation. Do
not combine focused notice text with a duplicate alert announcement.

The provider button navigates to `/auth/oidc/login` only after a click or keyboard activation. It starts a fresh
attempt; mounting, reloading, navigating back or rendering an outcome never starts OIDC or replays a callback. Local
submission continues to use the local sign-in endpoint. An outcome does not disable either configured sign-in method.

## Loading, errors and setup

While sign-in options load, retain "Loading…" alongside any recognised outcome notice. If the options cannot load,
retain "We couldn't load the sign-in options. Try reloading the page." and its alert semantics alongside the notice.
Neither state initiates provider navigation. First-administrator setup redirection takes precedence over the notice.
Local submission retains its existing pending state and credential-error behaviour.

## Rendered acceptance

- Compare all six outcomes at 1920 x 1080 in light and dark themes against this layout and exact copy.
- Spot-check 2880 x 1800 and 3440 x 1440 for column width, centring and spacing.
- Check WCAG 1.4.10 reflow at 320 CSS px (400% zoom): no horizontal scroll, clipped text or unreachable controls.
- Check notice focus, Tab order, focus visibility, contrast and explicit-click retry.
- Check ordinary login, loading, options failure and setup redirection; check local-only and OIDC-only controls.
- Confirm no provider request occurs before activation, and the first activation starts exactly one fresh attempt.
