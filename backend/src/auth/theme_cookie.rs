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
//! (Path=/, Max-Age=31536000, SameSite=Lax, no HttpOnly). Drift produces
//! two cookies of the same name with divergent attributes in the browser
//! jar.
//!
//! Secure is conditional on the deployment context (`SecurityConfig::
//! behind_https`): when the user-facing connection is HTTPS — whether
//! TLS-terminated at a reverse proxy or directly at the backend — the
//! cookie is emitted with Secure. The frontend mirrors this via
//! `location.protocol === 'https:'`, which converges to the same answer
//! the browser sees.

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

use crate::models::theme_preference::ThemePreference;

/// Cookie name for the FOUC theme preference. Duplicated in:
///   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
///   - frontend/src/lib/theme/cookie.ts
///
/// All three MUST agree. Tracked as instance 1 under UNK-105.
pub const THEME_COOKIE_NAME: &str = "reverie_theme";

pub fn set_theme_cookie(jar: CookieJar, value: ThemePreference, secure: bool) -> CookieJar {
    let mut builder = Cookie::build((THEME_COOKIE_NAME, value.as_str().to_owned()))
        .path("/")
        .http_only(false)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(365));
    if secure {
        builder = builder.secure(true);
    }
    jar.add(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::CookieJar;

    #[test]
    fn set_theme_cookie_writes_canonical_attributes_without_secure() {
        let jar = set_theme_cookie(CookieJar::new(), ThemePreference::Dark, false);

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
        // Secure must be absent when behind_https=false (plain HTTP
        // dev environment, or HTTP-only homelab without a TLS proxy).
        assert_eq!(cookie.secure(), None);
    }

    #[test]
    fn set_theme_cookie_sets_secure_when_behind_https() {
        let jar = set_theme_cookie(CookieJar::new(), ThemePreference::Light, true);

        let cookie = jar
            .get("reverie_theme")
            .expect("cookie present in returned jar");

        // Other attributes unchanged from the false case.
        assert_eq!(cookie.value(), "light");
        assert_eq!(cookie.http_only(), Some(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.max_age(), Some(Duration::days(365)));
        // Secure set when behind_https=true (direct TLS or HTTPS proxy).
        assert_eq!(cookie.secure(), Some(true));
    }

}
