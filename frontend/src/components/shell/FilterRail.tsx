/**
 * Contextual filter rail for browse surfaces: the sole editing surface for
 * the library's filter grammar and sort stack in every view mode.
 *
 * Quick search sits at the top, followed by one collapsible section per
 * editable concern: the sort stack, the shelf and series facets, the
 * vocabulary families (authors, tags, genres, moods), reading status, the
 * text-operator columns (title, subtitle, ISBN), the numeric ranges (pages,
 * rating), and the added-date range. A section with active conditions opens
 * on mount, shows a count beside its name, and carries a clear affordance;
 * inactive sections mount collapsed to keep the column scannable.
 *
 * URL state is canonical: the rail parses the search params it edits and
 * every write goes through the filter codec (or the sort helper), always
 * dropping `cursor` because a changed filter or sort invalidates the keyset
 * position. Free-text and numeric inputs edit a local draft committed after
 * a debounce; a committed filter change from anywhere else resyncs those
 * drafts, so a pending keystroke can never resurrect a cleared condition.
 * The masthead summary is the read-only counterpart of this surface; the
 * table view's header click / ctrl-click sort is the one gesture that edits
 * the same URL state from outside the rail.
 */
import { useQuery } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, X } from "lucide-react";
import { useEffect, useId, useState, type ReactElement, type ReactNode } from "react";
import { useSearchParams } from "react-router";

