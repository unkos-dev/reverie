/**
 * Library filter bar: quick search, a per-column filter builder, and the
 * active-condition chip row. Presentational per the container split, it reads a
 * {@link FilterState} and reports edits through `onFiltersChange`; it owns no
 * URL logic (the page turns state changes into search-param navigation).
 *
 * Quick search narrows the table in place (writes the `q` filter) and is
 * distinct from the command palette, which navigates to a single book. Typing
 * only re-renders the local input; the debounced value is what reaches the
 * parent, so the grid does not refetch on every keystroke.
 */
import { useQuery } from "@tanstack/react-query";
import { Filter, X } from "lucide-react";
import { type ReactElement, useEffect, useState } from "react";

import { resolveAuthors, type SuggestKind } from "@/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { queryKeys } from "@/lib/query/keys";
import {
  emptyFilterState,
  serializeFilterParams,
  type FilterState,
  type SetFilter,
} from "@/routes/library-params";

import {
  DateRangeEditor,
  RangeFilterEditor,
  StatusEditor,
  TextFilterEditor,
  type TextOp,
} from "./editors";
import { TypeaheadMultiSelect, type TypeaheadOption } from "./TypeaheadMultiSelect";

/** Minimum quick-search length; below it the input clears the `q` filter. */
const MIN_Q_LEN = 2;
/** Debounce before a quick-search keystroke becomes a URL change. */
const QUICK_SEARCH_DEBOUNCE_MS = 250;

/** A stable, order-independent serialization of every filter *except* `q`.
 *  Quick search watches this: it stays constant while the user types (and
 *  across the debounced `q` write), and changes only on an external filter
 *  edit (a chip removal, clear-all), which is the signal to discard an
 *  uncommitted quick-search draft. */
function filterResetToken(filters: FilterState): string {
  const params = new URLSearchParams();
  serializeFilterParams(filters, params);
  params.delete("q");
  params.sort();
  return params.toString();
}

const STATUS_LABELS: Record<string, string> = {
  unread: "Unread",
  want_to_read: "Want to read",
  reading: "Reading",
  on_hold: "On hold",
  finished: "Finished",
  abandoned: "Abandoned",
};

type ColumnId =
  | "title"
  | "subtitle"
  | "isbn13"
  | "pages"
  | "rating"
  | "added"
  | "status"
  | "authors"
  | "series"
  | "tags"
  | "genres"
  | "moods";

const COLUMN_ORDER: readonly ColumnId[] = [
  "title",
  "subtitle",
  "isbn13",
  "pages",
  "rating",
  "added",
  "status",
  "authors",
  "series",
  "tags",
  "genres",
  "moods",
];

const COLUMN_LABELS: Record<ColumnId, string> = {
  title: "Title",
  subtitle: "Subtitle",
  isbn13: "ISBN",
  pages: "Pages",
  rating: "Rating",
  added: "Added",
  status: "Status",
  authors: "Authors",
  series: "Series",
  tags: "Tags",
  genres: "Genres",
  moods: "Moods",
};

/** Vocabulary families edited through the mode-select + typeahead widget. */
type VocabFamily = "authors" | "tags" | "genres" | "moods";
const VOCAB_KIND: Record<VocabFamily, SuggestKind> = {
  authors: "authors",
  tags: "tags",
  genres: "genres",
  moods: "moods",
};

type SetMode = "all" | "any" | "none";
const MODE_WORD: Record<SetMode, string> = { all: "all of", any: "any of", none: "none of" };

type Props = {
  filters: FilterState;
  onFiltersChange: (next: FilterState) => void;
  /** Series id to display name, from the page's loaded rows, for chip labels. */
  seriesLabels?: ReadonlyMap<string, string>;
};

export function FilterBar({ filters, onFiltersChange, seriesLabels }: Props): ReactElement {
  const authorNames = useAuthorLabels(filters);

  return (
    <div className="mb-4 flex flex-col gap-3" data-testid="filter-bar">
      <div className="flex flex-wrap items-center gap-2">
        <QuickSearch
          value={filters.q ?? ""}
          resetToken={filterResetToken(filters)}
          onCommit={(q) => {
            onFiltersChange({ ...filters, q });
          }}
        />
        <FilterBuilder
          filters={filters}
          onApply={onFiltersChange}
          resolveAuthorLabel={authorNames.labelFor}
          seriesLabels={seriesLabels}
        />
      </div>
      <ChipRow
        filters={filters}
        onFiltersChange={onFiltersChange}
        resolveAuthorLabel={authorNames.labelFor}
        seriesLabels={seriesLabels}
      />
    </div>
  );
}

