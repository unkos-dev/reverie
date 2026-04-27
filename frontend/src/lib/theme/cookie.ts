// Cookie name for the FOUC theme preference. Duplicated in:
//   - backend/src/auth/theme_cookie.rs (THEME_COOKIE_NAME const)
//   - frontend/src/fouc/fouc.js (inline FOUC script body, CSP-hashed at build)
// All three MUST agree. Tracked as instance 1 under UNK-105.
export const THEME_COOKIE_NAME = "reverie_theme";

// ONE YEAR in seconds. MUST equal `Duration::days(365).whole_seconds()` on the
// backend side (365 × 86400 = 31_536_000). If this constant changes, the
// matching `.max_age(Duration::days(...))` in
// backend/src/auth/theme_cookie.rs MUST change in the same commit.
const ONE_YEAR_SECONDS = 31_536_000;

export type ThemePreference = "system" | "light" | "dark";

export function readThemeCookie(): ThemePreference | null {
  const pairs = (document.cookie || "").split("; ");
  for (const pair of pairs) {
    const eq = pair.indexOf("=");
    if (eq === -1) continue;
    if (pair.slice(0, eq) !== THEME_COOKIE_NAME) continue;
    const raw = pair.slice(eq + 1);
    if (raw === "system" || raw === "light" || raw === "dark") return raw;
    return null;
  }
  return null;
}

// Attribute parity with backend `set_theme_cookie`:
//   Path=/            — matches backend
//   Max-Age=31536000  — matches backend's Duration::days(365)
//   SameSite=Lax      — matches backend
//   Secure            — always set. Reverie requires HTTPS for any
//                       publicly-reachable deployment (see backend
//                       theme_cookie.rs module header). Browsers accept
//                       Secure cookies on http://localhost as a secure
//                       context, so dev still works.
//   HttpOnly          — intentionally absent on BOTH sides (JS reads for FOUC)
// Drift is caught by the backend `set_theme_cookie_*` tests plus this
// module's `cookie.test.ts` attribute assertions.
export function writeThemeCookie(value: ThemePreference): void {
  document.cookie =
    `${THEME_COOKIE_NAME}=${value}; ` +
    `Path=/; ` +
    `Max-Age=${ONE_YEAR_SECONDS}; ` +
    `SameSite=Lax; ` +
    `Secure`;
}