import {
  listShelves,
  MAX_SORT_LEVELS,
  parseSortParam,
  SORT_FIELDS,
  type Shelf,
  type SortField,
  type SortLevelParam,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { useDebouncedCommit } from "@/lib/hooks/use-debounced-commit";
import { queryKeys } from "@/lib/query/keys";
import {
  applySortToSearchParams,
  parseFilterParams,
  serializeFilterParams,
  type FilterState,
  type RangeFilter,
  type TextFilter,
} from "@/routes/library-params";

import {
  DateRangeEditor,
  RangeFilterEditor,
  StatusEditor,
  TextFilterEditor,
  VocabEditor,
  type TextOp,
  type VocabFamily,
} from "../library/editors";

/** One selectable series in the facet (id is the URL/param value). */
export interface SeriesFacetOption {
  id: string;
  name: string;
}

/** Minimum quick-search length; below it the input clears the `q` filter. */
const MIN_Q_LEN = 2;
/** Debounce before a quick-search or typed-section edit becomes a URL change. */
const FILTER_DEBOUNCE_MS = 250;

const TITLE_OPS: readonly TextOp[] = ["contains", "eq", "ne"];
const SUBTITLE_OPS: readonly TextOp[] = ["contains", "empty"];
const ISBN_OPS: readonly TextOp[] = ["contains", "eq", "empty"];

/** Sort-picker labels, matching the table view's column header names. */
const SORT_FIELD_LABELS: Record<SortField, string> = {
  title: "Title",
  author: "Authors",
  created_at: "Added",
  pages: "Pages",
};

/** A stable, order-independent serialization of every filter *except* `q`.
 *  Quick search watches this: it stays constant while the user types (and
 *  across the debounced `q` write), and changes only on an external filter
 *  edit, which is the signal to discard an uncommitted quick-search draft. */
function filterResetToken(filters: FilterState): string {
  const params = new URLSearchParams();
  serializeFilterParams(filters, params);
  params.delete("q");
  params.sort();
  return params.toString();
}

/** Order-independent serialization of the whole committed filter state; the
 *  draft-holding sections resync whenever it changes (see `useDraftSlice`). */
function fullFilterToken(filters: FilterState): string {
  const params = new URLSearchParams();
  serializeFilterParams(filters, params);
  params.sort();
  return params.toString();
}

function textSliceToken(value: TextFilter): string {
  return [value.contains, value.eq, value.ne, value.empty]
    .map((part) => (part === undefined ? "" : String(part)))
    .join("|");
}

function rangeSliceToken(value: RangeFilter): string {
  return [value.gte, value.lte, value.empty]
    .map((part) => (part === undefined ? "" : String(part)))
    .join("|");
}

/**
 * Local draft over one committed filter slice, committed after a debounce.
 * `fullToken` is the committed-state serialization: any committed change,
 * from this section's own commit or anywhere else, resyncs the draft to the
 * committed value, cancelling a pending debounce so a stale draft cannot
 * resurrect a condition the user just cleared.
 */
function useDraftSlice<T>(
  committed: T,
  fullToken: string,
  sliceToken: (value: T) => string,
  commit: (next: T) => void,
): { draft: T; setDraft: (next: T) => void } {
  const [draft, setDraft] = useState(committed);
  const [synced, setSynced] = useState(fullToken);
  // Render-phase state adjustment, the compiler-accepted alternative to a
  // sync effect.
  if (fullToken !== synced) {
    setSynced(fullToken);
    setDraft(committed);
  }
  useDebouncedCommit(
    sliceToken(draft),
    sliceToken(committed),
    () => {
      commit(draft);
    },
    FILTER_DEBOUNCE_MS,
  );
  return { draft, setDraft };
}

interface FilterRailProps {
  /** Distinct series from the loaded pages. */
  seriesOptions: SeriesFacetOption[];
}

/** The library's filter and sort editing surface; see the module docstring. */
export function FilterRail({ seriesOptions }: FilterRailProps): ReactElement {
  const [searchParams, setSearchParams] = useSearchParams();
  const filters = parseFilterParams(searchParams);
  const sortLevels = parseSortParam(searchParams.get("sort") ?? "");
  const fullToken = fullFilterToken(filters);

  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);

  function commitFilters(next: FilterState): void {
    const updated = new URLSearchParams(searchParams);
    serializeFilterParams(next, updated);
    // A filter change invalidates keyset pagination position.
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
  }

  function commitSort(levels: readonly SortLevelParam[]): void {
    setSearchParams(applySortToSearchParams(searchParams, levels), { replace: true });
  }

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
        commitFilters({ ...filters, [family]: { all: [], any: [], none: [] } });
      }}
    >
      <VocabEditor
        family={family}
        draft={filters}
        setDraft={commitFilters}
        resolveAuthorLabel={authorNames.labelFor}
      />
    </RailSection>
  );

  return (
    <aside aria-label="Filters" className="flex flex-col gap-4 text-sm">
      <QuickSearch
        value={filters.q ?? ""}
        resetToken={filterResetToken(filters)}
        onCommit={(q) => {
          commitFilters({ ...filters, q });
        }}
      />
      <SortSection levels={sortLevels} onChange={commitSort} />
      <ShelfSection
        activeShelf={filters.shelf}
        onPick={(shelf) => {
          commitFilters({ ...filters, shelf });
        }}
      />
      <RailSection
        title="Series"
        activeCount={filters.series === undefined ? 0 : 1}
        onClear={() => {
          commitFilters({ ...filters, series: undefined });
        }}
      >
        <FacetList
          options={seriesOptions}
          active={filters.series}
          emptyText="No series in view."
          onPick={(series) => {
            commitFilters({ ...filters, series });
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
          commitFilters({ ...filters, status: { any: [], none: [] } });
        }}
      >
        <StatusEditor
          value={filters.status.any}
          onChange={(any) => {
            commitFilters({ ...filters, status: { ...filters.status, any } });
          }}
        />
      </RailSection>
      <TextSection
        title="Title"
        ops={TITLE_OPS}
        committed={filters.title}
        fullToken={fullToken}
        activeCount={textCount(filters.title)}
        onCommit={(title) => {
          commitFilters({ ...filters, title });
        }}
      />
      <TextSection
        title="Subtitle"
        ops={SUBTITLE_OPS}
        committed={filters.subtitle}
        fullToken={fullToken}
        activeCount={textCount(filters.subtitle)}
        onCommit={(subtitle) => {
          commitFilters({ ...filters, subtitle });
        }}
      />
      <TextSection
        title="ISBN"
        ops={ISBN_OPS}
        committed={filters.isbn13}
        fullToken={fullToken}
        activeCount={textCount(filters.isbn13)}
        onCommit={(isbn13) => {
          commitFilters({ ...filters, isbn13 });
        }}
      />
      <RangeSection
        title="Pages"
        min={0}
        committed={filters.pages}
        fullToken={fullToken}
        activeCount={rangeCount(filters.pages)}
        onCommit={(pages) => {
          commitFilters({ ...filters, pages });
        }}
      />
      <RangeSection
        title="Rating"
        min={1}
        max={5}
        committed={filters.rating}
        fullToken={fullToken}
        activeCount={rangeCount(filters.rating)}
        onCommit={(rating) => {
          commitFilters({ ...filters, rating });
        }}
      />
      <RailSection
        title="Added"
        activeCount={
          (filters.addedAfter === undefined ? 0 : 1) + (filters.addedBefore === undefined ? 0 : 1)
        }
        onClear={() => {
          commitFilters({ ...filters, addedAfter: undefined, addedBefore: undefined });
        }}
      >
        <DateRangeEditor
          after={filters.addedAfter}
          before={filters.addedBefore}
          onChange={({ after, before }) => {
            commitFilters({ ...filters, addedAfter: after, addedBefore: before });
          }}
        />
      </RailSection>
    </aside>
  );
}

type QuickSearchProps = {
  value: string;
  resetToken: string;
  onCommit: (q: string | undefined) => void;
};

