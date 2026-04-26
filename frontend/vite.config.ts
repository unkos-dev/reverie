import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { cspHashPlugin } from "./vite-plugins/csp-hash";

// Dev-only CSP — intentionally relaxed with 'unsafe-inline' / 'unsafe-eval' so
// Vite HMR, esbuild error overlays, and Tailwind JIT work. The production CSP
// is a strict, hash-based policy served by the backend (see
// backend/src/security/csp.rs). These dev relaxations do not ship to prod.
const DEV_CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval'",
  "style-src 'self' 'unsafe-inline'",
  "connect-src 'self' ws://localhost:5173 ws://127.0.0.1:5173",
  "img-src 'self' data:",
  "font-src 'self' https://cdn.fontshare.com",
].join("; ");

export default defineConfig({
  plugins: [react(), tailwindcss(), cspHashPlugin()],
  server: {
    headers: {
      "Content-Security-Policy": DEV_CSP,
    },
    // Bind on all interfaces (IPv4 + IPv6) so cloud dev environments
    // (Coder, Codespaces, Gitpod, ngrok) and same-host reverse proxies
    // can reach the dev server. Without this, Vite binds only to
    // localhost and an IPv4-side proxy hits ECONNREFUSED.
    host: true,
    // DNS-rebinding guard disabled in dev so the same proxies can serve
    // the dev bundle under their assigned hostname. This widens the
    // attack surface: the proxy block below forwards `/api`, `/auth`,
    // and `/opds` to the backend, including authenticated routes
    // (OIDC callback, token CRUD, ingestion scan, OPDS feed). With
    // allowedHosts:true, a malicious page that successfully DNS-rebinds
    // to the dev workstation can reach those backend routes through
    // the dev proxy. The risk is accepted because (a) Vite is dev-only
    // and never ships to production, (b) the attack requires the
    // developer to have a live session cookie scoped to the rebound
    // hostname, which is unusual in dev workflows, and (c) cloud dev
    // environments (Coder, Codespaces) generate workspace-specific
    // hostnames that are impractical to enumerate in a static
    // allowlist. If you tighten this later, narrow allowedHosts to
    // an env-driven allowlist (e.g. REVERIE_DEV_HOSTS) rather than
    // restricting the proxy.
    allowedHosts: true,
    proxy: {
      "/api": { target: "http://localhost:3000", changeOrigin: true },
      "/auth": { target: "http://localhost:3000", changeOrigin: true },
      "/opds": { target: "http://localhost:3000", changeOrigin: true },
    },
  },
  test: {
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
