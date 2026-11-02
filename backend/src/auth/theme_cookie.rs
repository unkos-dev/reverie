//! FOUC theme-preference cookie helpers.
//!
//! Lifecycle: `reverie_theme` **survives logout by design**. It is device
//! state (visual preference, non-PII, non-session-scoped), not session
//! state. See `docs/design/visual-identity.md` § Theme Cookie Lifecycle
//! for the rationale and the contrast rule: any future *session-state*
//! cookie MUST be `HttpOnly` and MUST be cleared on logout — this one
//! is the explicit counterexample.
//!
//! Attribute parity: the frontend `writeThemeCookie`
//! (`frontend/src/lib/theme/cookie.ts`) MUST produce matching attributes
//! (Path=/, Max-Age=31536000, SameSite=Lax, Secure, no `HttpOnly`). Drift
//! produces two cookies of the same name with divergent attributes in
//! the browser jar.
//!
//! Secure is always set. Reverie's threat model is "multi-user exposed
//! instance"; HTTP-only public deployments are unsupported. Localhost
//! dev still works because Chrome (≥v89) and Firefox treat
//! `http://localhost` as a secure context and accept Secure cookies on
//! it. An operator running HTTP-only behind a public DNS name will see
//! the cookie silently rejected by the browser — the documented signal
//! to put the deployment behind TLS.

#![expect(
    clippy::disallowed_types,
    reason = "the cookie builder's `max_age` takes a time::Duration and accepts no chrono equivalent; see docs/adr/0046-chrono-is-the-first-party-datetime-crate.md"
)]

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

use crate::models::theme_preference::ThemePreference;

/// Cookie name for the FOUC theme preference. Duplicated in:
///   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
///   - frontend/src/lib/theme/cookie.ts
///
/// All three MUST agree; this is the backend instance of that cross-stack
/// constant.
pub const THEME_COOKIE_NAME: &str = "reverie_theme";

/// Add or replace the `reverie_theme` cookie in `jar` with the given preference.
///
/// Cookie attributes are set to match the frontend's `writeThemeCookie`
/// (`frontend/src/lib/theme/cookie.ts`) exactly: `Path=/`,
/// `Max-Age=31536000`, `SameSite=Lax`, `Secure`, no `HttpOnly`.
/// Attribute drift between this function and the frontend writer produces
/// two cookies of the same name with different attributes in the browser jar,
/// which silently breaks FOUC suppression. See the module-level docs for
/// the lifecycle rationale.
///
/// The returned `CookieJar` replaces the caller's jar and must be included
/// in the response via `(jar, response)` — discarding it silently drops the
/// cookie write.
pub fn set_theme_cookie(jar: CookieJar, value: ThemePreference) -> CookieJar {
    let cookie = Cookie::build((THEME_COOKIE_NAME, value.as_str().to_owned()))
        .path("/")
        .http_only(false)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(365))
        .build();
    jar.add(cookie)
}
