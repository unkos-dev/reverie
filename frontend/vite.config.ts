import path from "node:path";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { parseAllowedHosts } from "./vite-plugins/allowed-hosts";
import { cspHashPlugin } from "./vite-plugins/csp-hash";
import { parseHmrConfig } from "./vite-plugins/hmr-config";

// Dev-only CSP — intentionally relaxed with 'unsafe-inline' / 'unsafe-eval' so
// Vite HMR, esbuild error overlays, and Tailwind JIT work. The production CSP
// is a strict, hash-based policy served by the backend (see
// backend/src/security/csp.rs). These dev relaxations do not ship to prod.
//
// `static.cloudflareinsights.com` (script-src) and `cloudflareinsights.com`
// (connect-src) accommodate the Cloudflare Web Analytics RUM beacon that the
// edge auto-injects into HTML when a dev hostname is proxied through a CF
// zone with RUM enabled. Per-hostname Exclude rules are a Cloudflare Pro
// feature, so the cheapest fix is allowlisting the beacon origins here. The
// allowlist is dev-only and does not appear in the production CSP. See
// docs/deployment/cloudflare-edge-dev-hostnames.md.
const DEV_CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval' https://static.cloudflareinsights.com",
  "style-src 'self' 'unsafe-inline'",
  "connect-src 'self' ws://localhost:5173 ws://127.0.0.1:5173 https://cloudflareinsights.com",
  "img-src 'self' data:",
  "font-src 'self'",
].join("; ");

export default defineConfig({
  plugins: [react(), tailwindcss(), cspHashPlugin()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  build: {
    rollupOptions: {
      output: {
        // Route the dev-only design tree into its own chunk. main.tsx gates
        // the import behind `if (import.meta.env.DEV)`; in production
        // `import.meta.env.DEV` is replaced with literal `false`, the
        // dynamic-import branch becomes dead code, Vite tree-shakes the
        // chunk, and no `design-*.js` is emitted into `dist/assets/`.
        // Substring-grepping the minified output is unreliable (Vite
        // mangles names); the Level 4 gate in the plan checks for the
        // chunk file's structural absence instead.
        manualChunks(id) {
          if (id.includes("/src/routes/design") || id.includes("/src/pages/design/")) {
            return "design";
          }
        },
      },
    },
  },
  server: {
    headers: {
      "Content-Security-Policy": DEV_CSP,
    },
    // Bind on all interfaces (IPv4 + IPv6) so cloud dev environments
    // (Coder, Codespaces, Gitpod, ngrok) and same-host reverse proxies
    // can reach the dev server. Without this, Vite binds only to
    // localhost and an IPv4-side proxy hits ECONNREFUSED.
    host: true,
    // When fronted by a reverse proxy on a different external port
    // (e.g. a Cloudflare tunnel terminating TLS on 443), the browser
    // would otherwise try `wss://<host>:5173/` and fail — set
    // REVERIE_DEV_HMR_CLIENT_PORT to reconnect via the edge instead.
    // Localhost dev leaves it unset.
    ...parseHmrConfig(process.env.REVERIE_DEV_HMR_CLIENT_PORT),
    // DNS-rebinding guard active against an env-driven allowlist
    // (REVERIE_DEV_HOSTS, comma-separated). The guard rejects
    // non-loopback hostnames that are not in the allowlist; loopback
    // hosts (localhost, *.localhost, any IPv4/IPv6 literal) are
    // accepted unconditionally by Vite's hardcoded short-circuit (see
    // the comment in vite-plugins/allowed-hosts.ts). The proxy block
    // below forwards `/api`, `/auth`, and `/opds` to the backend,
    // including authenticated routes; bounding the allowlist closes
    // the DNS-rebind path that previously reached those routes when
    // the guard was disabled. Cloud dev environments (Coder,
    // Codespaces) must export REVERIE_DEV_HOSTS to match their
    // assigned hostname (see frontend/CLAUDE.md and dev/README.md).
    allowedHosts: parseAllowedHosts(process.env.REVERIE_DEV_HOSTS),
    proxy: {
      "/api": { target: "http://localhost:3000", changeOrigin: true },
      "/auth": { target: "http://localhost:3000", changeOrigin: true },
      "/opds": { target: "http://localhost:3000", changeOrigin: true },
    },
  },
  test: {
    // Coverage is configured at the root (not per-project) so a single
    // report aggregates both the vite-plugins and frontend projects. The
    // LCOV reporter writes coverage/lcov.info, which CI uploads to Codecov.
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      include: ["src/**", "vite-plugins/**"],
    },
    projects: [
      {
        extends: true,
        test: {
          name: "vite-plugins",
          environment: "node",
          include: ["vite-plugins/**/__tests__/**/*.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "frontend",
          environment: "jsdom",
          globals: true,
          setupFiles: ["./tests/setup.ts"],
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
        },
      },
    ],
  },
});
