import { useKeyboard } from "react-aria";
import type { ReactElement, ReactNode } from "react";

import { TopBar } from "./TopBar";
import "./shell.css";

type AppShellProps = {
  children: ReactNode;
  onOpenSearch: () => void;
};

export function AppShell({ children, onOpenSearch }: Readonly<AppShellProps>): ReactElement {
  const { keyboardProps } = useKeyboard({
    shortcuts: { "Control+k": onOpenSearch, "Meta+k": onOpenSearch },
  });

  return (
    <div {...keyboardProps} className="bg-shell text-fg min-h-dvh p-0 pb-28 lg:p-3">
      <a
        href="#main"
        className="focus:bg-sheet focus:text-fg focus:ring-accent sr-only z-50 focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:rounded-md focus:px-3 focus:py-2 focus:ring-2"
      >
        Skip to content
      </a>
      <TopBar onOpenSearch={onOpenSearch} />
      <main
        id="main"
        tabIndex={-1}
        className="bg-sheet border-hairline min-h-[calc(100dvh-84px)] rounded-xl border outline-none"
      >
        {children}
      </main>
    </div>
  );
}

export type AppZone = "admin";

export type ZoneHandle = {
  zone: AppZone;
};
