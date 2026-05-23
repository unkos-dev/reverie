/**
 * Application-wide TanStack Query client.
 *
 * The `QueryClient` is a module singleton — route loaders import it
 * directly for `prefetchQuery`, components share it through the
 * `QueryClientProvider` mounted in `main.tsx`, and a single shared
 * cache means a prefetch in a loader is hot on the component that
 * renders next. Per the JSON-API ADR and the
 * `feedback_industry_standard_default` memory, the auth-failure path
 * funnels through `QueryCache.onError` rather than being repeated at
 * every call site.
 *
 * The 401 redirect is wired indirectly via {@link setUnauthenticatedHandler}.
 * Importing `react-router` here would create a cycle: `main.tsx` must
 * mount the `QueryClientProvider` before the router can render, but
 * the router needs the navigate function inside the handler. The
 * `<App/>` route component calls `setUnauthenticatedHandler(navigate)`
 * once on mount, after both providers are alive.
 *
 * Retry policy: the default of `failureCount < 2` keeps transient
 * network noise from surfacing while ensuring `401 Unauthorized` and
 * `403 Forbidden` short-circuit immediately — retrying an auth
 * failure cannot resolve it and only delays the redirect.
 */
import { QueryCache, QueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api";

/**
 * Handler invoked by {@link queryClient}'s cache when a query rejects
 * with an `ApiError` whose status is 401. Stored in module-level
 * mutable state so the route layer can swap in a `useNavigate`-backed
 * implementation once the router is mounted.
 *
 * Default is a no-op — react-query may emit cache errors before the
 * router is alive (e.g. devtools-driven prefetch in tests).
 */
let unauthenticatedHandler: () => void = () => {};

/**
 * Replace the 401 handler installed on {@link queryClient}'s cache.
 *
 * Called once from the `<App/>` mount effect with
 * `() => navigate("/login")`. Subsequent calls (e.g. on route remount
 * during hot reload) replace the previous handler.
 *
 * @param fn - The new handler. Pass a no-op `() => {}` to disable.
 */
export function setUnauthenticatedHandler(fn: () => void): void {
  unauthenticatedHandler = fn;
}

/**
 * Singleton `QueryClient`. Route loaders import this directly for
 * `prefetchQuery`; `<QueryClientProvider client={queryClient}/>` makes
 * the same instance available to components via context.
 */
export const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (err) => {
      if (err instanceof ApiError && err.status === 401) {
        unauthenticatedHandler();
      }
    },
  }),
  defaultOptions: {
    queries: {
      retry: (count, err) => {
        if (err instanceof ApiError && (err.status === 401 || err.status === 403)) {
          return false;
        }
        return count < 2;
      },
      staleTime: 30_000,
    },
  },
});
