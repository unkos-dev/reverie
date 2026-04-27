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
//! (Path=/, Max-Age=31536000, SameSite=Lax, no HttpOnly, no Secure).
//! Drift produces two cookies of the same name with divergent attributes
//! in the browser jar.

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

/// Cookie name for the FOUC theme preference. Duplicated in:
///   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
///   - frontend/src/lib/theme/cookie.ts
///
/// All three MUST agree. Tracked as instance 1 under UNK-105.
pub const THEME_COOKIE_NAME: &str = "reverie_theme";

/// Server-side allowlist for `theme_preference`. The set is shared with the
/// frontend `ThemePreference` union literal type ("system" | "light" | "dark"
/// in `frontend/src/lib/theme/cookie.ts`); cross-stack drift is tracked under
/// UNK-105.
pub const ALLOWED_THEMES: &[&str] = &["system", "light", "dark"];

pub fn set_theme_cookie(jar: CookieJar, value: &str) -> CookieJar {
    let cookie = Cookie::build((THEME_COOKIE_NAME, value.to_owned()))
        .path("/")
        .http_only(false)
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
        let jar = set_theme_cookie(CookieJar::new(), "dark");

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
        // Must not have Secure — matches session cookie behaviour
        // (plain HTTP behind TLS-terminating proxy).
        assert_eq!(cookie.secure(), None);
    }

    #[test]
    fn allowed_themes_matches_frontend_union() {
        // Sanity: any change here must mirror `ThemePreference` in
        // frontend/src/lib/theme/cookie.ts. UNK-105 drift guard.
        assert_eq!(ALLOWED_THEMES, &["system", "light", "dark"]);
    }
}
