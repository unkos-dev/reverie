// Synchronous pre-paint theme resolution.
//
// Reads the `reverie_theme` cookie (one of "system", "light", "dark") and
// sets `<html data-theme>` to a concrete "light" or "dark" before the React
// bundle loads, eliminating the unstyled flash on cold loads. Plain ES5
// because this body runs before any module loader; vite-plugins/csp-hash.ts
// hashes it at build and emits the matching CSP `'sha256-...'` source.
//
// Cross-stack invariants:
//   - Cookie name "reverie_theme" matches THEME_COOKIE_NAME on the backend
//     (backend/src/auth/theme_cookie.rs) and frontend (lib/theme/cookie.ts).
//     UNK-105 tracks any drift.
//   - Body must not contain a closing-script-tag literal (would terminate
//     the surrounding inline script when injected into index.html); the
//     csp-hash Vite plugin throws if it sees one.
//   - try/catch fallback to "light" is the documented worst-case path
//     (malformed cookie, parse error, or matchMedia failure).
(function () {
  try {
    var cookie = document.cookie
      .split('; ')
      .find(function (c) { return c.indexOf('reverie_theme=') === 0; });
    var pref = cookie ? cookie.split('=')[1] : 'system';
    if (pref !== 'system' && pref !== 'light' && pref !== 'dark') {
      pref = 'system';
    }
    var effective = pref;
    if (pref === 'system') {
      effective = window.matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light';
    }
    document.documentElement.dataset.theme = effective;
  } catch (e) {
    document.documentElement.dataset.theme = 'light';
    // Surface the failure so a regression in this hashed inline script
    // (e.g. d29a7cc which fixed a </'+'script literal that the catch
    // would otherwise have hidden) leaves a breadcrumb for debugging.
    if (window.console && window.console.warn) {
      window.console.warn('[reverie] FOUC theme resolution failed; defaulting to light', e);
    }
  }
})();
