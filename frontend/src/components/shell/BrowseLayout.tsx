/**
 * Browse-surface layout: content column + page-owned filter rail
 * (spec §2 — the rail is page chrome, NOT shell chrome; spec S2).
 *
 * ≥1280px the rail sits in a sticky 264px column right of the content,
 * and the column can fully collapse (`railCollapsed`), handing its width
 * back to the content. Below 1280px the rail rides a right sheet. Both
 * visibility states are controlled by the owning page: the masthead's
 * filter toggle is the single show/hide affordance at every width, so
 * this layout renders no trigger of its own.
 *
 * GOTCHA (from the mockup build): no `margin-inline: auto` on the
 * content column — auto margins on a grid item kill column stretch;
 * centring belongs to the page's own max-width wrapper.
 */
import type { ReactElement, ReactNode } from "react";

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";

/**
 * Media query for the rail-column breakpoint. Must describe the same width
 * as the `xl:` utilities on the layout below (Tailwind's default `xl` is
 * 1280px): the owning page routes its toggle between the persisted column
 * collapse and the transient sheet by this query, so if the two ever
 * disagree the toggle drives a surface the viewer cannot see.
 */
export const RAIL_DESKTOP_MEDIA_QUERY = "(min-width: 1280px)";

interface BrowseLayoutProps {
  /** The filter rail (a `FilterRail`, typically). */
  rail: ReactElement;
  children: ReactNode;
  /** Hides the ≥1280px rail column; the sheet path is unaffected. */
  railCollapsed: boolean;
  /** Controls the <1280px rail sheet. */
  sheetOpen: boolean;
  onSheetOpenChange: (open: boolean) => void;
}

/**
 * Content + filter-rail grid with the <1280px rail sheet. The sheet needs
 * no close-on-navigate handling: the layout is page-owned, so leaving the
 * page unmounts it, while same-page param writes (picking a filter) keep
 * it open by design.
 */
export function BrowseLayout({
  rail,
  children,
  railCollapsed,
  sheetOpen,
  onSheetOpenChange,
}: BrowseLayoutProps): ReactElement {
  return (
    <div
      className={
        railCollapsed
          ? "cinematic-grid"
          : "cinematic-grid xl:grid xl:grid-cols-[minmax(0,1fr)_264px]"
      }
    >
      <div className="min-w-0">
        <Sheet open={sheetOpen} onOpenChange={onSheetOpenChange}>
          <SheetContent side="right" className="w-[300px] max-w-[85vw] overflow-y-auto p-4">
            <SheetHeader className="p-0">
              <SheetTitle>Filters</SheetTitle>
              <SheetDescription className="sr-only">Refine the books in view.</SheetDescription>
            </SheetHeader>
            {rail}
          </SheetContent>
        </Sheet>
        {children}
      </div>
      {railCollapsed ? null : (
        <div data-chrome="" className="border-border hidden border-l xl:block">
          <div className="sticky top-0 max-h-screen overflow-y-auto px-5 py-8">{rail}</div>
        </div>
      )}
    </div>
  );
}
