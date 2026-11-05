import { RouterProvider as AriaRouterProvider } from "react-aria-components";
import { useState, ViewTransition, type ReactElement } from "react";
import { Outlet, useHref, useNavigate } from "react-router";

import { CommandPalette } from "@/components/CommandPalette";
import { AppShell } from "@/components/shell/AppShell";

/**
 * Root route component (`/`).
 *
 * Mounts the `AppShell` chrome (left rail, utility strip, admin-zone
 * tone) around the `<Outlet />` that the data-mode router fills with
 * child routes (`/library`, `/b/:id`, …), plus the command palette.
 *
 */
function App(): ReactElement {
  const navigate = useNavigate();
  const [paletteOpen, setPaletteOpen] = useState(false);
  return (
    <AriaRouterProvider
      navigate={(to, options) => {
        void navigate(to, options);
      }}
      useHref={useHref}
    >
      <ViewTransition>
        <AppShell
          onOpenSearch={() => {
            setPaletteOpen(true);
          }}
        >
          <Outlet />
        </AppShell>
      </ViewTransition>
      <CommandPalette isOpen={paletteOpen} onOpenChange={setPaletteOpen} />
    </AriaRouterProvider>
  );
}

export default App;
