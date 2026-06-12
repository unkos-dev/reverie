/**
 * App shell — dual-rail chrome around the routed page (spec §2).
 *
 * Desktop (≥1024px): sticky 228px left rail + content column with the
 * utility strip on top. Below 1024px the rail collapses into a
 * focus-trapped left drawer behind a menu button in the strip, and the
 * lockup moves into the strip — feature parity at every size, no
 * desktop-only paths (spec §7).
 *
 * Admin routes (`handle: { zone: "admin" }`) flip the content column
 * and strip to `canvas-2` — the "rooms of the house" tone shift
 * (philosophy §4). The flag rides on `data-zone` so the strip and main
 * read it via a Tailwind group variant instead of prop drilling.
 *
 * The right filter rail is page-owned (`BrowseLayout`), NOT shell
 * chrome (spec S2) — the shell knows nothing about it.
 */
import { Menu } from "lucide-react";
import { useState, type ReactElement, type ReactNode } from "react";
import { useMatches } from "react-router";

import { Lockup } from "@/components/Lockup";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { useTheme } from "@/lib/theme/ThemeProvider";

import { LeftRail } from "./LeftRail";
import { UtilityStrip } from "./UtilityStrip";

interface AppShellProps {
  children: ReactNode;
}

/** Shell chrome: skip link, left rail (or drawer), strip, main column. */
export function AppShell({ children }: AppShellProps): ReactElement {
  const matches = useMatches();
  const isAdminZone = matches.some((match) => hasAdminZone(match.handle));

  return (
    <div
      className="group/zone bg-canvas text-fg flex min-h-screen"
      {...(isAdminZone ? { "data-zone": "admin" } : {})}
    >
      <a
        href="#main"
        className="focus:bg-surface focus:text-fg focus:ring-accent sr-only z-50 focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:rounded-md focus:px-3 focus:py-2 focus:ring-2"
      >
        Skip to content
      </a>
      <div className="sticky top-0 hidden h-screen w-[228px] shrink-0 lg:block">
        <LeftRail />
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <UtilityStrip menuSlot={<DrawerSlot />} />
        <main id="main" className="group-data-[zone=admin]/zone:bg-canvas-2 flex-1">
          {children}
        </main>
      </div>
    </div>
  );
}

/**
 * Strip-left slot for viewports under 1024px: menu button opening the
 * rail drawer, plus the lockup (the rail that normally carries it is
 * hidden). Clicking any nav link inside the drawer closes it so the
 * chosen destination isn't left covered — event-driven rather than a
 * pathname effect (set-state-in-effect causes cascading renders).
 */
function DrawerSlot(): ReactElement {
  const [open, setOpen] = useState(false);
  const { effective } = useTheme();

  return (
    <div className="flex items-center gap-3 lg:hidden">
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetTrigger asChild>
          <button
            type="button"
            className="hover:bg-surface focus-visible:ring-accent flex size-9 items-center justify-center rounded-md transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            <Menu className="size-5" aria-hidden="true" />
            <span className="sr-only">Open navigation</span>
          </button>
        </SheetTrigger>
        <SheetContent
          side="left"
          className="w-[260px] max-w-[80vw] gap-0 p-0"
          onClick={(event) => {
            if (event.target instanceof Element && event.target.closest("a") !== null) {
              setOpen(false);
            }
          }}
        >
          <SheetHeader className="sr-only">
            <SheetTitle>Navigation</SheetTitle>
            <SheetDescription>Primary navigation drawer.</SheetDescription>
          </SheetHeader>
          <LeftRail />
        </SheetContent>
      </Sheet>
      <Lockup size={13} theme={effective} />
    </div>
  );
}

/** Narrow a route `handle` to the admin-zone flag. */
function hasAdminZone(handle: unknown): boolean {
  return (
    typeof handle === "object" && handle !== null && "zone" in handle && handle.zone === "admin"
  );
}
