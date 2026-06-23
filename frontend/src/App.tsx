import { useEffect, type ReactElement } from "react";
import { Outlet } from "react-router";

import { CommandPalette } from "@/components/CommandPalette";
import { AppShell } from "@/components/shell/AppShell";
import { useSessionRecovery } from "@/hooks/useSessionRecovery";
import { setUnauthenticatedHandler } from "@/lib/query/client";

/**
 * Root route component (`/`).
 *
 * Mounts the `AppShell` chrome (left rail, utility strip, admin-zone
 * tone) around the `<Outlet />` that the data-mode router fills with
 * child routes (`/library`, `/b/:id`, …), plus the global
 * `CommandPalette` as a sibling so its Cmd-K binding survives route
 * transitions.
 *
 * Owns one cross-cutting effect: wiring the `QueryClient`'s 401
 * handler to a full-page redirect at `/auth/login`. The backend OIDC
 * initiator lives at that path (no SPA `/login` route exists), so the
 * redirect must be a `window.location.assign(...)` — a client-side
 * `navigate()` would never hit the backend. The handler lives in the
 * query module to avoid a router import there; injecting it on mount
 * keeps the two providers decoupled (see `lib/query/client.ts`). On
 * unmount the handler is reset to a no-op so a remounted router tree
 * (e.g. during HMR) cannot navigate via a stale closure.
 *
 * It also drives session recovery via `useSessionRecovery`: when the
 * shared `/auth/me` query settles unauthenticated, that hook funnels
 * into the same `/auth/login` redirect, so a lapsed first-party session
 * recovers (silent re-auth, or the IdP login) instead of stranding the
 * user on a degraded shell.
 */
function App(): ReactElement {
  useEffect(() => {
    setUnauthenticatedHandler(() => {
      window.location.assign("/auth/login");
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

  return (
    <>
      <AppShell>
        <Outlet />
      </AppShell>
      <CommandPalette />
    </>
  );
}

export default App;
