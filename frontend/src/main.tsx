/**
 * Frontend entrypoint.
 *
 * Mounts the React tree into `#root` and assembles the router. Dev-only
 * design routes (`/design/system`, `/design/hero/*`) are lazy-imported
 * behind `import.meta.env.DEV` so the route module and everything it
 * reaches from `pages/design/**` is dead code in production builds —
 * Vite's `manualChunks` config in `vite.config.ts` isolates the chunk
 * and tree-shaking strips it from the production bundle.
 *
 * Provider order (outer → inner) is load-bearing: `StrictMode` →
 * `ThemeProvider` (sets `data-theme` on `<html>` from the cookie before
 * the route renders, paired with the inline FOUC script hashed into
 * `index.html`) → `RouterProvider` → `Toaster`. The `Toaster` sits as
 * a sibling of the router rather than inside a route so toast state
 * survives navigation.
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider, type RouteObject } from "react-router";
import "./index.css";
import App from "./App.tsx";
import { ThemeProvider } from "./lib/theme/ThemeProvider.tsx";
import { Toaster } from "./components/ui/sonner.tsx";

const routes: RouteObject[] = [{ path: "/", element: <App /> }];

if (import.meta.env.DEV) {
  const { designRoutes } = await import("./routes/design");
  routes.push(...designRoutes);
}

const router = createBrowserRouter(routes);

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Reverie: #root element not found in document. Check index.html.");
}
createRoot(rootElement).render(
  <StrictMode>
    <ThemeProvider>
      <RouterProvider router={router} />
      <Toaster />
    </ThemeProvider>
  </StrictMode>,
);
