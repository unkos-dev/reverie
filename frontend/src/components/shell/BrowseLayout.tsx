/**
 * Browse-surface layout: content column + page-owned filter rail
 * (spec §2 — the rail is page chrome, NOT shell chrome; spec S2).
 *
 * ≥1280px the rail sits in a sticky 264px column right of the
 * content. Below that it collapses into a "Refine" button opening a
 * right sheet carrying the same controls — feature parity at every
 * size (spec §7).
 *
 * GOTCHA (from the mockup build): no `margin-inline: auto` on the
 * content column — auto margins on a grid item kill column stretch;
 * centring belongs to the page's own max-width wrapper.
 */
import { useState, type ReactElement, type ReactNode } from "react";
import { useSearchParams } from "react-router";

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";

interface BrowseLayoutProps {
  /** The filter rail (a `FilterRail`, typically). */
  rail: ReactElement;
  children: ReactNode;
}

/**
 * Content + filter-rail grid with the <1280px Refine sheet. The sheet
 * needs no close-on-navigate handling: the layout is page-owned, so
 * leaving the page unmounts it, while same-page param writes (picking
 * a filter) keep it open by design.
 */
export function BrowseLayout({ rail, children }: BrowseLayoutProps): ReactElement {
  const [refineOpen, setRefineOpen] = useState(false);
  const [searchParams] = useSearchParams();

  return (
    <div className="xl:grid xl:grid-cols-[minmax(0,1fr)_264px]">
      <div className="min-w-0">
        <div data-chrome="" className="flex justify-end px-4 pt-4 sm:px-6 xl:hidden">
          <Sheet open={refineOpen} onOpenChange={setRefineOpen}>
            <SheetTrigger asChild>
              <button
                type="button"
                className="border-border bg-surface text-fg-muted hover:border-border-strong hover:text-fg focus-visible:ring-accent rounded-md border px-3 py-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2"
              >
                Refine
                {searchParams.get("series") !== null ? (
                  <span
                    aria-hidden="true"
                    className="bg-accent ml-2 inline-block size-1.5 rounded-full"
                  />
                ) : null}
              </button>
            </SheetTrigger>
            <SheetContent side="right" className="w-[300px] max-w-[85vw] overflow-y-auto p-4">
              <SheetHeader className="p-0">
                <SheetTitle>Filters</SheetTitle>
                <SheetDescription className="sr-only">Refine the books in view.</SheetDescription>
              </SheetHeader>
              {rail}
            </SheetContent>
          </Sheet>
        </div>
        {children}
      </div>
      <div data-chrome="" className="hidden xl:block">
        <div className="sticky top-14 max-h-[calc(100vh-3.5rem)] overflow-y-auto px-5 py-8">
          {rail}
        </div>
      </div>
    </div>
  );
}
