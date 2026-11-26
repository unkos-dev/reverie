/**
 * The library's view-neutral working toolbar: quick search, the view
 * switcher, and the Filters trigger in every mode; the density toggle and
 * Columns popover in table mode only, where they have something to drive.
 *
 * The toolbar owns no state: quick search dispatches to the page owner, the view
 * switcher and Filters trigger dispatch to the container, and display
 * preferences arrive as props.
 */
import { AlignJustify, Rows3, LayoutGrid, SlidersHorizontal, Table2 } from "lucide-react";
import type { ReactElement, ReactNode, RefObject } from "react";
import { ToggleButton, ToggleButtonGroup } from "@/components/ui/toggle-button-group";

import { DENSITIES, SORT_FIELDS, type SortLevelParam, type Density } from "@/api";
import { QuickSearch } from "@/components/library/QuickSearch";
import { Button, buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { LibraryView } from "@/routes/library-params";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { graphql, useFragment } from "react-relay";
import type { LibraryToolbar_summary$key } from "@/lib/relay/__generated__/LibraryToolbar_summary.graphql";
import type { LibraryToolbar_facets$key } from "@/lib/relay/__generated__/LibraryToolbar_facets.graphql";
import { Slider } from "@/components/ui/slider";
import { SORT_FIELD_LABELS } from "@/components/library/sort-summary";

import { ColumnsPopover } from "./ColumnsPopover";

type LibraryToolbarProps = {
  filters: ReactNode;
  editControls: ReactNode;
  summary: LibraryToolbar_summary$key | undefined;
  facets: LibraryToolbar_facets$key | undefined;
  filtered: boolean;
  query: string;
  onQueryChange: (value: string) => void;
  view: LibraryView;
  onViewChange: (view: LibraryView) => void;
  /** Filters drawer trigger state. */
  filtersOpen: boolean;
  filterButtonRef: RefObject<HTMLButtonElement | null>;
  coverWidth: number;
  onCoverWidthChange: (width: number) => void;
  onCoverWidthChangeEnd: (width: number) => void;
  sortLevels: readonly SortLevelParam[];
  onSortChange: (levels: readonly SortLevelParam[]) => void;
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

const summaryFragment = graphql`
  fragment LibraryToolbar_summary on LibrarySummary @throwOnFieldError {
    totalCount
  }
`;

const facetsFragment = graphql`
  fragment LibraryToolbar_facets on LibraryFacets @throwOnFieldError {
    matchingCount
  }
`;

const DENSITY_LABELS: Record<Density, string> = {
  comfortable: "Comfortable",
  compact: "Compact",
};

export function LibraryToolbar({
  filters,
  editControls,
  summary,
  facets,
  filtered,
  query,
  onQueryChange,
  view,
  onViewChange,
  filtersOpen,
  filterButtonRef,
  coverWidth,
  onCoverWidthChange,
  onCoverWidthChangeEnd,
  sortLevels,
  onSortChange,
  activeFilterCount,
  onOpenFilters,
  density,
  onDensityChange,
  hiddenColumns,
  onToggleColumn,
  canResetColumns,
  onResetColumns,
}: Readonly<LibraryToolbarProps>): ReactElement {
  const summaryData = useFragment(summaryFragment, summary ?? null);
  const facetData = useFragment(facetsFragment, facets ?? null);
  const firstSort = sortLevels.at(0);
  const sortDirection = firstSort?.desc === true ? "desc" : "asc";
  const sortValue = firstSort === undefined ? null : `${firstSort.field}:${sortDirection}`;
  return (
    <div data-chrome="" className="mb-2 flex flex-wrap items-center gap-2 py-2">
      <div className="min-w-60 max-w-60 flex-1">
        <QuickSearch value={query} onChange={onQueryChange} />
      </div>
      {filters}
      {summaryData != null && (!filtered || facetData != null) ? (
        <span className="text-fg-muted text-xs tabular-nums">
          {filtered && facetData != null ? `${String(facetData.matchingCount)} of ` : ""}
          {summaryData.totalCount} books
        </span>
      ) : null}
      <span className="flex-1" />
      {editControls}
      <div className="flex flex-wrap items-center gap-2">
        <ToggleButtonGroup
          aria-label="View mode"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[view]}
          onSelectionChange={(keys) => {
            const key = keys.values().next().value;
            if (key === "grid" || key === "table") onViewChange(key);
          }}
          className="border-border bg-surface inline-flex items-center rounded-md border p-1"
        >
          <ToggleButton
            id="table"
            aria-label="Table"
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
            )}
          >
            <Table2 className="size-4" aria-hidden="true" />
            <span className="hidden sm:inline">Table</span>
          </ToggleButton>
          <ToggleButton
            id="grid"
            aria-label="Covers"
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
            )}
          >
            <LayoutGrid className="size-4" aria-hidden="true" />
            <span className="hidden sm:inline">Covers</span>
          </ToggleButton>
        </ToggleButtonGroup>
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
              className="border-border bg-surface inline-flex items-center rounded-md border p-1"
            >
              {DENSITIES.map((option) => (
                <ToggleButton
                  key={option}
                  id={option}
                  className={cn(
                    buttonVariants({ variant: "ghost", size: "sm" }),
                    "data-selected:bg-accent-soft data-selected:text-fg data-selected:data-hovered:bg-accent-soft",
                  )}
                >
                  {option === "comfortable" ? (
                    <Rows3 className="size-4" aria-hidden="true" />
                  ) : (
                    <AlignJustify className="size-4" aria-hidden="true" />
                  )}
                  <span className="sr-only">{DENSITY_LABELS[option]}</span>
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
        ) : (
          <>
            <Select
              aria-label="Sort books"
              value={sortValue}
              placeholder="Sort"
              onChange={(value) => {
                for (const field of SORT_FIELDS) {
                  if (value === `${field}:asc` || value === `${field}:desc`) {
                    onSortChange([{ field, desc: value.endsWith(":desc") }]);
                    break;
                  }
                }
              }}
            >
              <SelectTrigger className="border-hairline text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SORT_FIELDS.flatMap((field) =>
                  [false, true].map((desc) => {
                    const direction = desc ? "descending" : "ascending";
                    let order = direction;
                    if (field === "created_at") order = desc ? "newest first" : "oldest first";
                    return (
                      <SelectItem
                        key={`${field}:${desc ? "desc" : "asc"}`}
                        id={`${field}:${desc ? "desc" : "asc"}`}
                      >
                        {`${SORT_FIELD_LABELS[field]}, ${order}`}
                      </SelectItem>
                    );
                  }),
                )}
              </SelectContent>
            </Select>
            <Slider
              label="Cover width"
              minValue={120}
              maxValue={300}
              step={1}
              value={coverWidth}
              onChange={onCoverWidthChange}
              onChangeEnd={onCoverWidthChangeEnd}
            />
          </>
        )}
        <Button
          ref={filterButtonRef}
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
