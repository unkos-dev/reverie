/**
 * Contextual filter rail for browse surfaces: the editing surface for the
 * library's filter grammar and sort stack. Quick search lives in the page
 * toolbar (`components/library/QuickSearch`) and owns its own URL key, so
 * the two surfaces share no state and need no protocol between them.
 *
 * One collapsible section per editable concern: the sort stack, the shelf
 * and series facets, the vocabulary families (authors, tags, genres,
 * moods), reading status, the text-operator columns (title, subtitle,
 * ISBN), the numeric ranges (pages, rating), and the added-date range. A
 * section with active conditions opens on mount, shows a count beside its
 * name, and carries a clear affordance; inactive sections mount collapsed
 * to keep the column scannable.
 *
 * URL state is canonical for filters, and each section writes only the keys
 * of its own slice (see `lib/hooks/use-library-filters`). Free-text and
 * numeric sections debounce their writes, which defers the URL update
 * without deferring the value they render, so they hold no draft to
 * reconcile. A clear affordance writes immediately, and that is what
 * cancels a debounced write still queued on the same keys, so a pending
 * keystroke cannot resurrect a condition the user just cleared. Nothing
 * here can disturb a sibling section, because nothing here writes a
 * sibling's keys.
 *
 * Sort is the exception: it is a per-user preference with no URL form
 * (adr/2026-08-08-library-sort-per-user-preference.md), so the sort section
 * renders and edits the resolved stack the page passes down, and its
 * gestures dispatch to the page's one sort intent handler. The table view's
 * header click / ctrl-click edits the same stack through the same handler.
 */
import { useQuery } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, X } from "lucide-react";
import { useEffect, useState, type ReactElement, type ReactNode } from "react";

import { listShelves, MAX_SORT_LEVELS, SORT_FIELDS, type Shelf, type SortLevelParam } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SORT_FIELD_LABELS } from "@/components/library/sort-summary";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { useLibraryFilters, type FilterSlice } from "@/lib/hooks/use-library-filters";
import { queryKeys } from "@/lib/query/keys";
import {
  TEXT_COLUMN_OPS,
  type FilterState,
  type RangeFilter,
  type TextFilter,
  type TextOp,
} from "@/routes/library-params";

import {
  DateRangeEditor,
  RangeFilterEditor,
  StatusEditor,
  TextFilterEditor,
  VocabEditor,
  type VocabFamily,
} from "../library/editors";

/** One selectable series in the facet (id is the URL/param value). */
export interface SeriesFacetOption {
  id: string;
  name: string;
}

interface FilterRailProps {
  /** Distinct series from the loaded pages. */
  seriesOptions: SeriesFacetOption[];
  /** The resolved effective sort stack; see the module docstring. */
  sortLevels: readonly SortLevelParam[];
  /** True when the stack is the inherited installation order, which is when
   *  the sort section offers no reset. */
  sortInherited: boolean;
  /** The page's one sort intent handler; an empty stack means reset. */
  onSortChange: (levels: readonly SortLevelParam[]) => void;
}

