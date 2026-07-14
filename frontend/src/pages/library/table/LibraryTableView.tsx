/**
 * Editable table view of the library: the production face of the grid
 * adapter. Presentational per the container split for data fetching:
 * `LibraryContent` owns the infinite query and hands loaded rows plus the
 * query's cache key down. Cell-edit orchestration (`useCellEdit`) lives
 * here as the table's own concern instead, since it is scoped entirely to
 * this view's columns and has no bearing on how the page fetches rows.
 * Paging is fetch-on-scroll over the keyset cursor, so the grid always
 * operates on the loaded window: the scrollbar, Ctrl+End, and the row count
 * all reflect rows fetched so far, never a pretend 50K extent. Sorting is a
 * capped multi-level stack (see `MAX_SORT_LEVELS`): each level carries its
 * own direction, and a header click/ctrl-click writes the same stack the
 * filter rail's sort section edits, via `onSortChange`.
 */
import { Loader2, PanelRight } from "lucide-react";
import { useMemo, useState, type ReactElement, type ReactNode, type UIEvent } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";

import { MAX_SORT_LEVELS, type BookListItem, type SortField, type SortLevelParam } from "@/api";
import { ReactDataGridBinding } from "@/lib/grid/ReactDataGridBinding";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import type { BooksListKey } from "@/lib/query/keys";
import type { GridColumn, GridEditorProps, SortState } from "@/lib/grid/types";

import type { Density } from "../display-storage";
import { LOCKED_COLUMN_KEYS, MOBILE_COLUMN_KEYS, TABLE_MOBILE_MEDIA_QUERY } from "../table-columns";

import { AuthorsCellEditor } from "./editors/AuthorsCellEditor";
import { RatingCellEditor } from "./editors/RatingCellEditor";
import { StatusCellEditor } from "./editors/StatusCellEditor";
import { TextCellEditor } from "./editors/TextCellEditor";
import {
  GridShortcutsDialog,
  GridShortcutsTrigger,
  useShortcutsHotkey,
} from "./GridShortcutsDialog";
import { pendingKey, useCellEdit } from "./useCellEdit";

const EMPTY_CELL = "—";

/** Row heights per density; the header stays fixed across both. */
const ROW_HEIGHT_BY_DENSITY: Record<Density, number> = { comfortable: 64, compact: 44 };
const TABLE_HEADER_HEIGHT = 42;

/**
 * Header tooltip for a work-scoped column (title/subtitle/authors): a
 * committed edit fans out to every loaded sibling edition of the same work.
 * Carried as a hoverable, dismissable info control beside the header text.
 * The control's label reads as a suffix on the column's accessible name
 * ("Title (all editions)"), matching what the header previously spelled out
 * in visible text.
 */
const WORK_SCOPED_TOOLTIP = {
  label: "(all editions)",
  content: "Edits apply to all editions of the work",
};

const EMPTY_READING_STATE: NonNullable<BookListItem["reading_state"]> = {
  status: null,
  rating: null,
  progress_pct: null,
};

function renderTitleEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  return (
    <TextCellEditor
      value={editorProps.row.title}
      kind="text"
      required
      onDraft={(value) => {
        editorProps.update({ ...editorProps.row, title: value ?? editorProps.row.title });
      }}
    />
  );
}

function renderSubtitleEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  return (
    <TextCellEditor
      value={editorProps.row.subtitle}
      kind="text"
      onDraft={(value) => {
        editorProps.update({ ...editorProps.row, subtitle: value });
      }}
    />
  );
}

function renderIsbnEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  return (
    <TextCellEditor
      value={editorProps.row.isbn_13}
      kind="text"
      onDraft={(value) => {
        editorProps.update({ ...editorProps.row, isbn_13: value });
      }}
    />
  );
}

function renderPagesEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  const { pages } = editorProps.row;
  return (
    <TextCellEditor
      value={pages === null ? null : String(pages)}
      kind="positive-int"
      onDraft={(value) => {
        editorProps.update({
          ...editorProps.row,
          pages: value === null ? null : Number(value),
        });
      }}
    />
  );
}

function renderAuthorsEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  return (
    <AuthorsCellEditor
      authors={editorProps.row.authors}
      onCommit={(authors) => {
        editorProps.commit({ ...editorProps.row, authors });
      }}
      onCancel={editorProps.cancel}
    />
  );
}

function renderStatusEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  const readingState = editorProps.row.reading_state ?? EMPTY_READING_STATE;
  return (
    <StatusCellEditor
      value={readingState.status}
      onCommit={(status) => {
        editorProps.commit({ ...editorProps.row, reading_state: { ...readingState, status } });
      }}
    />
  );
}

function renderRatingEditCell(editorProps: GridEditorProps<BookListItem>): ReactElement {
  const readingState = editorProps.row.reading_state ?? EMPTY_READING_STATE;
  return (
    <RatingCellEditor
      value={readingState.rating}
      onCommit={(rating) => {
        editorProps.commit({ ...editorProps.row, reading_state: { ...readingState, rating } });
      }}
    />
  );
}

/**
 * Wire sort field to grid column key. Two fields diverge from their column
 * key: `author` sources the `authors` column and `created_at` the `added`
 * column. Every sortable field must map to a real column key here, or RDG
 * silently drops a sort level whose key names no column.
 */
const COLUMN_BY_FIELD: Record<SortField, string> = {
  title: "title",
  author: "authors",
  created_at: "added",
  pages: "pages",
};

/**
 * Grid column key to wire sort field, the reverse of {@link COLUMN_BY_FIELD}.
 * Derived rather than hand-maintained so the two maps cannot drift; lossless
 * because every value in `COLUMN_BY_FIELD` is distinct. `handleSortChange`
 * reads this to translate RDG's column-keyed sort back to wire fields.
 */
const FIELD_BY_COLUMN: Partial<Record<string, SortField>> = Object.fromEntries(
  (Object.entries(COLUMN_BY_FIELD) as [SortField, string][]).map(
    ([field, column]) => [column, field] as const,
  ),
);

/** Renders the `created_at` RFC 3339 timestamp as a locale date for the
 *  read-only "Added" column and its plain-text export projection. */
function formatAddedDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    console.error("[LibraryTableView] unparsable created_at", iso);
    return EMPTY_CELL;
  }
  return date.toLocaleDateString();
}

/**
 * Base column defs: accessor projections, sort wiring, and per-column
 * editors. `editable`/`renderCell` are layered on top per render in
 * {@link useEditableColumns} once pending-cell state exists to gate them;
 * `accessor` here stays the export-safe plain-text projection either way.
 */
const BASE_COLUMNS: readonly GridColumn<BookListItem>[] = [
  {
    key: "title",
    name: "Title",
    headerTooltip: WORK_SCOPED_TOOLTIP,
    sortable: true,
    accessor: (row) => row.title,
    // renderCell is layered per render (cover mark, mobile author stack)
    // in `composeColumns`; the accessor stays the plain-text projection.
    renderEditCell: renderTitleEditCell,
  },
  {
    key: "subtitle",
    name: "Subtitle",
    headerTooltip: WORK_SCOPED_TOOLTIP,
    sortable: false,
    accessor: (row) => row.subtitle ?? EMPTY_CELL,
    renderEditCell: renderSubtitleEditCell,
  },
  {
    key: "authors",
    name: "Authors",
    headerTooltip: WORK_SCOPED_TOOLTIP,
    sortable: true,
    accessor: (row) => (row.authors.length > 0 ? row.authors.join(", ") : EMPTY_CELL),
    renderEditCell: renderAuthorsEditCell,
    editorOptions: { commitOnOutsideClick: false },
  },
  {
    key: "series",
    name: "Series",
    sortable: false,
    accessor: (row) => {
      if (row.series === null) return EMPTY_CELL;
      if (row.series.position === null) return row.series.name;
      return `${row.series.name} · #${String(row.series.position)}`;
    },
  },
  {
    key: "isbn_13",
    name: "ISBN",
    sortable: false,
    width: 140,
    accessor: (row) => row.isbn_13 ?? EMPTY_CELL,
    renderEditCell: renderIsbnEditCell,
  },
  {
    key: "pages",
    name: "Pages",
    sortable: true,
    width: 80,
    accessor: (row) => (row.pages === null ? EMPTY_CELL : String(row.pages)),
    renderEditCell: renderPagesEditCell,
  },
  {
    key: "status",
    name: "Status",
    sortable: false,
    width: 120,
    accessor: (row) => {
      const status = row.reading_state?.status ?? null;
      return status === null ? EMPTY_CELL : status.replaceAll("_", " ");
    },
    renderEditCell: renderStatusEditCell,
  },
  {
    key: "rating",
    name: "Rating",
    sortable: false,
    width: 90,
    accessor: (row) => {
      const rating = row.reading_state?.rating ?? null;
      // Guarding below 1 here rather than in the zod schema: a schema bound
      // would fail the whole page parse over one bad row, while this degrades
      // a single cell. repeat() throws on negative counts.
      return rating === null || rating < 1 ? EMPTY_CELL : "★".repeat(rating);
    },
    renderEditCell: renderRatingEditCell,
  },
  {
    key: "added",
    name: "Added",
    sortable: true,
    width: 120,
    accessor: (row) => formatAddedDate(row.created_at),
  },
];