function QuickSearch({ value, resetToken, onCommit }: QuickSearchProps): ReactElement {
  const [draft, setDraft] = useState(value);
  const [synced, setSynced] = useState({ value, resetToken });
  // Reset the input to reflect `value` when the committed `q` changes from
  // outside (navigation) OR when any other filter changes (a section clear,
  // clear-all). The second case discards an uncommitted draft so a pending
  // keystroke cannot resurrect after the user clears filters.
  if (value !== synced.value || resetToken !== synced.resetToken) {
    setSynced({ value, resetToken });
    setDraft(value);
  }
  const trimmed = draft.trim();
  const next = trimmed.length >= MIN_Q_LEN ? trimmed : undefined;
  // Treating an empty box (`next` undefined) as equal to an absent `q`
  // (`value` "") stops an untouched search from committing spuriously.
  useDebouncedCommit(
    next ?? "",
    value,
    () => {
      onCommit(next);
    },
    FILTER_DEBOUNCE_MS,
  );

  return (
    <Input
      type="search"
      aria-label="Quick search"
      placeholder="Quick search…"
      className="h-9"
      value={draft}
      onChange={(event) => {
        setDraft(event.target.value);
      }}
    />
  );
}

type RailSectionProps = {
  title: string;
  /** Number of active conditions; positive renders the badge + clear control. */
  activeCount: number;
  onClear: () => void;
  children: ReactNode;
};

/**
 * One collapsible rail section. Open state is uncontrolled after mount (the
 * initial value is captured once), so a filter edit re-rendering the rail
 * never snaps a section the user toggled; a section that mounts with active
 * conditions starts open.
 */
function RailSection({ title, activeCount, onClear, children }: RailSectionProps): ReactElement {
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
            aria-label={`Clear ${title} filters`}
            onClick={(event) => {
              // A summary click toggles the disclosure; clearing must not.
              event.preventDefault();
              event.stopPropagation();
              onClear();
            }}
            className="text-fg-muted hover:text-fg focus-visible:ring-accent ml-auto flex min-h-6 items-center rounded-sm px-2 font-mono text-xs uppercase tracking-wide transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            Clear
          </button>
        ) : null}
      </summary>
      <div className="mt-2">{children}</div>
    </details>
  );
}

type SortSectionProps = {
  levels: readonly SortLevelParam[];
  onChange: (levels: readonly SortLevelParam[]) => void;
};

/**
 * Sort-stack editor: add, remove, reorder, and flip levels. The table view's
 * header click / ctrl-click writes the same stack; this section is the sort
 * home for grid and list and the full-stack editor everywhere. The sr-only
 * summary renders unconditionally as a live region so a screen reader hears
 * every stack change, including header-driven ones.
 */
function SortSection({ levels, onChange }: SortSectionProps): ReactElement {
  const summaryId = useId();
  const remaining = SORT_FIELDS.filter((field) => !levels.some((level) => level.field === field));
  const summary =
    levels.length === 0
      ? "Not sorted"
      : `Sorted by ${levels
          .map(
            (level) =>
              `${SORT_FIELD_LABELS[level.field]} ${level.desc ? "descending" : "ascending"}`,
          )
          .join(", then ")}`;

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
      activeCount={levels.length}
      onClear={() => {
        onChange([]);
      }}
    >
      <p id={summaryId} className="sr-only" aria-live="polite">
        {summary}
      </p>
      <div role="group" aria-label="Sort order" className="flex flex-col gap-1">
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
      </div>
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
function FacetList({ options, active, emptyText, onPick }: FacetListProps): ReactElement {
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
};

function ShelfSection({ activeShelf, onPick }: ShelfSectionProps): ReactElement {
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
  const emptyText = isLoading
    ? "Loading shelves…"
    : isError
      ? "Couldn't load shelves."
      : "No shelves yet.";

  return (
    <RailSection
      title="Shelf"
      activeCount={activeShelf === undefined ? 0 : 1}
      onClear={() => {
        onPick(undefined);
      }}
    >
      <FacetList options={options} active={activeShelf} emptyText={emptyText} onPick={onPick} />
    </RailSection>
  );
}

type TextSectionProps = {
  title: string;
  ops: readonly TextOp[];
  committed: TextFilter;
  fullToken: string;
  activeCount: number;
  onCommit: (next: TextFilter) => void;
};

function TextSection({
  title,
  ops,
  committed,
  fullToken,
  activeCount,
  onCommit,
}: TextSectionProps): ReactElement {
  const { draft, setDraft } = useDraftSlice(committed, fullToken, textSliceToken, onCommit);
  return (
    <RailSection
      title={title}
      activeCount={activeCount}
      onClear={() => {
        onCommit({});
      }}
    >
      <TextFilterEditor value={draft} ops={ops} onChange={setDraft} />
    </RailSection>
  );
}

type RangeSectionProps = {
  title: string;
  min?: number;
  max?: number;
  committed: RangeFilter;
  fullToken: string;
  activeCount: number;
  onCommit: (next: RangeFilter) => void;
};

function RangeSection({
  title,
  min,
  max,
  committed,
  fullToken,
  activeCount,
  onCommit,
}: RangeSectionProps): ReactElement {
  const { draft, setDraft } = useDraftSlice(committed, fullToken, rangeSliceToken, onCommit);
  return (
    <RailSection
      title={title}
      activeCount={activeCount}
      onClear={() => {
        onCommit({});
      }}
    >
      <RangeFilterEditor value={draft} min={min} max={max} allowEmpty onChange={setDraft} />
    </RailSection>
  );
}
