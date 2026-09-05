// Synchronous pre-paint theme resolution.
//
// Reads the `reverie_theme` cookie (one of "system", "light", "dark") and
// sets `<html data-theme>` to a concrete "light" or "dark" before the React
// bundle loads, eliminating the unstyled flash on cold loads. Plain ES5
// because this body runs before any module loader; vite-plugins/csp-hash.ts
// hashes it at build and emits the matching CSP `'sha256-...'` source.
//
// THREAT: this script is the *only* inline JavaScript whitelisted by the
// runtime CSP. Any edit lands as a new sha256 in `dist/csp-hashes.json`
// the backend serves in the response `Content-Security-Policy` header — a
// drift between source and CSP would fail-closed in the browser (script
// refused, FOUC returns; not an exposure). The csp-hash plugin's closing-
// script-tag guard (`/<\/script[\s/>]/i`) is load-bearing: a `</script`
// literal anywhere in this body — including inside a comment — would
// escape the inline script element when injected into index.html and
// execute as page-level HTML. Keep the body free of that pattern; the
// Vite plugin will fail the build if one slips in.
//
// Cross-stack invariants:
//   - Cookie name "reverie_theme" matches THEME_COOKIE_NAME on the backend
//     (backend/src/auth/theme_cookie.rs) and frontend (lib/theme/cookie.ts).
//     All three MUST agree.
//   - Body must not contain a closing-script-tag literal (would terminate
//     the surrounding inline script when injected into index.html); the
//     csp-hash Vite plugin throws if it sees one.
//   - try/catch fallback to "light" is the documented worst-case path
//     (malformed cookie, parse error, or matchMedia failure).
//
// Cross-references:
//   - `docs/adr/0004-tiered-comment-policy-for-an-open-source-codebase.md` § Tier 2 (threat-annotation
//     shape).
//   - `docs/adr/0030-adopt-oxlint-replacing-the-eslint-toolchain.md` (docstring
//     presence is no longer lint-gated; this file-header is the Tier 2
//     documentation surface, since the IIFE exports nothing a docstring
//     could attach to).
//   - `frontend/vite-plugins/csp-hash.ts::cspHashPlugin` (the plugin that
//     hashes this body).
(function () {
  try {
    var cookie = document.cookie.split("; ").find(function (c) {
      return c.indexOf("reverie_theme=") === 0;
    });
    var pref = cookie ? cookie.split("=")[1] : "system";
    if (pref !== "system" && pref !== "light" && pref !== "dark") {
      pref = "system";
    }
    var effective = pref;
    if (pref === "system") {
      effective = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    document.documentElement.dataset.theme = effective;
  } catch (e) {
    document.documentElement.dataset.theme = "light";
    // Surface the failure so a regression in this hashed inline script
    // (e.g. d29a7cc which fixed a </'+'script literal that the catch
    // would otherwise have hidden) leaves a breadcrumb for debugging.
    if (window.console && window.console.warn) {
      window.console.warn("[reverie] FOUC theme resolution failed; defaulting to light", e);
    }
  }
})();
