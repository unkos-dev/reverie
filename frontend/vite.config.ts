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
    // the dev bundle under their assigned hostname. The dev server has
    // no credentials and serves only the public OSS source bundle; the
    // theoretical DNS-rebinding read of dev assets is the same content
    // already on GitHub. Production is unaffected — Vite is dev-only.
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
