/**
 * Contextual filter rail for browse surfaces: the editing surface for the
 * library's filter grammar and sort stack. Quick search lives in the page
 * toolbar (`components/library/QuickSearch`) and shares this rail's
 * draft-survival protocol through the container-owned `lastEdit` ref.
 *
 * One collapsible section per editable concern: the sort stack, the shelf
 * and series facets, the vocabulary families (authors, tags, genres,
 * moods), reading status, the text-operator columns (title, subtitle,
 * ISBN), the numeric ranges (pages, rating), and the added-date range. A
 * section with active conditions opens on mount, shows a count beside its
 * name, and carries a clear affordance; inactive sections mount collapsed
 * to keep the column scannable.
 *
 * URL state is canonical: the rail parses the search params it edits and
 * every write goes through the filter codec (or the sort helper) and the
 * page-owned `applyParams` write authority, always dropping `cursor`
 * because a changed filter or sort invalidates the keyset position.
 * Free-text and numeric inputs edit a local draft committed after a
 * debounce. A clear affordance or an external committed change (navigation,
 * the page's clear-all) resyncs those drafts, so a pending keystroke can
 * never resurrect a cleared condition; the rail's own edits in other
 * sections leave them alone, so a sibling's commit never eats in-flight
 * keystrokes.
 * The masthead summary is the read-only counterpart of this surface; the
 * table view's header click / ctrl-click sort is the one gesture that edits
 * the same URL state from outside the rail.
 */
import { useQuery } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, X } from "lucide-react";
import { useEffect, useState, type ReactElement, type ReactNode, type RefObject } from "react";

import { useSearchParams } from "react-router";