/** The library's filter and sort editing surface; see the module docstring. */
export function FilterRail({
  seriesOptions,
  sortLevels,
  sortInherited,
  onSortChange,
}: Readonly<FilterRailProps>): ReactElement {
  const { filters, commitSlice } = useLibraryFilters();

  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);

  /** Typed inputs debounce; every other gesture writes immediately, which
   *  is also what cancels a debounced write still queued on those keys. */
  const commitTyped = (slice: FilterSlice, patch: (current: FilterState) => FilterState): void => {
    commitSlice(slice, patch, { debounced: true });
  };

  const setCount = (set: { all: string[]; any: string[]; none: string[] }): number =>
    set.all.length + set.any.length + set.none.length;
  const textCount = (value: TextFilter): number =>
    [value.contains, value.eq, value.ne, value.empty].filter((part) => part !== undefined).length;
  const rangeCount = (value: RangeFilter): number =>
    [value.gte, value.lte, value.empty].filter((part) => part !== undefined).length;

  const vocabSection = (family: VocabFamily, title: string): ReactElement => (
    <RailSection
      title={title}
      activeCount={setCount(filters[family])}
      onClear={() => {
        commitSlice(family, (current) => ({
          ...current,
          [family]: { all: [], any: [], none: [] },
        }));
      }}
    >
      <VocabEditor
        family={family}
        draft={filters}
        setDraft={(next) => {
          // Take only this editor's family from its output: the rest of the
          // editor's state is a render snapshot and must not overwrite a
          // commit that landed since.
          commitSlice(family, (current) => ({ ...current, [family]: next[family] }));
        }}
        resolveAuthorLabel={authorNames.labelFor}
      />
    </RailSection>
  );

  return (
    <aside aria-label="Filters" className="flex flex-col gap-4 text-sm">
      <SortSection levels={sortLevels} inherited={sortInherited} onChange={onSortChange} />
      <ShelfSection
        activeShelf={filters.shelf}
        onPick={(shelf) => {
          commitSlice("shelf", (current) => ({ ...current, shelf }));
        }}
        onClear={() => {
          commitSlice("shelf", (current) => ({ ...current, shelf: undefined }));
        }}
      />
      <RailSection
        title="Series"
        activeCount={filters.series === undefined ? 0 : 1}
        onClear={() => {
          commitSlice("series", (current) => ({ ...current, series: undefined }));
        }}
      >
        <FacetList
          options={seriesOptions}
          active={filters.series}
          emptyText="No series in view."
          onPick={(series) => {
            commitSlice("series", (current) => ({ ...current, series }));
          }}
        />
      </RailSection>
      {vocabSection("authors", "Authors")}
      {vocabSection("tags", "Tags")}
      {vocabSection("genres", "Genres")}
      {vocabSection("moods", "Moods")}
      <RailSection
        title="Status"
        activeCount={filters.status.any.length + filters.status.none.length}
        onClear={() => {
          commitSlice("status", (current) => ({ ...current, status: { any: [], none: [] } }));
        }}
      >
        <StatusEditor
          value={filters.status.any}
          onChange={(any) => {
            commitSlice("status", (current) => ({
              ...current,
              status: { ...current.status, any },
            }));
          }}
        />
      </RailSection>
      <TextSection
        title="Title"
        ops={TEXT_COLUMN_OPS.title}
        value={filters.title}
        activeCount={textCount(filters.title)}
        onChange={(title) => {
          commitTyped("title", (current) => ({ ...current, title }));
        }}
        onClear={() => {
          commitSlice("title", (current) => ({ ...current, title: {} }));
        }}
      />
      <TextSection
        title="Subtitle"
        ops={TEXT_COLUMN_OPS.subtitle}
        value={filters.subtitle}
        activeCount={textCount(filters.subtitle)}
        onChange={(subtitle) => {
          commitTyped("subtitle", (current) => ({ ...current, subtitle }));
        }}
        onClear={() => {
          commitSlice("subtitle", (current) => ({ ...current, subtitle: {} }));
        }}
      />
      <TextSection
        title="ISBN"
        ops={TEXT_COLUMN_OPS.isbn_13}
        value={filters.isbn13}
        activeCount={textCount(filters.isbn13)}
        onChange={(isbn13) => {
          commitTyped("isbn13", (current) => ({ ...current, isbn13 }));
        }}
        onClear={() => {
          commitSlice("isbn13", (current) => ({ ...current, isbn13: {} }));
        }}
      />
      <RangeSection
        title="Pages"
        min={0}
        value={filters.pages}
        activeCount={rangeCount(filters.pages)}
        onChange={(pages) => {
          commitTyped("pages", (current) => ({ ...current, pages }));
        }}
        onClear={() => {
          commitSlice("pages", (current) => ({ ...current, pages: {} }));
        }}
      />
      <RangeSection
        title="Rating"
        min={1}
        max={5}
        value={filters.rating}
        activeCount={rangeCount(filters.rating)}
        onChange={(rating) => {
          commitTyped("rating", (current) => ({ ...current, rating }));
        }}
        onClear={() => {
          commitSlice("rating", (current) => ({ ...current, rating: {} }));
        }}
      />
      <RailSection
        title="Added"
        activeCount={
          (filters.addedAfter === undefined ? 0 : 1) + (filters.addedBefore === undefined ? 0 : 1)
        }
        onClear={() => {
          commitSlice("added", (current) => ({
            ...current,
            addedAfter: undefined,
            addedBefore: undefined,
          }));
        }}
      >
        <DateRangeEditor
          after={filters.addedAfter}
          before={filters.addedBefore}
          onChange={({ after, before }) => {
            commitSlice("added", (current) => ({
              ...current,
              addedAfter: after,
              addedBefore: before,
            }));
          }}
        />
      </RailSection>
    </aside>
  );
}

type RailSectionProps = {
  title: string;
  /** Number of active conditions; positive renders the badge + clear control. */
  activeCount: number;
  onClear: () => void;
  /** Label for the clear control; the sort section's clear is a reset. */
  clearLabel?: string;
  /** Accessible name for the clear control when the visible label needs
   *  more context than `Clear <title> filters` would give. */
  clearAriaLabel?: string;
  children: ReactNode;
};

/**
 * One collapsible rail section. Open state is uncontrolled after mount (the
 * initial value is captured once), so a filter edit re-rendering the rail
 * never snaps a section the user toggled; a section that mounts with active
 * conditions starts open.
 */
