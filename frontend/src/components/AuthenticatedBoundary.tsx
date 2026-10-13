import { useEffect, type ReactElement } from "react";
import { Outlet } from "react-router";

import { fetchSetupStatus } from "@/api/auth";
import { useSessionRecovery } from "@/hooks/useSessionRecovery";
import { queryClient, setUnauthenticatedHandler } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

/**
 * Resolve where an unauthenticated/lapsed session should be sent.
 *
 * Provider-aware: when OIDC is enabled the redirect targets the
 * backend OIDC initiator `/auth/oidc/login`, so a session lapse against
 * a still-live upstream SSO re-authenticates silently. Otherwise it
 * targets the SPA local login form at `/login`. Provider state is read
 * from the cached `GET /auth/setup/status` (shared with the auth
 * screens); any failure to read it falls back to the always-valid local
 * form rather than stranding the user.
 */
async function resolveLoginRedirect(): Promise<string> {
  try {
    const status = await queryClient.query({
      queryKey: queryKeys.auth.setupStatus(),
      queryFn: ({ signal }) => fetchSetupStatus(signal),
      retry: false,
      staleTime: "static",
    });
    return status.oidc_enabled ? "/auth/oidc/login" : "/login";
  } catch {
    return "/login";
  }
}

/**
 * Pathless layout route wrapping every route that requires a session. It
 * renders no chrome, only the child route.
 *
 * Owns the `QueryClient`'s 401 handler: a full-page redirect to the target
 * {@link resolveLoginRedirect} picks (the OIDC initiator, else `/login`), so an
 * OIDC initiation actually hits the backend. The handler lives in the query
 * module to avoid a router import there; wiring it on mount keeps the two
 * decoupled (see `lib/query/client.ts`). On unmount it is reset to a no-op so a
 * remounted router tree (e.g. during HMR) cannot navigate via a stale closure.
 *
 * It also drives session recovery via `useSessionRecovery`: when the shared
 * `/auth/me` query settles unauthenticated, that hook funnels into the same
 * redirect, so a lapsed first-party session recovers instead of stranding the
 * user on a degraded page.
 *
 * Pre-auth screens stay outside it, or an unauthenticated visitor would loop.
 */
function AuthenticatedBoundary(): ReactElement {
  useEffect(() => {
    setUnauthenticatedHandler(() => {
      void resolveLoginRedirect().then((target) => {
        window.location.assign(target);
      });
    });
    return () => {
      setUnauthenticatedHandler(() => {});
    };
  }, []);

  // Load-bearing order: this call must stay AFTER the handler-wiring effect
  // above. React runs effects in declaration order, so the wiring effect
  // commits before useSessionRecovery's internal effect. Moving this up would
  // let recovery fire against the no-op handler on a first render that is
  // already settled (e.g. a stale cached 401 after HMR), silently dropping the
  // redirect.
  useSessionRecovery();

  return <Outlet />;
}

export { AuthenticatedBoundary };
