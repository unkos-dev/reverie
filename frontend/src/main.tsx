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
 * The `@tanstack/react-query-devtools` import lives in
 * `lib/query/devtools.tsx` and is referenced only inside a
 * `import.meta.env.DEV` branch so Rollup eliminates it in prod.
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import "./index.css";
import { ThemeProvider } from "./lib/theme/ThemeProvider.tsx";
import { Toaster } from "./components/ui/sonner.tsx";
import { queryClient } from "./lib/query/client";
import { routes } from "./route-table";

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