function RailSection({
  title,
  activeCount,
  onClear,
  clearLabel = "Clear",
  clearAriaLabel,
  children,
}: Readonly<RailSectionProps>): ReactElement {
  const [initialOpen] = useState(activeCount > 0);
  const active = activeCount > 0;
  return (
    <details open={initialOpen}>
      <summary
        className={`flex cursor-pointer select-none items-center gap-2 font-mono text-xs uppercase tracking-[0.14em] ${
          active ? "text-accent" : "text-fg-muted"
        }`}
      >
        <span>{title}</span>
        {active ? (
          <span className="bg-accent-soft text-fg rounded-full px-1.5 py-0.5 text-[0.65rem] leading-none">
            {activeCount}
          </span>
        ) : null}
        {active ? (
          <button
            type="button"
            aria-label={clearAriaLabel ?? `Clear ${title} filters`}
            onClick={(event) => {
              // A summary click toggles the disclosure; clearing must not.
              event.preventDefault();
              event.stopPropagation();
              onClear();
            }}
            className="text-fg-muted hover:text-fg focus-visible:ring-accent ml-auto flex min-h-6 items-center rounded-sm px-2 font-mono text-xs uppercase tracking-wide transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            {clearLabel}
          </button>
        ) : null}
      </summary>
      <div className="mt-2">{children}</div>
    </details>
  );
}

type SortSectionProps = {
  levels: readonly SortLevelParam[];
  /** True when the stack is the inherited installation order. */
  inherited: boolean;
  onChange: (levels: readonly SortLevelParam[]) => void;
};

/**
 * Sort-stack editor: add, remove, reorder, and flip levels. The table view's
 * header click / ctrl-click writes the same stack; this section is the sort
 * home for grid and list and the full-stack editor everywhere. Stack
 * announcements are not this section's job: the aria-live sort summary is
 * mounted by `LibraryPage`, because the rail unmounts when collapsed and a
 * live region must stay mounted to announce.
 *
 * The stack shown is always the effective order, because the library is
 * never unordered. When it is the inherited installation stack, the section
 * reads as inactive and offers no reset: there is nothing to reset. Editing
 * an inherited level materialises it as the reader's own override, and
 * removing the last level or pressing Reset drops the override, visibly
 * returning the stack to the installation order.
 */
