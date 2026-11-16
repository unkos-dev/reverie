import {
  ChartNoAxesColumn,
  Download,
  House,
  Library,
  PanelsTopLeft,
  Search,
  Users,
} from "lucide-react";
import type { ReactElement } from "react";
import { useLocation } from "react-router";

import { Lockup } from "@/components/Lockup";
import { Button } from "@/components/ui/button";
import { Link } from "@/components/ui/link";
import { Tooltip, TooltipContent } from "@/components/ui/tooltip";
import { useAuthMe } from "@/hooks/useAuthMe";
import { useTheme } from "@/lib/theme/ThemeProvider";

import { PRIMARY_NAV } from "./nav-items";
import { UserChip } from "./UserMenu";
import { useShellIngestion } from "./use-shell-ingestion";

const NAV_ICONS = [House, Library, PanelsTopLeft, ChartNoAxesColumn];

export function TopBar({ onOpenSearch }: Readonly<{ onOpenSearch: () => void }>): ReactElement {
  const { effective } = useTheme();
  const { data: me } = useAuthMe();
  return (
    <>
      <header
        data-chrome=""
        className="shell-bar sticky top-0 z-30 mb-3 flex h-[52px] items-center gap-3 px-3 lg:top-3 lg:grid lg:grid-cols-[1fr_minmax(0,560px)_1fr] lg:rounded-lg lg:border lg:border-hairline lg:shadow-(--shadow-raised)"
      >
        <div className="flex shrink-0 items-center gap-3">
          <Link href="/library" aria-label="Reverie library" className="shrink-0">
            <Lockup size={16} theme={effective} />
          </Link>
          <div className="hidden lg:block">
            <PrimaryNavigation />
          </div>
        </div>
        <div className="flex min-w-0 flex-1 justify-end lg:justify-center">
          <Button
            variant="ghost"
            aria-label="Search library"
            onPress={onOpenSearch}
            className="border-hairline text-fg-muted h-9 w-10 rounded-md lg:w-full lg:justify-start lg:border lg:bg-sheet/60"
          >
            <Search aria-hidden="true" strokeWidth={1.7} className="size-4" />
            <span className="hidden truncate lg:inline">Search title, author, series or ISBN</span>
            <kbd className="bg-surface-2 ml-auto hidden rounded-sm px-1.5 py-0.5 text-xs lg:inline">
              Ctrl K
            </kbd>
          </Button>
        </div>
        <div className="flex shrink-0 items-center gap-3 lg:justify-self-end">
          {me?.role === "admin" ? <AdminNavigation /> : null}
          <div className="shrink-0 lg:border-l lg:border-hairline lg:pl-2">
            <UserChip />
          </div>
        </div>
      </header>
      <div data-chrome="" className="lg:hidden">
        <PrimaryNavigation />
      </div>
    </>
  );
}

function PrimaryNavigation(): ReactElement {
  const { pathname } = useLocation();
  return (
    <nav
      aria-label="Primary"
      className="shell-primary shell-bar fixed bottom-4 left-1/2 z-30 -translate-x-1/2 rounded-xl border border-hairline p-1.5 shadow-(--shadow-raised) lg:static lg:translate-x-0 lg:rounded-none lg:border-0 lg:bg-transparent lg:p-0 lg:shadow-none lg:backdrop-blur-none"
    >
      <ul className="flex items-center gap-1">
        {PRIMARY_NAV.map((item, index) => {
          const Icon = NAV_ICONS[index];
          const content = (
            <>
              <Icon aria-hidden="true" strokeWidth={1.7} className="size-[18px] lg:size-4" />
              {item.label}
            </>
          );
          const classes =
            "flex min-h-12 w-[min(74px,21vw)] flex-col items-center justify-center gap-0.5 rounded-lg px-2 text-xs lg:min-h-9 lg:w-auto lg:flex-row lg:gap-2 lg:rounded-sm lg:px-3 lg:text-sm";
          return (
            <li key={item.label}>
              {item.disabled === true || item.to === undefined ? (
                <span aria-disabled="true" className={`${classes} text-fg-muted`}>
                  {content}
                  <span className="text-[9px] leading-none lg:text-[10px]">Planned</span>
                </span>
              ) : (
                <Link
                  href={item.to}
                  aria-current={
                    pathname === item.to || pathname.startsWith(`${item.to}/`) ? "page" : undefined
                  }
                  className={classes}
                >
                  {content}
                </Link>
              )}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

function AdminNavigation(): ReactElement {
  const { attention, scanRunning, enrichmentRunning } = useShellIngestion();
  const activity = [
    scanRunning ? "scan running" : null,
    enrichmentRunning ? "enrichment running" : null,
  ].filter(Boolean);
  const state = [
    ...activity,
    ...(attention > 0 ? [`${String(attention)} need attention`] : []),
  ].join(", ");
  const label = state.length > 0 ? `Ingestion: ${state}` : "Ingestion";
  return (
    <nav aria-label="Admin" className="hidden items-center gap-1 lg:flex">
      <Tooltip>
        <Link href="/admin/dashboard" aria-label={label} className="relative size-9">
          <Download aria-hidden="true" strokeWidth={1.7} className="size-4" />
          {attention > 0 ? (
            <span
              aria-hidden="true"
              className="bg-accent text-fg-on-accent absolute -right-1 -top-1 min-w-4 rounded-full px-1 text-center text-[10px] tabular-nums"
            >
              {attention}
            </span>
          ) : null}
          {scanRunning || enrichmentRunning ? (
            <span
              aria-hidden="true"
              className="absolute inset-x-1 bottom-0 h-0.5 overflow-hidden rounded-full"
            >
              <span className="shell-sweep bg-accent block h-full w-1/2" />
            </span>
          ) : null}
        </Link>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <Link href="/admin/users" aria-label="Users" className="size-9">
          <Users aria-hidden="true" strokeWidth={1.7} className="size-4" />
        </Link>
        <TooltipContent>Users</TooltipContent>
      </Tooltip>
    </nav>
  );
}
