import path from "node:path";
import { defineConfig, lazyPlugins } from "vite-plus";
import { playwright } from "vite-plus/test/browser-playwright";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { cspHashPlugin } from "./vite-plugins/csp-hash";

// THREAT: 'unsafe-inline' and 'unsafe-eval' let Vite HMR, error overlays and
// Tailwind JIT run. This policy feeds only the dev server; production serves
// the strict hash-based policy in backend/src/security/csp.rs.
// THREAT: http://localhost:8400 is the Impeccable live helper, which serves
// the picker script and its event stream. Any local process bound to that
// port can run script in the dev page, so the origin stays out of production.
const DEV_CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval' http://localhost:8400",
  "style-src 'self' 'unsafe-inline'",
  "connect-src 'self' ws://localhost:5173 ws://127.0.0.1:5173 http://localhost:8400",
  "img-src 'self' data:",
  "font-src 'self'",
].join("; ");

export default defineConfig({
  // fmt + lint are defined in the root vite.config.ts; this file is frontend
  // build/server/test config only.
  plugins: lazyPlugins(() => [react({ compiler: true }), tailwindcss(), cspHashPlugin()]),
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  server: {
    headers: {
      "Content-Security-Policy": DEV_CSP,
    },
    // Bind on IPv4 and IPv6, so a client dialling 127.0.0.1 still connects
    // when localhost resolves to ::1.
    host: true,
    // THREAT: allowedHosts stays at Vite's default, which rejects unknown DNS
    // hostnames but accepts loopback names and IP addresses. The proxy below
    // forwards authenticated routes, so widening it would reopen DNS
    // rebinding against them.
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
        // React Aria's virtualiser reads process.env.VIRT_ON whenever NODE_ENV is "test".
        define: { "process.env.VIRT_ON": JSON.stringify("1") },
        optimizeDeps: {
          include: [
            "react-aria",
            "@tanstack/react-query",
            "react-router",
            "react-error-boundary",
            "react-dom/client",
          ],
        },
        plugins: lazyPlugins(() => [react({ compiler: true }), tailwindcss()]),
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