/**
 * Cover slot for the title cell: the thumbnail when art exists, otherwise
 * a typographic mark tile (first letters of the title) per the redesign's
 * ledger treatment. A thumbnail that fails to load falls back to the mark
 * too, mirroring the grid card's spine fallback. Hidden in compact
 * density, where the row is too short to carry it.
 */
function TitleCellMark({ row }: { row: BookListItem }): ReactElement {
  const [coverFailed, setCoverFailed] = useState(false);
  if (row.cover_url !== "" && !coverFailed) {
    return (
      <img
        src={row.cover_url}
        alt=""
        loading="lazy"
        decoding="async"
        onError={() => {
          setCoverFailed(true);
        }}
        className="border-border h-[42px] w-[30px] shrink-0 border object-cover"
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      className="border-accent text-accent-text bg-surface-1 font-display flex h-[42px] w-[30px] shrink-0 items-center justify-center border text-[0.5rem] leading-none tracking-tight"
    >
      {row.title.slice(0, 3).toUpperCase()}
    </span>
  );
}

/**
 * Composes the render-time column set from the base defs: applies the
 * user's hidden-column preference (desktop) or the fixed mobile trim,
 * layers the rich title cell (cover mark, link, mobile author stack), and
 * prepends the details column: the guaranteed drawer path that survives
 * any visibility or density configuration.
 */
function composeColumns(
  hiddenColumns: ReadonlySet<string>,
  density: Density,
  isMobile: boolean,
  onRowActivate: (book: BookListItem) => void,
): readonly GridColumn<BookListItem>[] {
  const visible = BASE_COLUMNS.filter((col) => {
    if (LOCKED_COLUMN_KEYS.has(col.key)) return true;
    if (isMobile) return MOBILE_COLUMN_KEYS.has(col.key);
    return !hiddenColumns.has(col.key);
  });
  const withTitleCell = visible.map((col) => {
    if (col.key !== "title") return col;
    const renderCell = (row: BookListItem): ReactNode => (
      <div className="flex min-w-0 items-center gap-3">
        {density === "comfortable" ? <TitleCellMark row={row} /> : null}
        <div className="min-w-0">
          {/* min-h-6 keeps the link's hit target at the 24px WCAG 2.2 AA
              floor; an inline anchor's line box alone falls under it. */}
          <Link
            to={`/b/${row.id}`}
            className="text-fg hover:text-accent inline-flex min-h-6 max-w-full items-center truncate font-medium"
          >
            {row.title}
          </Link>
          {isMobile && density === "comfortable" ? (
            <div className="text-fg-muted truncate text-xs">
              {row.authors.length > 0 ? row.authors.join(", ") : EMPTY_CELL}
            </div>
          ) : null}
        </div>
      </div>
    );
    return { ...col, renderCell };
  });
  const detailsColumn: GridColumn<BookListItem> = {
    key: "details",
    name: "",
    sortable: false,
    width: 44,
    accessor: () => "",
    renderCell: (row) => (
      <button
        type="button"
        aria-label={`Open details for ${row.title}`}
        onClick={() => {
          onRowActivate(row);
        }}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent flex min-h-6 min-w-6 items-center justify-center rounded-sm focus-visible:outline-none focus-visible:ring-2"
      >
        <PanelRight className="size-4" aria-hidden="true" />
      </button>
    ),
  };
  return [detailsColumn, ...withTitleCell];
}

/**
 * Layers pending-edit state onto the composed columns: an editable column
 * is blocked from re-opening its editor while its own commit is in flight,
 * and renders the in-flight draft (muted, `aria-busy`) instead of the
 * cached value until the server confirms. Memoized so a fresh array does
 * not invalidate the binding's own `toRdgColumns` memo on every unrelated
 * re-render (scroll, sort) at up to 50K loaded rows.
 */
function useComposedColumns(
  pendingCells: ReadonlyMap<string, string>,
  hiddenColumns: ReadonlySet<string>,
  density: Density,
  isMobile: boolean,
  onRowActivate: (book: BookListItem) => void,
): readonly GridColumn<BookListItem>[] {
  return useMemo(
    () =>
      composeColumns(hiddenColumns, density, isMobile, onRowActivate).map((col) => {
        if (col.renderEditCell === undefined) return col;
        const renderFinal = col.renderCell ?? col.accessor;
        const renderCell = (row: BookListItem): ReactNode => {
          const draft = pendingCells.get(pendingKey(row.id, col.key));
          if (draft === undefined) return renderFinal(row);
          return (
            <span aria-busy="true" className="text-fg-muted italic">
              {draft}
            </span>
          );
        };
        return {
          ...col,
          editable: (row: BookListItem) => !pendingCells.has(pendingKey(row.id, col.key)),
          renderCell,
        };
      }),
    // onRowActivate is a stable prop-drilled callback identity-wise only if
    // the container memoizes it; include it so a changed handler is honored.
    [pendingCells, hiddenColumns, density, isMobile, onRowActivate],
  );
}

/**
 * Near-bottom detector from the grid vendor's own infinite-scroll recipe.
 * The 10px slack absorbs subpixel scroll rounding; without it the boundary
 * check can sit forever a fraction of a pixel short and never fire.
 */
function isAtBottom({ currentTarget }: UIEvent<HTMLDivElement>): boolean {
  return currentTarget.scrollTop + 10 >= currentTarget.scrollHeight - currentTarget.clientHeight;
}

type Props = {
  items: readonly BookListItem[];
  sort: readonly SortLevelParam[];
  onSortChange: (levels: readonly SortLevelParam[]) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
  /** Exact key of the page's suspense list query (cursor stripped); cell
   *  edits patch this same cache slot rather than triggering a refetch. */
  listQueryKey: BooksListKey;
  /** Row density; drives row height and whether the cover mark renders. */
  density: Density;
  /** User-hidden column keys; locked columns are exempt (see LOCKED_COLUMN_KEYS). */
  hiddenColumns: ReadonlySet<string>;
  /** Selected book ids; selection state is owned by the container. */
  selectedIds: ReadonlySet<string>;
  onSelectionChange: (ids: ReadonlySet<string>) => void;
  /** Opens the book-detail drawer for a row. */
  onRowActivate: (book: BookListItem) => void;
};

export function LibraryTableView({
  items,
  sort,
  onSortChange,
  hasNextPage,
  isFetchingNextPage,
  isFetchNextPageError,
  onLoadMore,
  listQueryKey,
  density,
  hiddenColumns,
  selectedIds,
  onSelectionChange,
  onRowActivate,
}: Props): ReactElement {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  useShortcutsHotkey(setShortcutsOpen);
  const isMobile = useMediaQuery(TABLE_MOBILE_MEDIA_QUERY);
  const { pendingCells, onCellEdit, onGridKeyDown } = useCellEdit({
    listKey: listQueryKey,
    columns: BASE_COLUMNS,
  });
  const columns = useComposedColumns(pendingCells, hiddenColumns, density, isMobile, onRowActivate);

  const sortState: SortState = sort.map((level) => ({
    columnKey: COLUMN_BY_FIELD[level.field],
    direction: level.desc ? "desc" : "asc",
  }));

  function handleSortChange(next: SortState): void {
    const levels: SortLevelParam[] = [];
    for (const entry of next) {
      const field = FIELD_BY_COLUMN[entry.columnKey];
      if (field === undefined) {
        console.error("[LibraryTableView] sortable column has no wire mapping", entry.columnKey);
        continue;
      }
      levels.push({ field, desc: entry.direction === "desc" });
    }
    onSortChange(levels.slice(0, MAX_SORT_LEVELS));
  }

  function handleScroll(event: UIEvent<HTMLDivElement>): void {
    // After a failed page fetch the explicit Retry control is the only
    // refire path; without the error gate, wheel movement inside the
    // bottom slack would hammer a failing endpoint on every scroll event.
    if (!hasNextPage || isFetchingNextPage || isFetchNextPageError || !isAtBottom(event)) return;
    onLoadMore();
  }

  return (
    // onKeyDown here (not window) is the Ctrl+Z undo trigger: it only fires
    // while focus sits somewhere inside the table, and bubbles up from the
    // grid's own cells and any open editor without needing a ref.
    <div data-testid="library-table" onKeyDown={onGridKeyDown}>
      <div className="mb-2 flex items-center justify-end gap-4">
        <GridShortcutsTrigger onOpenChange={setShortcutsOpen} />
      </div>
      {/* Sort-stack announcements come from LibraryPage's always-mounted
          live region; the grid carries no sort-summary association of its
          own. */}
      <ReactDataGridBinding<BookListItem>
        rows={items}
        columns={columns}
        label="Library books"
        sort={sortState}
        onSortChange={handleSortChange}
        onCellFocus={() => undefined}
        onCellEdit={onCellEdit}
        onRowActivate={onRowActivate}
        selection={{
          selectedKeys: selectedIds,
          onSelectionChange,
          // Select-all covers loaded rows only (keyset paging has no "all
          // matching the filter" client-side); the label says so.
          selectAllLabel: "Select all loaded books",
        }}
        onScroll={handleScroll}
        rowKey={(row) => row.id}
        rowHeight={ROW_HEIGHT_BY_DENSITY[density]}
        headerRowHeight={TABLE_HEADER_HEIGHT}
        className="h-[calc(100dvh-22rem)] min-h-96"
      />
      {/* Single owner of every paging state in table mode; the page-level
          Load-more block stays out of table view so a failure or fetch is
          announced exactly once. */}
      <div className="text-fg-muted mt-3 flex min-h-6 items-center justify-center gap-2 text-sm">
        <PagingFooter
          loadedCount={items.length}
          hasNextPage={hasNextPage}
          isFetchingNextPage={isFetchingNextPage}
          isFetchNextPageError={isFetchNextPageError}
          onLoadMore={onLoadMore}
        />
      </div>
      <GridShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
    </div>
  );
}

type PagingFooterProps = {
  loadedCount: number;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
};

/**
 * The table's one paging status line. Branch order is the precedence: an
 * in-flight fetch displaces the error state until it settles.
 */
function PagingFooter({
  loadedCount,
  hasNextPage,
  isFetchingNextPage,
  isFetchNextPageError,
  onLoadMore,
}: PagingFooterProps): ReactElement | null {
  if (isFetchingNextPage) {
    return (
      <output className="flex items-center gap-2">
        <Loader2 className="size-4 animate-spin" aria-hidden="true" />
        Loading more…
      </output>
    );
  }
  if (isFetchNextPageError) {
    return (
      <span role="alert">
        Couldn&apos;t load more books.{" "}
        <button type="button" className="hover:text-accent underline" onClick={onLoadMore}>
          Retry
        </button>
      </span>
    );
  }
  if (hasNextPage) {
    // Scroll is the primary paging path; the button is the escape hatch when
    // the viewport is too tall for a scrollbar, and the keyboard-reachable
    // path either way.
    return (
      <Button type="button" variant="outline" size="sm" onClick={onLoadMore}>
        Load more
      </Button>
    );
  }
  if (loadedCount > 0) return <span>{loadedCount} books loaded · end of list</span>;
  return null;
}
