import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import type { Plugin, ResolvedConfig } from "vite-plus";

const MARKER = "<!-- reverie:fouc-hash -->";
const FOUC_SOURCE = "src/fouc/fouc.js";
const SIDECAR_FILENAME = "csp-hashes.json";
// Standard base64 alphabet (RFC 4648 §4, with padding). CSP rejects base64url.
const STANDARD_BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;

/**
 * Custom Vite plugin (UNK-106) that turns `src/fouc/fouc.js` into a CSP-hashed
 * inline `<script>` in `index.html` and, on `vite build` only, emits the
 * matching sha256 in `dist/csp-hashes.json` for the backend CSP middleware to
 * consume at startup.
 *
 * THREAT: every byte of inline JavaScript shipped to the browser is
 * unconditionally executed. The runtime CSP only protects against *future*
 * injection, not against compromise of the bundled FOUC script itself. The
 * plugin hashes ONLY the literal contents of `src/fouc/fouc.js`; any other
 * inline `<script>` introduced into `index.html` is intentionally unhashed
 * and will be refused by the browser at navigation time. Two adversary
 * surfaces are closed by guards in `transformIndexHtml`:
 *
 *   1. `<!-- reverie:fouc-hash -->` marker presence + uniqueness. A duplicate
 *      or missing marker fails the build rather than producing an HTML
 *      document with the FOUC script in the wrong place.
 *   2. Closing-script-tag literal detection inside `fouc.js`. The HTML parser
 *      terminates an inline `<script>` at `</script` + whitespace/`/`/`>`;
 *      a literal in source would escape the script element and execute as
 *      page-level HTML. The regex `/<\/script[\s/>]/i` matches the parser's
 *      terminator definition. UNK-114 issue 5 broadened the original
 *      `/<\/script>/i` after a comment-embedded literal in fouc.js silently
 *      terminated the script under D3.13 — the broader regex is load-bearing.
 *
 * The emitted base64 is checked against the RFC 4648 §4 standard alphabet
 * (CSP L3 rejects base64url) so a future Node version that switches digest
 * encoding fails the build instead of producing a CSP that browsers ignore.
 *
 * Cross-references:
 *   - `adr/2026-05-08-tiered-comment-policy.md` § Tier 2 (threat-annotation
 *     shape).
 *   - `adr/2026-05-22-frontend-docstring-tooling.md` § Carve-outs (the `.js`
 *     scope inclusion is for this plugin's pinned source file).
 *   - `backend/src/security/csp.rs` (the consumer of `csp-hashes.json`).
 *
 * @returns Vite plugin that injects + hashes `src/fouc/fouc.js` into the
 *   HTML build output. No public API beyond the standard `Plugin` shape.
 */
export function cspHashPlugin(): Plugin {
  let resolvedConfig: ResolvedConfig | undefined;
  return {
    name: "reverie-csp-hash",
    configResolved(config) {
      resolvedConfig = config;
    },
    transformIndexHtml: {
      order: "post",
      handler(html) {
        if (!resolvedConfig) {
          throw new Error("reverie-csp-hash: configResolved not called");
        }
        const foucPath = resolve(resolvedConfig.root, FOUC_SOURCE);
        const fouc = readFileSync(foucPath, "utf8");

        // Injection-safety guard: the HTML parser terminates an inline
        // <script> at `</script` followed by ASCII whitespace (\s — space,
        // tab, newline, etc.), `/`, or `>`. A trailing `>` is NOT required.
        // Content that matches escapes the script element and renders as
        // HTML. UNK-114 issue 5 broadened this from `/<\/script>/i` after a
        // `</script` literal in a comment terminated fouc.js silently in
        // D3.13. `</script` followed by a name character (e.g. `</scripty`)
        // is not a terminator — the regex requires the parser-recognised
        // suffix to keep the guard from false-positiving.
        if (/<\/script[\s/>]/i.test(fouc)) {
          throw new Error(
            `reverie-csp-hash: ${FOUC_SOURCE} contains a closing-script-tag literal (</script followed by whitespace, /, or >) — inline script injection would break the HTML.`,
          );
        }

        // Marker presence + uniqueness.
        const markerRegex = /<!-- reverie:fouc-hash -->/g;
        const markerCount = (html.match(markerRegex) ?? []).length;
        if (markerCount !== 1) {
          throw new Error(
            `reverie-csp-hash: expected exactly one '${MARKER}' in index.html, found ${String(markerCount)}`,
          );
        }

        const scriptTag = `<script>${fouc}</script>`;
        const injectedHtml = html.replace(MARKER, scriptTag);

        // Hash the script BODY (not the surrounding tag) — CSP L3 hashes
        // the text content of the <script> element.
        const digest = createHash("sha256").update(fouc).digest("base64");
        if (!STANDARD_BASE64.test(digest)) {
          throw new Error(
            `reverie-csp-hash: digest '${digest}' is not RFC 4648 §4 standard base64 (CSP L3 requires standard alphabet with padding)`,
          );
        }
        const sriValue = `sha256-${digest}`;

        if (resolvedConfig.command === "build") {
          const outDir = resolvedConfig.build.outDir;
          const sidecarPath = resolve(resolvedConfig.root, outDir, SIDECAR_FILENAME);
          mkdirSync(dirname(sidecarPath), { recursive: true });
          writeFileSync(
            sidecarPath,
            JSON.stringify({ "script-src-hashes": [sriValue] }, null, 2) + "\n",
            "utf8",
          );
        }
        return injectedHtml;
      },
    },
  };
}
