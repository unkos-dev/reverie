import { RouterProvider as AriaRouterProvider } from "react-aria-components";
import { ViewTransition, type ReactElement } from "react";
import { Outlet, useHref, useNavigate } from "react-router";

import { CommandPalette } from "@/components/CommandPalette";
import { AppShell } from "@/components/shell/AppShell";

/**
 * Root route component (`/`).
 *
 * Mounts the `AppShell` chrome (left rail, utility strip, admin-zone
 * tone) around the `<Outlet />` that the data-mode router fills with
 * child routes (`/library`, `/b/:id`, …), plus the global
 * `CommandPalette` as a sibling so its Cmd-K binding survives route
 * transitions.
 *
 */
function App(): ReactElement {
  const navigate = useNavigate();
  return (
    <AriaRouterProvider
      navigate={(to, options) => {
        void navigate(to, options);
      }}
      useHref={useHref}
    >
      <ViewTransition>
        <AppShell>
          <Outlet />
        </AppShell>
      </ViewTransition>
      <CommandPalette />
    </AriaRouterProvider>
  );
}

export default App;
