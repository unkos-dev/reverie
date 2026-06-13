import { useEffect, type ReactElement } from "react";
import { Outlet } from "react-router";

import { CommandPalette } from "@/components/CommandPalette";
import { AppShell } from "@/components/shell/AppShell";
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
