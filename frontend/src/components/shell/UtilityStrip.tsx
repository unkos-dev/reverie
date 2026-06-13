/**
 * Sticky utility strip across the top of the content column (spec §2).
 *
 * 56px, translucent canvas + backdrop blur — the one sanctioned
 * translucency, for legibility when content scrolls beneath. Left
 * slot: breadcrumb trail on routes that declare a crumb handle
 * (`Library › <title>`); right slot: the field-shaped search
 * affordance that opens the global CommandPalette (spec S5 — one
 * search surface).
 *
 * Mounts on every route, so it must never throw: a missing or
 * throwing crumb degrades to the bare "Library" link.
 */
import type { ReactElement } from "react";
import { Link, useMatches } from "react-router";

import { openCommandPalette, searchHintLabel } from "@/lib/command-palette";

import { isCrumbHandle } from "./crumbs";

interface UtilityStripProps {
  /** Slot for the <1024px drawer menu button (AppShell owns it). */
  menuSlot?: ReactElement | null;
}

/** Sticky 56px strip: breadcrumbs left, search affordance right. */
export function UtilityStrip({ menuSlot = null }: UtilityStripProps): ReactElement {
  return (
    <header className="border-border sticky top-0 z-20 flex h-14 shrink-0 items-center justify-between gap-4 border-b bg-[color-mix(in_srgb,var(--canvas)_82%,transparent)] px-4 backdrop-blur-[10px] group-data-[zone=admin]/zone:bg-[color-mix(in_srgb,var(--canvas-2)_82%,transparent)] sm:px-6">
      <div className="flex min-w-0 items-center gap-3">
        {menuSlot}
        <Breadcrumbs />
      </div>
      <button
        type="button"
        onClick={() => {
          openCommandPalette();
        }}
        className="border-border bg-surface text-fg-muted hover:border-border-strong hover:text-fg focus-visible:ring-accent flex h-9 w-56 shrink-0 items-center justify-between gap-3 rounded-md border px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2"
      >
        <span className="truncate">Search the library…</span>
        <kbd className="text-fg-muted font-mono text-[0.65rem] tracking-wide">
          {searchHintLabel()}
        </kbd>
      </button>
    </header>
  );
}

/**
 * Breadcrumb trail from the deepest matched route carrying a crumb
 * handle. The crumb call is guarded — a throwing crumb (malformed
 * loader data) degrades to the bare "Library" link rather than taking
 * the strip down with it.
 */
function Breadcrumbs(): ReactElement | null {
  const matches = useMatches();
  const crumbMatch = [...matches].reverse().find((match) => isCrumbHandle(match.handle));
  if (crumbMatch === undefined || !isCrumbHandle(crumbMatch.handle)) return null;

  let title: string | null;
  try {
    title = crumbMatch.handle.crumb(crumbMatch.loaderData);
  } catch (error) {
    // Degrading is sanctioned; doing it silently is not — a throwing
    // crumb means a loader/crumb contract bug worth surfacing.
    console.error("[UtilityStrip] crumb function threw", error);
    title = null;
  }

  return (
    <nav aria-label="Breadcrumb" className="min-w-0">
      <ol className="text-fg-muted flex min-w-0 items-center gap-2 text-sm">
        <li>
          <Link
            to="/library"
            viewTransition
            className="hover:text-fg focus-visible:ring-accent rounded-sm transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            Library
          </Link>
        </li>
        {title !== null ? (
          <>
            <li aria-hidden="true" className="text-fg-faint">
              ›
            </li>
            <li aria-current="page" className="text-fg min-w-0 truncate">
              {title}
            </li>
          </>
        ) : null}
      </ol>
    </nav>
  );
}
