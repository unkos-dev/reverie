import path from "node:path";
import { defineConfig, lazyPlugins, type PluginOption } from "vite-plus";
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
  // vp lazyPlugins returns Plugin<any>[] which TS cannot reconcile with the
  // PluginOption[] vite-plus expects across the rolldown-vite type graph
  // (vitejs/vite#20948); the assertion is the documented vp-recommended fix.
  plugins: lazyPlugins(() => [react(), tailwindcss(), cspHashPlugin()] as PluginOption[]),
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  build: {
    // react-data-grid's color tokens are all light-dark() functions; the
    // rolldown-vite default CSS minifier (lightningcss) miscompiles
    // light-dark() (parcel-bundler/lightningcss#873), silently corrupting the
    // grid's dark palette in production builds. esbuild minifies it correctly.
    cssMinify: "esbuild",
    rollupOptions: {
      output: {
        // Route the dev-only design tree into its own named chunk. main.tsx
        // gates the import behind `if (import.meta.env.DEV)`; in production
        // `import.meta.env.DEV` is replaced with literal `false`, the
        // dynamic-import branch becomes dead code, rolldown tree-shakes it,
        // and no `design-*.js` is emitted into `dist/assets/`. The named
        // group makes any leak surface as a `design-*.js` chunk, which
        // scripts/assert-no-design-chunk.mjs fails the build on (substring-
        // grepping minified output is unreliable, so the gate checks for the
        // chunk file's structural presence instead). The `test` mirrors the
        // prior manualChunks predicate: `routes/design` with no trailing
        // slash matches design.tsx and the directory; `pages/design/` is
        // directory-only.
        codeSplitting: {
          groups: [{ name: "design", test: /\/src\/(routes\/design|pages\/design\/)/ }],
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
    // assigned hostname (see dev/README.md).
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
    // LCOV reporter writes coverage/lcov.info, which CI uploads to
    // SonarQube Cloud.
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      // scripts/a11y/*.mjs = allowlist.mjs, the only logic-bearing gate
      // surface. The Playwright spec (a11y.spec.ts) runs under Playwright, not
      // vitest, so it is deliberately outside the coverage set.
      include: ["src/**", "vite-plugins/**", "scripts/a11y/*.mjs"],
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
        // a11y gate logic (scripts/a11y/) is plain ESM tooling, not app code,
        // so it lives outside src/ and runs in a node env like vite-plugins.
        // The pure allowlist/verdict module is the only logic-bearing surface
        // and is unit-tested here; the Playwright spec (a11y.spec.ts) that
        // drives axe-core is exercised end-to-end via `npm run a11y`.
        extends: true,
        test: {
          name: "a11y",
          environment: "node",
          include: ["scripts/a11y/**/__tests__/**/*.test.mjs"],
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
          // Must stay above the async query ceiling in tests/setup.ts.
          // At the 5s default the two collide: a failing `findBy*` spends
          // its whole budget waiting, so the test times out first and the
          // report blames the test rather than naming the element that
          // never appeared.
          testTimeout: 15_000,
        },
      },
    ],
  },
});
