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
  },
  test: {
    environment: "node",
    include: ["vite-plugins/**/__tests__/**/*.test.ts"],
  },
});