import {
  listShelves,
  MAX_SORT_LEVELS,
  parseSortParam,
  SORT_FIELDS,
  type Shelf,
  type SortLevelParam,
} from "@/api";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  FILTER_DEBOUNCE_MS,
  fullFilterToken,
  makeFilterClear,
  makeFilterCommit,
  type EditTokens,
} from "@/components/library/filter-commit";
import { SORT_FIELD_LABELS } from "@/components/library/sort-summary";
import { useAuthorLabels } from "@/lib/hooks/use-author-labels";
import { useDebouncedCommit } from "@/lib/hooks/use-debounced-commit";
import type { ApplyParams } from "@/lib/hooks/use-live-search-params";
import { queryKeys } from "@/lib/query/keys";
import {
  applySortToSearchParams,
  parseFilterParams,
  TEXT_COLUMN_OPS,
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
 * `fullToken` is the committed-state serialization; a change resyncs the
 * draft to the committed value, cancelling a pending debounce so a stale
 * draft cannot resurrect a condition the user just cleared. The one
 * exception: when `lastEdit` shows the change is the rail's own edit in a
 * different section (this slice untouched), the draft survives, so a
 * sibling's debounced commit never eats in-flight keystrokes.
 */
function useDraftSlice<T>(
  committed: T,
  fullToken: string,
  sliceToken: (value: T) => string,
  commit: (next: T) => void,
  lastEdit: RefObject<EditTokens | null>,
  clearGen: RefObject<number>,
): { draft: T; setDraft: (next: T) => void } {
  // The generation is captured when the draft is EDITED, not per render:
  // a clearing write (or the drawer's Escape = abandon) bumps it after
  // the edit, so the fire-time check below vetoes the stale draft even
  // if a passive re-render (e.g. the drawer's closing pass) happens
  // between the bump and the timer or unmount flush.
  const [draftState, setDraftState] = useState(() => ({
    value: committed,
    gen: clearGen.current,
  }));
  const [synced, setSynced] = useState({ full: fullToken, slice: sliceToken(committed) });
  // Render-phase state adjustment, the compiler-accepted alternative to a
  // sync effect.
  if (fullToken !== synced.full) {
    const nextSlice = sliceToken(committed);
    const editElsewhere = lastEdit.current?.full === fullToken && nextSlice === synced.slice;
    setSynced({ full: fullToken, slice: nextSlice });
    if (!editElsewhere) setDraftState({ value: committed, gen: clearGen.current });
  }
  const draft = draftState.value;
  // A clear bumps the generation synchronously, but the render that resyncs
  // the draft (and cancels the timer) rides a router transition, which an
  // already-due timer can beat; the fire-time check keeps a stale draft from
  // re-writing the condition the clear just removed.
  useDebouncedCommit(
    sliceToken(draft),
    sliceToken(committed),
    () => {
      commit(draft);
    },
    FILTER_DEBOUNCE_MS,
    () => clearGen.current !== draftState.gen,
    // The rail unmounts when its drawer closes; a pending draft then
    // commits (the user typed it deliberately) unless a generation bump
    // (clear-all, or the drawer's Escape = abandon) vetoed it above.
    true,
  );
  return {
    draft,
    setDraft: (next: T) => {
      setDraftState({ value: next, gen: clearGen.current });
    },
  };
}

interface FilterRailProps {
  /** Distinct series from the loaded pages. */
  seriesOptions: SeriesFacetOption[];
  /** The page-owned URL write authority; every rail write goes through it
   *  so rail and page writers cannot clobber each other in one frame. */
  applyParams: ApplyParams;
  /** The write authority's clearing-write generation; pending debounced
   *  drafts check it at fire time so any clearing writer, including the
   *  page's clear-all, invalidates them. */
  clearGen: RefObject<number>;
  /** The container-owned edit-token ref shared with the toolbar quick
   *  search; one instance per page or the draft-survival protocol splits
   *  (see `filter-commit.ts`). */
  lastEdit: RefObject<EditTokens | null>;
  /** Drawer-close abandon generation: the host bumps it when the drawer
   *  closes via Escape, so pending drafts die instead of flushing. Other
   *  close paths leave it alone and pending drafts commit on unmount. */
  cancelGen: RefObject<number>;
}

/** The library's filter and sort editing surface; see the module docstring. */
export function FilterRail({
  seriesOptions,
  applyParams,
  clearGen,
  lastEdit,
  cancelGen,
}: Readonly<FilterRailProps>): ReactElement {
  const [searchParams] = useSearchParams();
  const filters = parseFilterParams(searchParams);
  const sortLevels = parseSortParam(searchParams.get("sort") ?? "");
  const fullToken = fullFilterToken(filters);

  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);

  const commitFilters = makeFilterCommit(applyParams, lastEdit);
  const clearFilters = makeFilterClear(applyParams, lastEdit);

  // One generation ref for the sections' fire-time staleness check,
  // covering both invalidation sources: clearing writes (clearGen) and
  // the drawer's Escape = abandon (cancelGen). Both only ever increment,
  // so the sum changes whenever either does; a getter keeps reads live
  // without threading a second ref through every section.
  const [staleGen] = useState(() => ({
    get current(): number {
      return clearGen.current + cancelGen.current;
    },
  }));

  function commitSort(levels: readonly SortLevelParam[]): void {
    applyParams((params) => applySortToSearchParams(params, levels));
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
        clearFilters((current) => ({ ...current, [family]: { all: [], any: [], none: [] } }));
      }}
    >
      <VocabEditor
        family={family}
        draft={filters}
        setDraft={(next) => {
          // Take only this editor's family from its output: the rest of the
          // editor's state is a render snapshot and must not overwrite a
          // commit that landed since.
          commitFilters((current) => ({ ...current, [family]: next[family] }));
        }}
        resolveAuthorLabel={authorNames.labelFor}
      />
    </RailSection>
  );

  return (
    <aside aria-label="Filters" className="flex flex-col gap-4 text-sm">
      <SortSection levels={sortLevels} onChange={commitSort} />
      <ShelfSection
        activeShelf={filters.shelf}
        onPick={(shelf) => {
          commitFilters((current) => ({ ...current, shelf }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, shelf: undefined }));
        }}
      />
      <RailSection
        title="Series"
        activeCount={filters.series === undefined ? 0 : 1}
        onClear={() => {
          clearFilters((current) => ({ ...current, series: undefined }));
        }}
      >
        <FacetList
          options={seriesOptions}
          active={filters.series}
          emptyText="No series in view."
          onPick={(series) => {
            commitFilters((current) => ({ ...current, series }));
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
          clearFilters((current) => ({ ...current, status: { any: [], none: [] } }));
        }}
      >
        <StatusEditor
          value={filters.status.any}
          onChange={(any) => {
            commitFilters((current) => ({ ...current, status: { ...current.status, any } }));
          }}
        />
      </RailSection>
      <TextSection
        title="Title"
        ops={TEXT_COLUMN_OPS.title}
        committed={filters.title}
        fullToken={fullToken}
        lastEdit={lastEdit}
        clearGen={staleGen}
        activeCount={textCount(filters.title)}
        onCommit={(title) => {
          commitFilters((current) => ({ ...current, title }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, title: {} }));
        }}
      />
      <TextSection
        title="Subtitle"
        ops={TEXT_COLUMN_OPS.subtitle}
        committed={filters.subtitle}
        fullToken={fullToken}
        lastEdit={lastEdit}
        clearGen={staleGen}
        activeCount={textCount(filters.subtitle)}
        onCommit={(subtitle) => {
          commitFilters((current) => ({ ...current, subtitle }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, subtitle: {} }));
        }}
      />
      <TextSection
        title="ISBN"
        ops={TEXT_COLUMN_OPS.isbn_13}
        committed={filters.isbn13}
        fullToken={fullToken}
        lastEdit={lastEdit}
        clearGen={staleGen}
        activeCount={textCount(filters.isbn13)}
        onCommit={(isbn13) => {
          commitFilters((current) => ({ ...current, isbn13 }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, isbn13: {} }));
        }}
      />
      <RangeSection
        title="Pages"
        min={0}
        committed={filters.pages}
        fullToken={fullToken}
        lastEdit={lastEdit}
        clearGen={staleGen}
        activeCount={rangeCount(filters.pages)}
        onCommit={(pages) => {
          commitFilters((current) => ({ ...current, pages }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, pages: {} }));
        }}
      />
      <RangeSection
        title="Rating"
        min={1}
        max={5}
        committed={filters.rating}
        fullToken={fullToken}
        lastEdit={lastEdit}
        clearGen={staleGen}
        activeCount={rangeCount(filters.rating)}
        onCommit={(rating) => {
          commitFilters((current) => ({ ...current, rating }));
        }}
        onClear={() => {
          clearFilters((current) => ({ ...current, rating: {} }));
        }}
      />
      <RailSection
        title="Added"
        activeCount={
          (filters.addedAfter === undefined ? 0 : 1) + (filters.addedBefore === undefined ? 0 : 1)
        }
        onClear={() => {
          clearFilters((current) => ({
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
            commitFilters((current) => ({ ...current, addedAfter: after, addedBefore: before }));
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
 * home for grid and list and the full-stack editor everywhere. Stack
 * announcements are not this section's job: the aria-live sort summary is
 * mounted by `LibraryPage`, because the rail unmounts when collapsed and a
 * live region must stay mounted to announce.
 */
function SortSection({ levels, onChange }: Readonly<SortSectionProps>): ReactElement {
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
      activeCount={levels.length}
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
  committed: TextFilter;
  fullToken: string;
  lastEdit: RefObject<EditTokens | null>;
  clearGen: RefObject<number>;
  activeCount: number;
  onCommit: (next: TextFilter) => void;
  /** Clear-affordance path, distinct from `onCommit` so it resets drafts. */
  onClear: () => void;
};

function TextSection({
  title,
  ops,
  committed,
  fullToken,
  lastEdit,
  clearGen,
  activeCount,
  onCommit,
  onClear,
}: Readonly<TextSectionProps>): ReactElement {
  const { draft, setDraft } = useDraftSlice(
    committed,
    fullToken,
    textSliceToken,
    onCommit,
    lastEdit,
    clearGen,
  );
  return (
    <RailSection title={title} activeCount={activeCount} onClear={onClear}>
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
  lastEdit: RefObject<EditTokens | null>;
  clearGen: RefObject<number>;
  activeCount: number;
  onCommit: (next: RangeFilter) => void;
  /** Clear-affordance path, distinct from `onCommit` so it resets drafts. */
  onClear: () => void;
};

function RangeSection({
  title,
  min,
  max,
  committed,
  fullToken,
  lastEdit,
  clearGen,
  activeCount,
  onCommit,
  onClear,
}: Readonly<RangeSectionProps>): ReactElement {
  const { draft, setDraft } = useDraftSlice(
    committed,
    fullToken,
    rangeSliceToken,
    onCommit,
    lastEdit,
    clearGen,
  );
  return (
    <RailSection title={title} activeCount={activeCount} onClear={onClear}>
      <RangeFilterEditor value={draft} min={min} max={max} allowEmpty onChange={setDraft} />
    </RailSection>
  );
}
