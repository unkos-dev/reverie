/**
 * Lazy route declarations for the production library UI.
 *
 * Each route's `lazy` callback returns the `loader` + `Component` from
 * a sibling module under `src/routes/<name>.tsx`. The split keeps page
 * components out of the entry bundle until the user navigates to
 * them; the matching `loader` ships in the same chunk so react-router
 * can fire data loading and component loading from the same dynamic
 * import.
 *
 * Production routes mount under the App-shell route in `main.tsx`,
 * giving them the `<Outlet />` surface, the 401 redirect handler, and
 * the `ThemeProvider` / `QueryClientProvider` providers above.
 */
import type { RouteObject } from "react-router";

/** `/library` — paginated grid of books visible to the current user. */
export const libraryRoute: RouteObject = {
  path: "library",
  lazy: async () => {
    const mod = await import("./library");
    return { loader: mod.loader, Component: mod.Component };
  },
};

/** `/b/:id` — detail for one manifestation. */
export const bookRoute: RouteObject = {
  path: "b/:id",
  lazy: async () => {
    const mod = await import("./book");
    return { loader: mod.loader, Component: mod.Component };
  },
};
