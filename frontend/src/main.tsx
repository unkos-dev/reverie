/**
 * Frontend entrypoint.
 *
 * Mounts the React tree into `#root` and assembles the data-mode
 * router. Provider order (outer → inner) is load-bearing: `StrictMode`
 * → `QueryClientProvider` (must be mounted before any `useQuery` or
 * route loader fires) → `ThemeProvider` (sets `data-theme` on
 * `<html>` from the cookie before the route renders, paired with the
 * inline FOUC script hashed into `index.html`) → `RouterProvider` →
 * dev-only `ReactQueryDevtoolsPanel` and `<Toaster />` as siblings of
 * the router so their state survives navigation.
 *
 * Dev-only design routes (`/design/system`, `/design/hero/*`) are
 * lazy-imported behind `import.meta.env.DEV` so the route module and
 * everything reachable from `pages/design/**` is dead code in
 * production builds — Vite's `manualChunks` config in
 * `vite.config.ts` isolates the chunk and tree-shaking strips it from
 * the production bundle. The `@tanstack/react-query-devtools` import
 * lives in `lib/query/devtools.tsx` and is referenced only inside a
 * `import.meta.env.DEV` branch so Rollup eliminates it in prod.
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, Navigate, RouterProvider, type RouteObject } from "react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import "./index.css";
import App from "./App.tsx";
import { ThemeProvider } from "./lib/theme/ThemeProvider.tsx";
import { Toaster } from "./components/ui/sonner.tsx";
import { queryClient } from "./lib/query/client";
import { bookRoute, libraryRoute } from "./routes/production";
import { RootErrorBoundary } from "./components/RootErrorBoundary";

const routes: RouteObject[] = [
  {
    path: "/",
    element: <App />,
    errorElement: <RootErrorBoundary />,
    children: [
      { index: true, element: <Navigate to="/library" replace /> },
      libraryRoute,
      bookRoute,
    ],
  },
];

if (import.meta.env.DEV) {
  const { designRoutes } = await import("./routes/design");
  routes.push(...designRoutes);
}

const router = createBrowserRouter(routes);

const devtools = import.meta.env.DEV
  ? await import("./lib/query/devtools").then((m) => <m.ReactQueryDevtoolsPanel />)
  : null;

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Reverie: #root element not found in document. Check index.html.");
}
createRoot(rootElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <RouterProvider router={router} />
        <Toaster />
        {devtools}
      </ThemeProvider>
    </QueryClientProvider>
  </StrictMode>,
);
