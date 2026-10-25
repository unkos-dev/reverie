/**
 * The library's view-neutral working toolbar: quick search, the view
 * switcher, and the Filters trigger in every mode; the density toggle and
 * Columns popover in table mode only, where they have something to drive.
 *
 * The toolbar owns no state: quick search dispatches to the page owner, the view
 * switcher and Filters trigger dispatch to the container, and display
 * preferences arrive as props.
 */
import { LayoutGrid, SlidersHorizontal, Table2 } from "lucide-react";
import type { ReactElement } from "react";
import { ToggleButton, ToggleButtonGroup } from "react-aria-components";

import { DENSITIES, type Density } from "@/api";
import { QuickSearch } from "@/components/library/QuickSearch";
import { Button, buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { LibraryView } from "@/routes/library-params";

import { ColumnsPopover } from "./ColumnsPopover";

type LibraryToolbarProps = {
  query: string;
  onQueryChange: (value: string) => void;
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
  query,
  onQueryChange,
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
}: Readonly<LibraryToolbarProps>): ReactElement {
  return (
    <div data-chrome="" className="mb-4 flex flex-wrap items-center gap-2">
      <QuickSearch value={query} onChange={onQueryChange} />
      <div className="flex flex-wrap items-center gap-2">
        {view === "table" ? (
          <>
            <ToggleButtonGroup
              aria-label="Table density"
              selectionMode="single"
              disallowEmptySelection
              selectedKeys={[density]}
              onSelectionChange={(keys) => {
                const key = keys.values().next().value;
                if (key === "comfortable" || key === "compact") onDensityChange(key);
              }}
              className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
            >
              {DENSITIES.map((option) => (
                <ToggleButton
                  key={option}
                  id={option}
                  className={cn(
                    buttonVariants({ variant: "ghost", size: "sm" }),
                    "font-mono text-[0.65rem] uppercase tracking-[0.08em] data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
                  )}
                >
                  {DENSITY_LABELS[option]}
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
            <ColumnsPopover
              hiddenColumns={hiddenColumns}
              onToggleColumn={onToggleColumn}
              canReset={canResetColumns}
              onReset={onResetColumns}
            />
          </>
        ) : null}
        <ToggleButtonGroup
          aria-label="View mode"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[view]}
          onSelectionChange={(keys) => {
            const key = keys.values().next().value;
            if (key === "grid" || key === "table") onViewChange(key);
          }}
          className="border-border bg-surface-1 inline-flex items-center rounded-md border p-1"
        >
          <ToggleButton
            id="grid"
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
            )}
          >
            <LayoutGrid className="size-4" aria-hidden="true" />
            <span className="sr-only">Grid</span>
          </ToggleButton>
          <ToggleButton
            id="table"
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
            )}
          >
            <Table2 className="size-4" aria-hidden="true" />
            <span className="sr-only">Table</span>
          </ToggleButton>
        </ToggleButtonGroup>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-expanded={filtersOpen}
          aria-controls="library-filter-drawer"
          onPress={onOpenFilters}
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