function SortSection({ levels, inherited, onChange }: Readonly<SortSectionProps>): ReactElement {
  const remaining = SORT_FIELDS.filter((field) => !levels.some((level) => level.field === field));

  function toggleDirection(index: number): void {
    onChange(levels.map((level, i) => (i === index ? { ...level, desc: !level.desc } : level)));
  }

  function remove(index: number): void {
    onChange(levels.filter((_, i) => i !== index));
  }

  function moveUp(index: number): void {
    if (index === 0) return;
    const next = [...levels];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    onChange(next);
  }

  function moveDown(index: number): void {
    if (index === levels.length - 1) return;
    const next = [...levels];
    [next[index + 1], next[index]] = [next[index], next[index + 1]];
    onChange(next);
  }

  return (
    <RailSection
      title="Sort"
      // The inherited installation stack is not an active condition: it
      // shows no badge and offers no reset, so the section reads as "at
      // rest" exactly when there is nothing of the reader's to remove.
      activeCount={inherited ? 0 : levels.length}
      clearLabel="Reset"
      clearAriaLabel="Reset sort to the installation order"
      onClear={() => {
        onChange([]);
      }}
    >
      <fieldset aria-label="Sort order" className="flex flex-col gap-1">
        {levels.map((level, index) => {
          const label = SORT_FIELD_LABELS[level.field];
          return (
            <div
              key={level.field}
              className="border-border bg-surface-1 flex items-center gap-0.5 rounded-md border py-1 pl-2 pr-1 text-xs"
            >
              <span className="text-fg-muted font-mono">{index + 1}</span>
              <span className="text-fg mr-auto pl-1.5 font-medium">{label}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="min-h-6 min-w-6"
                aria-label={`Change ${label} sort direction (currently ${level.desc ? "descending" : "ascending"})`}
                onClick={() => {
                  toggleDirection(index);
                }}
              >
                {level.desc ? <ArrowDown aria-hidden="true" /> : <ArrowUp aria-hidden="true" />}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="min-h-6 min-w-6"
                aria-label={`Move ${label} earlier in sort priority`}
                disabled={index === 0}
                onClick={() => {
                  moveUp(index);
                }}
              >
                <ChevronUp aria-hidden="true" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="min-h-6 min-w-6"
                aria-label={`Move ${label} later in sort priority`}
                disabled={index === levels.length - 1}
                onClick={() => {
                  moveDown(index);
                }}
              >
                <ChevronDown aria-hidden="true" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="min-h-6 min-w-6"
                aria-label={`Remove ${label} from sort`}
                onClick={() => {
                  remove(index);
                }}
              >
                <X aria-hidden="true" />
              </Button>
            </div>
          );
        })}
      </fieldset>
      {levels.length < MAX_SORT_LEVELS && remaining.length > 0 ? (
        <div className="mt-2">
          <Select
            value=""
            onValueChange={(field) => {
              const match = remaining.find((candidate) => candidate === field);
              if (match !== undefined) onChange([...levels, { field: match, desc: false }]);
            }}
          >
            <SelectTrigger className="h-8 w-full" aria-label="Add sort field">
              <SelectValue placeholder="Add sort field…" />
            </SelectTrigger>
            <SelectContent>
              {remaining.map((field) => (
                <SelectItem key={field} value={field}>
                  {SORT_FIELD_LABELS[field]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : null}
    </RailSection>
  );
}

type FacetListProps = {
  options: readonly { id: string; name: string }[];
  active: string | undefined;
  emptyText: string;
  onPick: (id: string | undefined) => void;
};

/** Single-select checkbox facet: re-picking the active value clears it (the
 *  URL grammar carries one value; the checkbox previews future multi-select). */
function FacetList({ options, active, emptyText, onPick }: Readonly<FacetListProps>): ReactElement {
  return (
    <div className="flex flex-col gap-0.5">
      {options.map((option) => (
        <label
          key={option.id}
          className="text-fg-muted hover:bg-surface hover:text-fg has-checked:bg-surface has-checked:text-fg flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 transition-colors"
        >
          <input
            type="checkbox"
            value={option.id}
            checked={active === option.id}
            onChange={() => {
              onPick(active === option.id ? undefined : option.id);
            }}
            className="accent-accent focus-visible:ring-accent size-3.5 focus-visible:outline-none focus-visible:ring-2"
          />
          <span className="truncate">{option.name}</span>
        </label>
      ))}
      {options.length === 0 ? <p className="text-fg-faint px-2 py-1.5">{emptyText}</p> : null}
    </div>
  );
}

type ShelfSectionProps = {
  activeShelf: string | undefined;
  onPick: (shelf: string | undefined) => void;
  /** Clear-affordance path, distinct from `onPick` so it resets drafts. */
  onClear: () => void;
};

function ShelfSection({ activeShelf, onPick, onClear }: Readonly<ShelfSectionProps>): ReactElement {
  const {
    data: shelves,
    isLoading,
    isError,
    error,
  } = useQuery<Shelf[]>({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });
  // The degraded copy below is acceptable UI; the failure behind it must
  // still reach the console (QueryCache.onError only routes 401s).
  useEffect(() => {
    if (isError) console.error("[FilterRail] shelves fetch failed", error);
  }, [isError, error]);

  const options = (shelves ?? []).map((shelf) => ({ id: shelf.id, name: shelf.name }));
  let emptyText = "No shelves yet.";
  if (isLoading) emptyText = "Loading shelves…";
  else if (isError) emptyText = "Couldn't load shelves.";

  return (
    <RailSection title="Shelf" activeCount={activeShelf === undefined ? 0 : 1} onClear={onClear}>
      <FacetList options={options} active={activeShelf} emptyText={emptyText} onPick={onPick} />
    </RailSection>
  );
}

type TextSectionProps = {
  title: string;
  ops: readonly TextOp[];
  value: TextFilter;
  activeCount: number;
  onChange: (next: TextFilter) => void;
  /** Clear affordance, undebounced, so it cancels a pending typed write. */
  onClear: () => void;
};

/** The editor renders the slice's own state directly: there is no draft to
 *  hold, because a debounced write updates that state at once and defers
 *  only the URL. */
function TextSection({
  title,
  ops,
  value,
  activeCount,
  onChange,
  onClear,
}: Readonly<TextSectionProps>): ReactElement {
  return (
    <RailSection title={title} activeCount={activeCount} onClear={onClear}>
      <TextFilterEditor value={value} ops={ops} onChange={onChange} />
    </RailSection>
  );
}

type RangeSectionProps = {
  title: string;
  min?: number;
  max?: number;
  value: RangeFilter;
  activeCount: number;
  onChange: (next: RangeFilter) => void;
  /** Clear affordance, undebounced, so it cancels a pending typed write. */
  onClear: () => void;
};

function RangeSection({
  title,
  min,
  max,
  value,
  activeCount,
  onChange,
  onClear,
}: Readonly<RangeSectionProps>): ReactElement {
  return (
    <RailSection title={title} activeCount={activeCount} onClear={onClear}>
      <RangeFilterEditor value={value} min={min} max={max} allowEmpty onChange={onChange} />
    </RailSection>
  );
}
