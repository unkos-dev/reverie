/**
 * Drives session recovery when the shared `/auth/me` query settles
 * unauthenticated.
 *
 * `useAuthMe` swallows a 401/403 into `data: undefined` with `isError: false`
 * so the shell can render in a degraded state; on its own that strands a user
 * whose first-party session lapsed. This hook observes that settled state and
 * routes it into the same funnel an `apiFetch` 401 uses
 * ({@link invokeUnauthenticatedHandler}), which performs a full-page navigation
 * to the backend OIDC initiator at `/auth/login` — silent re-auth when the
 * upstream SSO is still valid, the IdP login otherwise.
 *
 * Mount this only inside the auth-required shell, and only after the effect that
 * wires the handler via `setUnauthenticatedHandler`: React runs effects in
 * declaration order, so wiring must commit before this hook's effect or a
 * first-render-settled query (e.g. a stale cached 401) would fire recovery
 * against the no-op handler and drop the redirect. It piggybacks on the cached
 * `/auth/me` query (no extra request) and fires no navigation while the query
 * is loading or has errored operationally.
 */
import { useEffect } from "react";

import { invokeUnauthenticatedHandler } from "@/lib/query/client";

import { useAuthMe } from "./useAuthMe";

function useSessionRecovery(): void {
  const { data, isLoading, isError } = useAuthMe();
  useEffect(() => {
    if (!isLoading && !isError && data === undefined) {
      invokeUnauthenticatedHandler();
    }
  }, [data, isLoading, isError]);
}

export { useSessionRecovery };
