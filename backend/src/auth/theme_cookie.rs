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
//! (Path=/, Max-Age=31536000, SameSite=Lax, Secure, no HttpOnly). Drift
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

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

use crate::models::theme_preference::ThemePreference;

/// Cookie name for the FOUC theme preference. Duplicated in:
///   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
///   - frontend/src/lib/theme/cookie.ts
///
/// All three MUST agree. Tracked as instance 1 under UNK-105.
pub const THEME_COOKIE_NAME: &str = "reverie_theme";

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::CookieJar;

    #[test]
    fn set_theme_cookie_writes_canonical_attributes() {
        let jar = set_theme_cookie(CookieJar::new(), ThemePreference::Dark);

        // String-compare the literal so a rename of THEME_COOKIE_NAME trips
        // the test and surfaces UNK-105 cross-stack drift before it lands.
        let cookie = jar
            .get("reverie_theme")
            .expect("cookie present in returned jar");

        assert_eq!(cookie.name(), "reverie_theme");
        assert_eq!(cookie.value(), "dark");
        assert_eq!(cookie.http_only(), Some(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.max_age(), Some(Duration::days(365)));
        // Secure is always set — see module-level rationale.
        assert_eq!(cookie.secure(), Some(true));
    }
}
