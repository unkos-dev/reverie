import type { RouteObject } from "react-router";
import DesignSystemPage from "@/pages/design/system";

/**
 * Dev-only route tree. main.tsx imports this dynamically inside an
 * `if (import.meta.env.DEV)` block so the entire module — including
 * everything reachable from `pages/design/*` — is dead code in
 * production. The Vite `manualChunks` config in `vite.config.ts`
 * routes any module under `src/routes/design/` or `src/pages/design/`
 * into a `design` chunk; in production builds the chunk is empty
 * after tree-shaking and Vite skips emitting it altogether (verified
 * by the Level 4 structural gate in the plan).
 */
export const designRoutes: RouteObject[] = [
  { path: "/design/system", element: <DesignSystemPage /> },
];
