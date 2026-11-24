import { RouterProvider as AriaRouterProvider } from "react-aria-components";
import { useState, type ReactElement } from "react";
import { Outlet, useHref, useNavigate } from "react-router";
import { RelayEnvironmentProvider } from "react-relay";

import { CommandPalette } from "@/components/CommandPalette";
import { AppShell } from "@/components/shell/AppShell";
import { useAuthMe } from "@/hooks/useAuthMe";
import { relayEnvironment } from "@/lib/relay/environment";

/**
 * Root route component (`/`).
 *
 * Mounts the `AppShell` chrome (floating navigation and content sheet) around the `<Outlet />` that the data-mode router fills with
 * child routes (`/library`, `/b/:id`, …), plus the command palette.
 *
 */
function App(): ReactElement | null {
  const navigate = useNavigate();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const { data: account } = useAuthMe();
  if (account === undefined) return null;
  return (
    <RelayEnvironmentProvider key={account.id} environment={relayEnvironment()}>
      <AriaRouterProvider
        navigate={(to, options) => {
          void navigate(to, options);
        }}
        useHref={useHref}
      >
        <AppShell
          onOpenSearch={() => {
            setPaletteOpen(true);
          }}
        >
          <Outlet />
        </AppShell>
        <CommandPalette isOpen={paletteOpen} onOpenChange={setPaletteOpen} />
      </AriaRouterProvider>
    </RelayEnvironmentProvider>
  );
}

export default App;
