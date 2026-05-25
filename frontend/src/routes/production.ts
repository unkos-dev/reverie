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

/** `/series/:id` — series detail with ordered works + completeness indicator. */
export const seriesRoute: RouteObject = {
  path: "series/:id",
  lazy: async () => {
    const mod = await import("./series");
    return { loader: mod.loader, Component: mod.Component };
  },
};

/** `/shelves` — list of the caller's shelves with create / rename / delete. */
export const shelvesRoute: RouteObject = {
  path: "shelves",
  lazy: async () => {
    const mod = await import("./shelves");
    return { loader: mod.loader, Component: mod.Component };
  },
};

/** `/shelves/:id` — shelf detail with drag-to-reorder items. */
export const shelfDetailRoute: RouteObject = {
  path: "shelves/:id",
  lazy: async () => {
    const mod = await import("./shelf-detail");
    return { loader: mod.loader, Component: mod.Component };
  },
};

/** `/admin/users` — admin-only user management. */
export const adminUsersRoute: RouteObject = {
  path: "admin/users",
  lazy: async () => {
    const mod = await import("./admin");
    return { loader: mod.loader, Component: mod.Component };
  },
};