/** Author id to display label resolution for chips and the authors editor. */
function useAuthorLabels(filters: FilterState): { labelFor: (id: string) => string } {
  const ids = [
    ...new Set([...filters.authors.all, ...filters.authors.any, ...filters.authors.none]),
  ];
  const { data, isFetching, isError, error } = useQuery({
    queryKey: queryKeys.authors.resolve(ids),
    queryFn: ({ signal }) => resolveAuthors(ids, signal),
    enabled: ids.length > 0,
    staleTime: 60_000,
  });
  // A resolve failure must not read as "author doesn't exist": leave a console
  // breadcrumb (QueryCache.onError forwards only 401s) and label the chip as
  // unresolved rather than unknown, mirroring the next-page handler in
  // LibraryContent.
  useEffect(() => {
    if (isError) console.error("[FilterBar] failed to resolve author labels", error);
  }, [isError, error]);
  function labelFor(id: string): string {
    const hit = data?.find((row) => row.id === id);
    if (hit !== undefined) return hit.value;
    if (isFetching) return "resolving…";
    if (isError) return "author unavailable";
    return "Unknown author";
  }
  return { labelFor };
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
  // outside (navigation) OR when any other filter changes (a chip removal,
  // clear-all). The second case discards an uncommitted draft so a pending
  // keystroke cannot resurrect after the user clears filters. Render-phase
  // state adjustment, the compiler-accepted alternative to a sync effect.
  if (value !== synced.value || resetToken !== synced.resetToken) {
    setSynced({ value, resetToken });
    setDraft(value);
  }

  useEffect(() => {
    const trimmed = draft.trim();
    const next = trimmed.length >= MIN_Q_LEN ? trimmed : undefined;
    // Nothing to commit when the debounced draft already matches the committed
    // value. Treating an empty box (`next` undefined) as equal to an absent `q`
    // (`value` "") is what stops an untouched search from firing a spurious
    // commit on every re-render.
    if ((next ?? "") === value) return;
    const handle = window.setTimeout(() => {
      onCommit(next);
    }, QUICK_SEARCH_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(handle);
    };
  }, [draft, value, onCommit]);

  return (
    <Input
      type="search"
      aria-label="Quick search"
      placeholder="Quick search…"
      className="h-8 w-56"
      value={draft}
      onChange={(event) => {
        setDraft(event.target.value);
      }}
    />
  );
}

type FilterBuilderProps = {
  filters: FilterState;
  onApply: (next: FilterState) => void;
  resolveAuthorLabel: (id: string) => string;
  seriesLabels?: ReadonlyMap<string, string>;
};

