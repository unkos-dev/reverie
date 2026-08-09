/**
 * The library's view-neutral working toolbar: quick search, the view
 * switcher, and the Filters trigger in every mode; the density toggle and
 * Columns popover in table mode only, where they have something to drive.
 *
 * The toolbar owns no state: quick search owns its own URL key, the view
 * switcher and Filters trigger dispatch to the container, and display
 * preferences arrive as props.
 */
import { LayoutGrid, SlidersHorizontal, Table2 } from "lucide-react";
import type { ReactElement } from "react";

import { DENSITIES, type Density } from "@/api";
import { QuickSearch } from "@/components/library/QuickSearch";
import { Button } from "@/components/ui/button";
import type { LibraryView } from "@/routes/library-params";

import { ColumnsPopover } from "./ColumnsPopover";

type LibraryToolbarProps = {
  view: LibraryView;
  onViewChange: (view: LibraryView) => void;
  /** Filters drawer trigger state. */
  filtersOpen: boolean;
  activeFilterCount: number;
  onOpenFilters: () => void;
  /** Table-mode display controls; omitted entirely in grid mode. */
  density: Density;
  onDensityChange: (density: Density) => void;
  hiddenColumns: ReadonlySet<string>;
  onToggleColumn: (key: string, hidden: boolean) => void;
  canResetColumns: boolean;
  onResetColumns: () => void;
};

const DENSITY_LABELS: Record<Density, string> = {
  comfortable: "Comfortable",
  compact: "Compact",
};

export function LibraryToolbar({
  view,
  onViewChange,
  filtersOpen,
  activeFilterCount,
  onOpenFilters,
  density,
  onDensityChange,
  hiddenColumns,
  onToggleColumn,
  canResetColumns,
  onResetColumns,
}: LibraryToolbarProps): ReactElement {
  return (
    <div data-chrome="" className="mb-4 flex flex-wrap items-center gap-2">
      <QuickSearch />
      <div className="flex flex-wrap items-center gap-2">
        {view === "table" ? (
          <>
            <div
              role="group"
              aria-label="Table density"
              className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
            >
              {DENSITIES.map((option) => (
                <Button
                  key={option}
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-pressed={density === option}
                  onClick={() => {
                    onDensityChange(option);
                  }}
                  className={`font-mono text-[0.65rem] uppercase tracking-[0.08em] ${
                    density === option ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""
                  }`}
                >
                  {DENSITY_LABELS[option]}
                </Button>
              ))}
            </div>
            <ColumnsPopover
              hiddenColumns={hiddenColumns}
              onToggleColumn={onToggleColumn}
              canReset={canResetColumns}
              onReset={onResetColumns}
            />
          </>
        ) : null}
        <div
          role="group"
          aria-label="View mode"
          className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-pressed={view === "grid"}
            onClick={() => {
              onViewChange("grid");
            }}
            className={view === "grid" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""}
          >
            <LayoutGrid className="size-4" aria-hidden="true" />
            <span className="sr-only">Grid</span>
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-pressed={view === "table"}
            onClick={() => {
              onViewChange("table");
            }}
            className={view === "table" ? "bg-accent-soft text-fg hover:bg-accent-soft" : ""}
          >
            <Table2 className="size-4" aria-hidden="true" />
            <span className="sr-only">Table</span>
          </Button>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-expanded={filtersOpen}
          aria-controls="library-filter-drawer"
          onClick={onOpenFilters}
        >
          <SlidersHorizontal className="size-4" aria-hidden="true" />
          Filters
          {activeFilterCount > 0 ? (
            <span className="bg-accent-soft text-fg rounded-full px-1.5 py-0.5 text-[0.65rem] leading-none">
              {activeFilterCount}
            </span>
          ) : null}
        </Button>
      </div>
    </div>
  );
}
