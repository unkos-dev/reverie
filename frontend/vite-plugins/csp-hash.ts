import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import type { Plugin, ResolvedConfig } from "vite";

const MARKER = "<!-- reverie:fouc-hash -->";
const FOUC_SOURCE = "src/fouc/fouc.js";
const SIDECAR_FILENAME = "csp-hashes.json";
// Standard base64 alphabet (RFC 4648 §4, with padding). CSP rejects base64url.
const STANDARD_BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;

/**
 * Custom Vite plugin for UNK-106. Injects the `src/fouc/fouc.js` contents as
 * an inline `<script>` where `index.html` has `<!-- reverie:fouc-hash -->`,
 * and (on `vite build` only) writes `dist/csp-hashes.json` with the sha256
 * hash of the inline body.
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

        // Injection-safety guard: inline <script> is terminated by </script>
        // (case-insensitive) and the browser treats the first match as the
        // script-tag close. Content that includes this literal would escape
        // the script element and render as HTML.
        if (/<\/script>/i.test(fouc)) {
          throw new Error(
            `reverie-csp-hash: ${FOUC_SOURCE} contains '</script>' — inline script injection would break the HTML.`,
          );
        }

        // Marker presence + uniqueness.
        const markerRegex = /<!-- reverie:fouc-hash -->/g;
        const markerCount = (html.match(markerRegex) ?? []).length;
        if (markerCount !== 1) {
          throw new Error(
            `reverie-csp-hash: expected exactly one '${MARKER}' in index.html, found ${markerCount}`,
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
