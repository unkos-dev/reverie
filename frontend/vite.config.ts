import path from "node:path";
import { defineConfig, lazyPlugins } from "vite-plus";
import { playwright } from "vite-plus/test/browser-playwright";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { parseAllowedHosts } from "./vite-plugins/allowed-hosts";
import { cspHashPlugin } from "./vite-plugins/csp-hash";
import { buildDevCsp } from "./vite-plugins/dev-csp";
import { parseHmrConfig } from "./vite-plugins/hmr-config";

// Dev-only CSP. The `connect-src` HMR websocket origins are derived from
// REVERIE_DEV_HOSTS so a non-loopback dev host fronted by a TLS edge is
// allowed to open its HMR socket; see vite-plugins/dev-csp.ts for the policy
// rationale and the Cloudflare RUM beacon allowances.
const DEV_CSP = buildDevCsp(process.env.REVERIE_DEV_HOSTS);

export default defineConfig({
  // fmt + lint are defined in the root vite.config.ts; this file is frontend
  // build/server/test config only.
  plugins: lazyPlugins(() => [react(), tailwindcss(), cspHashPlugin()]),
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
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
    // assigned hostname (see dev/README.md).
    allowedHosts: parseAllowedHosts(process.env.REVERIE_DEV_HOSTS),
    proxy: {
      "/api": { target: "http://localhost:3000", changeOrigin: true },
      "/auth": { target: "http://localhost:3000", changeOrigin: true },
      "/opds": { target: "http://localhost:3000", changeOrigin: true },
    },
  },
  test: {
    projects: [
      {
        extends: false,
        optimizeDeps: {
          include: [
            "@tanstack/react-query",
            "react-router",
            "react-error-boundary",
            "react-dom/client",
          ],
        },
        plugins: lazyPlugins(() => [react(), tailwindcss()]),
        resolve: { alias: { "@": path.resolve(__dirname, "src") } },
        test: {
          name: "components-chromium",
          include: ["tests/browser/**/*.test.tsx"],
          setupFiles: ["./tests/browser/setup.ts"],
          browser: {
            enabled: true,
            headless: true,
            screenshotDirectory: "./test-results/browser",
            provider: playwright({ contextOptions: { locale: "en-AU", timezoneId: "UTC" } }),
            instances: [{ browser: "chromium", viewport: { width: 1280, height: 800 } }],
          },
        },
      },
      {
        extends: true,
        test: {
          name: "vite-plugins",
          environment: "node",
          include: ["vite-plugins/**/__tests__/**/*.test.ts"],
        },
      },
      {
        // Drift gate for the generated color primitives: the committed
        // artifact must equal the emitter's output, so the two only ever
        // change together (including dependency bumps that shift color math).
        extends: true,
        test: {
          name: "radix-gen",
          environment: "node",
          include: ["scripts/radix-gen/**/__tests__/**/*.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "frontend-node",
          environment: "node",
          setupFiles: ["./tests/setup.node.ts"],
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
        },
      },
    ],
  },
});