function FilterBuilder({
  filters,
  onApply,
  resolveAuthorLabel,
  seriesLabels,
}: FilterBuilderProps): ReactElement {
  const [open, setOpen] = useState(false);
  const [column, setColumn] = useState<ColumnId | null>(null);
  const [draft, setDraft] = useState<FilterState>(filters);

  function openBuilder(next: boolean): void {
    setOpen(next);
    if (next) {
      setDraft(filters);
      setColumn(null);
    }
  }

  function apply(): void {
    // The builder never edits `q`; quick search commits it on a debounce timer,
    // the one filter that can change while this popover stays open. Take `q`
    // from the live `filters` prop so a search committed after the builder
    // opened is not clobbered by the older `draft` snapshot.
    onApply({ ...draft, q: filters.q });
    setOpen(false);
    setColumn(null);
  }

  return (
    <Popover open={open} onOpenChange={openBuilder}>
      <PopoverTrigger asChild>
        <Button type="button" variant="outline" size="sm" className="h-8 gap-1">
          <Filter className="size-4" aria-hidden="true" />
          Add filter
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80">
        {column === null ? (
          <fieldset className="flex flex-col gap-1">
            <legend className="text-fg-muted mb-1 px-1 text-xs uppercase tracking-wide">
              Filter by column
            </legend>
            {COLUMN_ORDER.map((id) => (
              <Button
                key={id}
                type="button"
                variant="ghost"
                size="sm"
                className="justify-start"
                onClick={() => {
                  setColumn(id);
                }}
              >
                {COLUMN_LABELS[id]}
              </Button>
            ))}
          </fieldset>
        ) : (
          <div className="flex flex-col gap-3">
            <p className="text-fg text-sm font-medium">{COLUMN_LABELS[column]}</p>
            <ColumnEditor
              column={column}
              draft={draft}
              setDraft={setDraft}
              resolveAuthorLabel={resolveAuthorLabel}
              seriesLabels={seriesLabels}
            />
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => {
                  setColumn(null);
                }}
              >
                Back
              </Button>
              <Button type="button" size="sm" onClick={apply}>
                Apply
              </Button>
            </div>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}

type ColumnEditorProps = {
  column: ColumnId;
  draft: FilterState;
  setDraft: (next: FilterState) => void;
  resolveAuthorLabel: (id: string) => string;
  seriesLabels?: ReadonlyMap<string, string>;
};

function ColumnEditor({
  column,
  draft,
  setDraft,
  resolveAuthorLabel,
  seriesLabels,
}: ColumnEditorProps): ReactElement {
  switch (column) {
    case "title":
      return (
        <TextFilterEditor
          value={draft.title}
          ops={TITLE_OPS}
          onChange={(title) => {
            setDraft({ ...draft, title });
          }}
        />
      );
    case "subtitle":
      return (
        <TextFilterEditor
          value={draft.subtitle}
          ops={SUBTITLE_OPS}
          onChange={(subtitle) => {
            setDraft({ ...draft, subtitle });
          }}
        />
      );
    case "isbn13":
      return (
        <TextFilterEditor
          value={draft.isbn13}
          ops={ISBN_OPS}
          onChange={(isbn13) => {
            setDraft({ ...draft, isbn13 });
          }}
        />
      );
    case "pages":
      return (
        <RangeFilterEditor
          value={draft.pages}
          min={0}
          allowEmpty
          onChange={(pages) => {
            setDraft({ ...draft, pages });
          }}
        />
      );
    case "rating":
      return (
        <RangeFilterEditor
          value={draft.rating}
          min={1}
          max={5}
          allowEmpty
          onChange={(rating) => {
            setDraft({ ...draft, rating });
          }}
        />
      );
    case "added":
      return (
        <DateRangeEditor
          after={draft.addedAfter}
          before={draft.addedBefore}
          onChange={({ after, before }) => {
            setDraft({ ...draft, addedAfter: after, addedBefore: before });
          }}
        />
      );
    case "status":
      return (
        <StatusEditor
          value={draft.status.any}
          onChange={(any) => {
            setDraft({ ...draft, status: { ...draft.status, any } });
          }}
        />
      );
    case "series":
      return <SeriesEditor draft={draft} setDraft={setDraft} seriesLabels={seriesLabels} />;
    default:
      return (
        <VocabEditor
          family={column}
          draft={draft}
          setDraft={setDraft}
          resolveAuthorLabel={resolveAuthorLabel}
        />
      );
  }
}

const TITLE_OPS: readonly TextOp[] = ["contains", "eq", "ne"];
const SUBTITLE_OPS: readonly TextOp[] = ["contains", "empty"];
const ISBN_OPS: readonly TextOp[] = ["contains", "eq", "empty"];

type VocabEditorProps = {
  family: VocabFamily;
  draft: FilterState;
  setDraft: (next: FilterState) => void;
  resolveAuthorLabel: (id: string) => string;
};

function VocabEditor({
  family,
  draft,
  setDraft,
  resolveAuthorLabel,
}: VocabEditorProps): ReactElement {
  const set = draft[family];
  const [mode, setMode] = useState<SetMode>(() => initialMode(set));
  const isAuthors = family === "authors";

  const selected: TypeaheadOption[] = set[mode].map((token) =>
    isAuthors ? { id: token, value: resolveAuthorLabel(token) } : { value: token },
  );

  function handleChange(next: TypeaheadOption[]): void {
    // Authors store the id (uuid) as the URL token; other vocabularies store
    // the display value, which is the token itself.
    const tokens = next.map((option) => (isAuthors ? (option.id ?? option.value) : option.value));
    setDraft({ ...draft, [family]: { ...set, [mode]: tokens } });
  }

  return (
    <div className="flex flex-col gap-2">
      <Select
        value={mode}
        onValueChange={(value) => {
          if (isSetMode(value)) setMode(value);
        }}
      >
        <SelectTrigger className="w-32" aria-label="Match mode">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="any">any of</SelectItem>
          <SelectItem value="all">all of</SelectItem>
          <SelectItem value="none">none of</SelectItem>
        </SelectContent>
      </Select>
      <TypeaheadMultiSelect
        kind={VOCAB_KIND[family]}
        label={`${COLUMN_LABELS[family].toLowerCase()} (${MODE_WORD[mode]})`}
        selected={selected}
        onChange={handleChange}
      />
    </div>
  );
}

type SeriesEditorProps = {
  draft: FilterState;
  setDraft: (next: FilterState) => void;
  seriesLabels?: ReadonlyMap<string, string>;
};

/** Single-select series filter reusing the multi-select widget: only the most
 *  recent pick is kept, so the URL's single `series` uuid stays single. The
 *  selected chip resolves to a series name via the page's loaded rows, falling
 *  back to the id when the series is off the current page. */
function SeriesEditor({ draft, setDraft, seriesLabels }: SeriesEditorProps): ReactElement {
  const selected: TypeaheadOption[] =
    draft.series !== undefined
      ? [{ id: draft.series, value: seriesLabels?.get(draft.series) ?? draft.series }]
      : [];
  return (
    <TypeaheadMultiSelect
      kind="series"
      label="series"
      selected={selected}
      onChange={(next) => {
        const latest = next.at(-1);
        setDraft({ ...draft, series: latest?.id ?? latest?.value });
      }}
    />
  );
}

type ChipRowProps = {
  filters: FilterState;
  onFiltersChange: (next: FilterState) => void;
  resolveAuthorLabel: (id: string) => string;
  seriesLabels?: ReadonlyMap<string, string>;
};

type Chip = { key: string; label: string; next: FilterState };

function ChipRow({
  filters,
  onFiltersChange,
  resolveAuthorLabel,
  seriesLabels,
}: ChipRowProps): ReactElement | null {
  const chips = buildChips(filters, resolveAuthorLabel, seriesLabels);
  if (chips.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-2" data-testid="filter-chips">
      {chips.map((chip) => (
        <Button
          key={chip.key}
          type="button"
          variant="outline"
          size="sm"
          className="border-border bg-accent-soft text-fg hover:bg-accent-soft/80 h-7 gap-1 rounded-full px-3 font-mono text-xs"
          onClick={() => {
            onFiltersChange(chip.next);
          }}
        >
          {chip.label}
          <X className="size-3" aria-hidden="true" />
          <span className="sr-only">Remove filter</span>
        </Button>
      ))}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="h-7 text-xs"
        onClick={() => {
          onFiltersChange(emptyFilterState());
        }}
      >
        Clear filters
      </Button>
    </div>
  );
}

function initialMode(set: SetFilter): SetMode {
  if (set.any.length > 0) return "any";
  if (set.all.length > 0) return "all";
  if (set.none.length > 0) return "none";
  return "any";
}

function isSetMode(value: string): value is SetMode {
  return value === "all" || value === "any" || value === "none";
}

/**
 * Enumerate the active conditions as removable chips. Each chip carries the
 * `FilterState` that results from dropping just that condition, so removal is a
 * pure state transition the row applies through `onFiltersChange`.
 */
function buildChips(
  filters: FilterState,
  resolveAuthorLabel: (id: string) => string,
  seriesLabels?: ReadonlyMap<string, string>,
): Chip[] {
  const chips: Chip[] = [];
  const push = (key: string, label: string, next: FilterState): void => {
    chips.push({ key, label, next });
  };

  if (filters.q !== undefined) {
    push("q", `Search: ${filters.q}`, { ...filters, q: undefined });
  }

  pushTextChips(chips, filters, "title", "Title");
  pushTextChips(chips, filters, "subtitle", "Subtitle");
  pushTextChips(chips, filters, "isbn13", "ISBN");
  pushRangeChips(chips, filters, "pages", "Pages");
  pushRangeChips(chips, filters, "rating", "Rating");

  if (filters.addedAfter !== undefined) {
    push("added_after", `Added after ${filters.addedAfter}`, { ...filters, addedAfter: undefined });
  }
  if (filters.addedBefore !== undefined) {
    push("added_before", `Added before ${filters.addedBefore}`, {
      ...filters,
      addedBefore: undefined,
    });
  }

  for (const token of filters.status.any) {
    push(`status_any_${token}`, `Status: ${STATUS_LABELS[token] ?? token}`, {
      ...filters,
      status: { ...filters.status, any: filters.status.any.filter((t) => t !== token) },
    });
  }

  pushVocabChips(chips, filters, "authors", "Author", (id) => resolveAuthorLabel(id));
  pushVocabChips(chips, filters, "tags", "Tag");
  pushVocabChips(chips, filters, "genres", "Genre");
  pushVocabChips(chips, filters, "moods", "Mood");

  if (filters.series !== undefined) {
    const label = seriesLabels?.get(filters.series) ?? "Series";
    push("series", `Series: ${label}`, { ...filters, series: undefined });
  }

  return chips;
}

function pushTextChips(
  chips: Chip[],
  filters: FilterState,
  family: "title" | "subtitle" | "isbn13",
  label: string,
): void {
  const filter = filters[family];
  const clear = (patch: Partial<FilterState[typeof family]>): FilterState => ({
    ...filters,
    [family]: { ...filter, ...patch },
  });
  if (filter.contains !== undefined) {
    chips.push({
      key: `${family}_contains`,
      label: `${label} contains "${filter.contains}"`,
      next: clear({ contains: undefined }),
    });
  }
  if (filter.eq !== undefined) {
    chips.push({
      key: `${family}_eq`,
      label: `${label} is "${filter.eq}"`,
      next: clear({ eq: undefined }),
    });
  }
  if (filter.ne !== undefined) {
    chips.push({
      key: `${family}_ne`,
      label: `${label} is not "${filter.ne}"`,
      next: clear({ ne: undefined }),
    });
  }
  if (filter.empty !== undefined) {
    chips.push({
      key: `${family}_empty`,
      label: filter.empty ? `${label} is empty` : `${label} is set`,
      next: clear({ empty: undefined }),
    });
  }
}

function pushRangeChips(
  chips: Chip[],
  filters: FilterState,
  family: "pages" | "rating",
  label: string,
): void {
  const filter = filters[family];
  const clear = (patch: Partial<FilterState[typeof family]>): FilterState => ({
    ...filters,
    [family]: { ...filter, ...patch },
  });
  if (filter.gte !== undefined) {
    chips.push({
      key: `${family}_gte`,
      label: `${label} ≥ ${String(filter.gte)}`,
      next: clear({ gte: undefined }),
    });
  }
  if (filter.lte !== undefined) {
    chips.push({
      key: `${family}_lte`,
      label: `${label} ≤ ${String(filter.lte)}`,
      next: clear({ lte: undefined }),
    });
  }
  if (filter.empty !== undefined) {
    chips.push({
      key: `${family}_empty`,
      label: filter.empty ? `${label} is empty` : `${label} is set`,
      next: clear({ empty: undefined }),
    });
  }
}

function pushVocabChips(
  chips: Chip[],
  filters: FilterState,
  family: VocabFamily,
  label: string,
  resolve?: (token: string) => string,
): void {
  const set = filters[family];
  for (const mode of ["all", "any", "none"] as const) {
    const tokens = set[mode];
    if (tokens.length === 0) continue;
    const names = resolve ? tokens.map(resolve) : tokens;
    chips.push({
      key: `${family}_${mode}`,
      label: `${label} ${MODE_WORD[mode]}: ${names.join(", ")}`,
      next: { ...filters, [family]: { ...set, [mode]: [] } },
    });
  }
}
