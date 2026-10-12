import { NuqsAdapter } from "nuqs/adapters/react-router/v8";
import type { ReactElement } from "react";
import { Outlet } from "react-router";

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
 * `NuqsAdapter` wraps the shell because typed search-param state is read
 * and written below it. It must mount INSIDE the router (it consumes
 * router context), which is why it lives here rather than around
 * `RouterProvider` in `main.tsx`. Consequence worth knowing: the
 * pre-auth screens and `/account/password` are siblings of this root route,
 * not children, so they sit outside the adapter and cannot use search-param
 * state.
 */
function App(): ReactElement {
  return (
    <NuqsAdapter>
      <AppShell>
        <Outlet />
      </AppShell>
      <CommandPalette />
    </NuqsAdapter>
  );
}

export default App;
