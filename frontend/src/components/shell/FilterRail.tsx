import { useQuery } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, X } from "lucide-react";
import { Button as AriaButton } from "react-aria-components";
import { useEffect, type ReactElement, type ReactNode } from "react";

import { listShelves, MAX_SORT_LEVELS, SORT_FIELDS, type Shelf, type SortLevelParam } from "@/api";
import { Checkbox } from "@/components/ui/checkbox";
import { Disclosure, DisclosurePanel } from "@/components/ui/collapsible";
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
import type { LibraryFilters } from "@/lib/hooks/use-library-filters";
import { queryKeys } from "@/lib/query/keys";
import {
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
export type SeriesFacetOption = {
  id: string;
  name: string;
};

type FilterRailProps = {
  owner: LibraryFilters;
  showContradiction: boolean;
  /** Distinct series from the loaded pages. */
  seriesOptions: SeriesFacetOption[];
  /** The resolved effective sort stack; see the module docstring. */
  sortLevels: readonly SortLevelParam[];
  /** True when the stack is the inherited installation order, which is when
   *  the sort section offers no reset. */
  sortInherited: boolean;
  /** The page's one sort intent handler; an empty stack means reset. */
  onSortChange: (levels: readonly SortLevelParam[]) => void;
};

/** The library's filter and sort editing surface; see the module docstring. */
export function FilterRail({
  owner,
  showContradiction,
  seriesOptions,
  sortLevels,
  sortInherited,
  onSortChange,
}: Readonly<FilterRailProps>): ReactElement {
  const { filters, commitAll } = owner;

  const authorIds = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const authorNames = useAuthorLabels(authorIds);

  const setCount = (set: { all: string[]; any: string[]; none: string[] }): number =>
    set.all.length + set.any.length + set.none.length;
  const textCount = (value: TextFilter): number =>
    value.op === "empty" || (value.text?.trim() ?? "") !== "" ? 1 : 0;
  const rangeCount = (value: RangeFilter): number =>
    [value.gte, value.lte, value.empty].filter((part) => part !== undefined).length;

  const vocabSection = (family: VocabFamily, title: string): ReactElement => (
    <RailSection
      title={title}
      activeCount={setCount(filters[family])}
      onClear={() => {
        commitAll((current) => ({
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
          commitAll((current) => ({ ...current, [family]: next[family] }));
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
          commitAll((current) => ({ ...current, shelf }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, shelf: undefined }));
        }}
      />
      <RailSection
        title="Series"
        activeCount={filters.series === undefined ? 0 : 1}
        onClear={() => {
          commitAll((current) => ({ ...current, series: undefined }));
        }}
      >
        <FacetList
          options={seriesOptions}
          active={filters.series}
          emptyText="No series in view."
          onPick={(series) => {
            commitAll((current) => ({ ...current, series }));
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
          commitAll((current) => ({ ...current, status: { any: [], none: [] } }));
        }}
      >
        <StatusEditor
          value={filters.status.any}
          onChange={(any) => {
            commitAll((current) => ({
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
          commitAll((current) => ({ ...current, title }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, title: {} }));
        }}
      />
      <TextSection
        title="Subtitle"
        ops={TEXT_COLUMN_OPS.subtitle}
        value={filters.subtitle}
        activeCount={textCount(filters.subtitle)}
        onChange={(subtitle) => {
          commitAll((current) => ({ ...current, subtitle }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, subtitle: {} }));
        }}
      />
      <TextSection
        title="ISBN"
        ops={TEXT_COLUMN_OPS.isbn_13}
        value={filters.isbn13}
        activeCount={textCount(filters.isbn13)}
        onChange={(isbn13) => {
          commitAll((current) => ({ ...current, isbn13 }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, isbn13: {} }));
        }}
      />
      <RangeSection
        title="Pages"
        showContradiction={showContradiction}
        min={0}
        max={2147483647}
        value={filters.pages}
        activeCount={rangeCount(filters.pages)}
        onChange={(patch) => {
          commitAll((current) => ({ ...current, pages: patch(current.pages) }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, pages: {} }));
        }}
      />
      <RangeSection
        title="Rating"
        showContradiction={showContradiction}
        min={1}
        max={5}
        value={filters.rating}
        activeCount={rangeCount(filters.rating)}
        onChange={(patch) => {
          commitAll((current) => ({ ...current, rating: patch(current.rating) }));
        }}
        onClear={() => {
          commitAll((current) => ({ ...current, rating: {} }));
        }}
      />
      <RailSection
        title="Added"
        activeCount={
          (filters.addedAfter === undefined ? 0 : 1) + (filters.addedBefore === undefined ? 0 : 1)
        }
        onClear={() => {
          commitAll((current) => ({
            ...current,
            addedAfter: undefined,
            addedBefore: undefined,
          }));
        }}
      >
        <DateRangeEditor
          showContradiction={showContradiction}
          after={filters.addedAfter}
          before={filters.addedBefore}
          onChange={(patch) => {
            commitAll((current) => {
              const next = patch({ after: current.addedAfter, before: current.addedBefore });
              return { ...current, addedAfter: next.after, addedBefore: next.before };
            });
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
 * One collapsible rail section. Open state follows only the disclosure
 * control after mount (the initial value is captured once), so a filter edit
 * re-rendering the rail never snaps a section the user toggled; a section
 * that mounts with active conditions starts open. The clear control is the
 * disclosure's sibling, never its descendant, so each is exposed and
 * operated on its own.
 */
function RailSection({
  title,
  activeCount,
  onClear,
  clearLabel = "Clear",
  clearAriaLabel,
  children,
}: Readonly<RailSectionProps>): ReactElement {
  const active = activeCount > 0;
  return (
    <section>
      <Disclosure defaultExpanded={activeCount > 0} className="group/section">
        <div className="flex items-center gap-2">
          <AriaButton
            type="button"
            slot="trigger"
            className={`flex flex-1 cursor-pointer select-none items-center gap-2 self-stretch text-left font-mono text-xs uppercase tracking-[0.14em] ${
              active ? "text-accent-text" : "text-fg-muted"
            }`}
          >
            <span>{title}</span>
            {active ? (
              <span className="bg-accent-soft text-fg rounded-full px-1.5 py-0.5 text-[0.65rem] leading-none">
                {activeCount}
              </span>
            ) : null}
          </AriaButton>
          {active ? (
            <button
              type="button"
              aria-label={clearAriaLabel ?? `Clear ${title} filters`}
              onClick={onClear}
              className="text-fg-muted hover:text-fg focus-visible:ring-accent flex min-h-6 items-center rounded-sm px-2 font-mono text-xs uppercase tracking-wide transition-colors focus-visible:outline-none focus-visible:ring-2"
            >
              {clearLabel}
            </button>
          ) : null}
        </div>
        <DisclosurePanel className="group-data-expanded/section:mt-2">{children}</DisclosurePanel>
      </Disclosure>
    </section>
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
                onPress={() => {
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
                isDisabled={index === 0}
                onPress={() => {
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
                isDisabled={index === levels.length - 1}
                onPress={() => {
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
                // Disabled, not hidden, matching the reorder arrows at the
                // stack's ends: removing the last inherited level would
                // dispatch a reset to the state already showing, a press
                // that visibly does nothing.
                isDisabled={inherited && levels.length === 1}
                onPress={() => {
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
            aria-label="Add sort field"
            placeholder="Add sort field…"
            value=""
            onChange={(field) => {
              if (typeof field !== "string") return;
              const match = remaining.find((candidate) => candidate === field);
              if (match !== undefined) onChange([...levels, { field: match, desc: false }]);
            }}
          >
            <SelectTrigger className="h-8 w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {remaining.map((field) => (
                <SelectItem key={field} id={field}>
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
        <Checkbox
          key={option.id}
          value={option.id}
          isSelected={active === option.id}
          onChange={(selected) => {
            onPick(selected ? option.id : undefined);
          }}
          className="text-fg-muted hover:bg-surface hover:text-fg data-selected:bg-surface data-selected:text-fg w-full cursor-pointer gap-2 rounded-md px-2 py-1.5 font-normal transition-colors [&>[data-slot=checkbox-indicator]]:size-3.5"
        >
          <span className="truncate">{option.name}</span>
        </Checkbox>
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
  onClear: () => void;
};

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
  showContradiction: boolean;
  title: string;
  min?: number;
  max?: number;
  value: RangeFilter;
  activeCount: number;
  onChange: (patch: (current: RangeFilter) => RangeFilter) => void;
  onClear: () => void;
};

function RangeSection({
  showContradiction,
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
      <RangeFilterEditor
        label={title}
        showContradiction={showContradiction}
        value={value}
        min={min}
        max={max}
        allowEmpty
        onChange={onChange}
      />
    </RailSection>
  );
}
