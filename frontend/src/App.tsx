import { useEffect, type ReactElement } from "react";
import { Outlet } from "react-router";

import { CommandPalette } from "@/components/CommandPalette";
import { setUnauthenticatedHandler } from "@/lib/query/client";

/**
 * Root route component (`/`).
 *
 * Renders the application shell that the data-mode router mounts
 * child routes (`/library`, `/b/:id`, …) into via `<Outlet />`.
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
    <main className="bg-canvas text-fg min-h-screen">
      <Outlet />
      <CommandPalette />
    </main>
  );
}

export default App;
