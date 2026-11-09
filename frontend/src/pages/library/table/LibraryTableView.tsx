/**
 * Editable table view of the library, rendered directly on react-data-grid
 * with its own column, editor, sort and selection types. Presentational per
 * the container split for data fetching:
 * `LibraryContent` owns the infinite query and hands loaded rows plus the
 * editing owner down. The Library page owns edits above conditional results.
 * Paging is fetch-on-scroll over the keyset cursor, so the grid always
 * operates on the loaded window: the scrollbar, Ctrl+End, and the row count
 * all reflect rows fetched so far, never a pretend 50K extent. Sorting is a
 * capped multi-level stack (see `MAX_SORT_LEVELS`): each level carries its
 * own direction, and a header click/ctrl-click writes the same stack the
 * filter rail's sort section edits, via `onSortChange`.
 */
import { Loader2, PanelRight } from "lucide-react";
import {
  useMemo,
  useState,
  type Key,
  type ReactElement,
  type ReactNode,
  type UIEvent,
} from "react";
import {
  Cell,
  DataGrid,
  SelectColumn,
  renderCheckbox as renderRdgCheckbox,
  renderSortIcon,
  renderValue,
  type Column,
  type RenderCellContentProps,
  type RenderCellProps,
  type RenderCheckboxProps,
  type RenderEditCellContentProps,
  type RenderSortStatusProps,
  type SortColumn,
} from "react-data-grid";
import "react-data-grid/lib/styles.css";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";

import {
  MAX_SORT_LEVELS,
  type BookListItem,
  type Density,
  type SortField,
  type SortLevelParam,
} from "@/api";
import "@/lib/grid/grid-theme.css";
import { useMediaQuery } from "@/lib/hooks/use-media-query";

import { LOCKED_COLUMN_KEYS, MOBILE_COLUMN_KEYS, TABLE_MOBILE_MEDIA_QUERY } from "../table-columns";

import { AuthorsCellEditor } from "./editors/AuthorsCellEditor";
import { RatingCellEditor } from "./editors/RatingCellEditor";
import { StatusCellEditor } from "./editors/StatusCellEditor";
import { TextCellEditor } from "./editors/TextCellEditor";
import { pendingKey, type UseCellEditResult } from "./useCellEdit";

const EMPTY_CELL = "—";

/** Row heights per density; the header stays fixed across both. */
const ROW_HEIGHT_BY_DENSITY: Record<Density, number> = { comfortable: 64, compact: 44 };
const TABLE_HEADER_HEIGHT = 42;

const EMPTY_READING_STATE: NonNullable<BookListItem["reading_state"]> = {
  status: null,
  rating: null,
  progress_pct: null,
  started_at: null,
  finished_at: null,
};

function renderTitleEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  return (
    <>
      <TextCellEditor
        value={row.title}
        kind="text"
        required
        onDraft={(value) => {
          onRowChange({ ...row, title: value ?? row.title });
        }}
      />
    </>
  );
}

function renderSubtitleEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  return (
    <>
      <TextCellEditor
        value={row.subtitle}
        kind="text"
        onDraft={(value) => {
          onRowChange({ ...row, subtitle: value });
        }}
      />
    </>
  );
}

function renderIsbnEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  return (
    <>
      <TextCellEditor
        value={row.isbn_13}
        kind="text"
        onDraft={(value) => {
          onRowChange({ ...row, isbn_13: value });
        }}
      />
    </>
  );
}

function renderPagesEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  const { pages } = row;
  return (
    <>
      <TextCellEditor
        value={pages === null ? null : String(pages)}
        kind="positive-int"
        onDraft={(value) => {
          onRowChange({
            ...row,
            pages: value === null ? null : Number(value),
          });
        }}
      />
    </>
  );
}

function renderAuthorsEditCell({
  row,
  onRowChange,
  onClose,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  return (
    <>
      <AuthorsCellEditor
        authors={row.authors}
        onCommit={(authors) => {
          onRowChange({ ...row, authors }, true);
        }}
        onCancel={onClose}
      />
    </>
  );
}

function renderStatusEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  const readingState = row.reading_state ?? EMPTY_READING_STATE;
  return (
    <>
      <StatusCellEditor
        value={readingState.status}
        onCommit={(status) => {
          onRowChange({ ...row, reading_state: { ...readingState, status } }, true);
        }}
      />
    </>
  );
}

function renderRatingEditCell({
  row,
  onRowChange,
}: RenderEditCellContentProps<BookListItem>): ReactElement {
  const readingState = row.reading_state ?? EMPTY_READING_STATE;
  return (
    <>
      <RatingCellEditor
        value={readingState.rating}
        onCommit={(rating) => {
          onRowChange({ ...row, reading_state: { ...readingState, rating } }, true);
        }}
      />
    </>
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
 * because every value in `COLUMN_BY_FIELD` is distinct. `handleSortColumnsChange`
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
 * Base column defs: cell text, sort wiring, and per-column editors.
 * `editable` and the pending-draft `renderCell` are layered on top per
 * render in {@link useComposedColumns} once pending-cell state exists to
 * gate them.
 */
const BASE_COLUMNS: readonly Column<BookListItem>[] = [
  {
    key: "title",
    name: "Title",
    sortable: true,
    renderCell: ({ row }) => row.title,
    renderEditCell: renderTitleEditCell,
  },
  {
    key: "subtitle",
    name: "Subtitle",
    sortable: false,
    renderCell: ({ row }) => row.subtitle ?? EMPTY_CELL,
    renderEditCell: renderSubtitleEditCell,
  },
  {
    key: "authors",
    name: "Authors",
    sortable: true,
    renderCell: ({ row }) => (row.authors.length > 0 ? row.authors.join(", ") : EMPTY_CELL),
    renderEditCell: renderAuthorsEditCell,
    editorOptions: { commitOnOutsideClick: false },
  },
  {
    key: "series",
    name: "Series",
    sortable: false,
    renderCell: ({ row }) => {
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
    renderCell: ({ row }) => row.isbn_13 ?? EMPTY_CELL,
    renderEditCell: renderIsbnEditCell,
  },
  {
    key: "pages",
    name: "Pages",
    sortable: true,
    width: 80,
    renderCell: ({ row }) => (row.pages === null ? EMPTY_CELL : String(row.pages)),
    renderEditCell: renderPagesEditCell,
  },
  {
    key: "status",
    name: "Status",
    sortable: false,
    width: 120,
    renderCell: ({ row }) => {
      const status = row.reading_state?.status ?? null;
      return status === null ? EMPTY_CELL : status.replaceAll("_", " ");
    },
    renderEditCell: renderStatusEditCell,
  },
  {
    key: "rating",
    name: "Rating",
    sortable: false,
    width: 173,
    renderCell: ({ row }) => {
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
    renderCell: ({ row }) => formatAddedDate(row.created_at),
  },
];

/**
 * Cover slot for the title cell: the thumbnail when art exists, otherwise
 * a typographic mark tile (first letters of the title) per the redesign's
 * ledger treatment. A thumbnail that fails to load falls back to the mark
 * too, mirroring the grid card's spine fallback. Hidden in compact
 * density, where the row is too short to carry it.
 */
function TitleCellMark({ row }: Readonly<{ row: BookListItem }>): ReactElement {
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
 * starts each column the installation default sorts descending at
 * descending, layers the rich title cell (cover mark, link, mobile author
 * stack), and prepends the selection column and the details column: the
 * guaranteed drawer path that survives any visibility or density
 * configuration.
 */
function composeColumns(
  hiddenColumns: ReadonlySet<string>,
  density: Density,
  isMobile: boolean,
  defaultSort: readonly SortLevelParam[],
  onRowActivate: (book: BookListItem) => void,
): readonly Column<BookListItem>[] {
  const descendingFirst = new Set(
    defaultSort.filter((level) => level.desc).map((level) => COLUMN_BY_FIELD[level.field]),
  );
  const visible = BASE_COLUMNS.filter((col) => {
    if (LOCKED_COLUMN_KEYS.has(col.key)) return true;
    if (isMobile) return MOBILE_COLUMN_KEYS.has(col.key);
    return !hiddenColumns.has(col.key);
  });
  const withTitleCell = visible.map((col): Column<BookListItem> => {
    const base = descendingFirst.has(col.key) ? { ...col, sortDescendingFirst: true } : col;
    if (col.key !== "title") return base;
    const renderCell = ({ row, tabIndex }: RenderCellContentProps<BookListItem>): ReactNode => (
      <div className="flex min-w-0 items-center gap-3">
        {density === "comfortable" ? <TitleCellMark row={row} /> : null}
        <div className="min-w-0">
          {/* min-h-6 keeps the link's hit target at the 24px WCAG 2.2 AA
              floor; an inline anchor's line box alone falls under it. */}
          <Link
            to={`/b/${row.id}`}
            tabIndex={tabIndex}
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
    return { ...base, renderCell };
  });
  const detailsColumn: Column<BookListItem> = {
    key: "details",
    name: "",
    sortable: false,
    width: 44,
    renderCell: ({ row, tabIndex }) => (
      <Button
        type="button"
        excludeFromTabOrder={tabIndex === -1}
        variant="ghost"
        size="icon-xs"
        aria-label={`Open details for ${row.title}`}
        onPress={() => {
          onRowActivate(row);
        }}
        className="text-fg-muted hover:text-fg focus-visible:ring-accent flex min-h-6 min-w-6 items-center justify-center rounded-sm focus-visible:outline-none focus-visible:ring-2"
      >
        <PanelRight className="size-4" aria-hidden="true" />
      </Button>
    ),
  };
  return [SelectColumn, detailsColumn, ...withTitleCell];
}

/**
 * Layers pending-edit state onto the composed columns: an editable column
 * is blocked from re-opening its editor while its own commit is in flight,
 * and renders the in-flight draft (muted, `aria-busy`) instead of the
 * cached value until the server confirms. Memoized so a fresh array does
 * not make RDG recompute its columns on every unrelated re-render (scroll,
 * sort) at up to 50K loaded rows.
 */
function useComposedColumns(
  pendingCells: ReadonlyMap<string, string>,
  hiddenColumns: ReadonlySet<string>,
  density: Density,
  isMobile: boolean,
  defaultSort: readonly SortLevelParam[],
  onRowActivate: (book: BookListItem) => void,
): readonly Column<BookListItem>[] {
  return useMemo(
    () =>
      composeColumns(hiddenColumns, density, isMobile, defaultSort, onRowActivate).map((col) => {
        if (col.renderEditCell == null) return col;
        const renderFinal = col.renderCell ?? renderValue;
        const renderCell = (props: RenderCellContentProps<BookListItem>): ReactNode => {
          const draft = pendingCells.get(pendingKey(props.row.id, col.key));
          if (draft === undefined) return renderFinal(props);
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
    [pendingCells, hiddenColumns, density, isMobile, defaultSort, onRowActivate],
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

/**
 * Overrides RDG's default sort-status renderer to add an accessible label
 * next to the bare priority number: the vendor's own render leaves the
 * digit unlabeled, so a screen reader announces "2" with no indication
 * it names a sort priority.
 */
function renderSortStatus({ sortDirection, priority }: RenderSortStatusProps): ReactNode {
  return (
    <>
      {renderSortIcon({ sortDirection })}
      {priority === undefined ? null : (
        <span>
          {priority}
          <span className="sr-only">{` sort level ${String(priority)}`}</span>
        </span>
      )}
    </>
  );
}

/**
 * RDG's checkbox with the header select-all control renamed. Select-all
 * covers loaded rows only (keyset paging has no "all matching the filter"
 * client-side), and the label says so. The header instance is the only one
 * handed `indeterminate`.
 */
function renderCheckbox(props: RenderCheckboxProps): ReactNode {
  return renderRdgCheckbox(
    props.indeterminate === undefined
      ? props
      : { ...props, "aria-label": "Select all loaded books" },
  );
}

function renderCell<R>(key: Key, props: RenderCellProps<R, unknown>): ReactNode {
  return (
    <Cell
      key={key}
      {...props}
      // React Aria's usePress stops left-button mousedown, so RDG never activates the cell of a pressed cell button.
      onMouseDownCapture={(event) => {
        if (
          event.button !== 0 ||
          !(event.target instanceof Element) ||
          event.target.closest("button") === null
        )
          return;
        props.setActivePosition({ rowIdx: props.rowIdx, idx: props.column.idx });
      }}
    />
  );
}

type Props = {
  items: readonly BookListItem[];
  sort: readonly SortLevelParam[];
  /** The installation default sort stack; a column it sorts descending
   *  starts descending on its first header press. */
  defaultSort: readonly SortLevelParam[];
  onSortChange: (levels: readonly SortLevelParam[]) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
  /** Exact key of the page's suspense list query (cursor stripped); cell
   *  edits patch this same cache slot rather than triggering a refetch. */
  edits: UseCellEditResult;
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
  defaultSort,
  onSortChange,
  hasNextPage,
  isFetchingNextPage,
  isFetchNextPageError,
  onLoadMore,
  edits,
  density,
  hiddenColumns,
  selectedIds,
  onSelectionChange,
  onRowActivate,
}: Readonly<Props>): ReactElement {
  const isMobile = useMediaQuery(TABLE_MOBILE_MEDIA_QUERY);
  const { pendingCells, onCellEdit, keyboardProps } = edits;
  const columns = useComposedColumns(
    pendingCells,
    hiddenColumns,
    density,
    isMobile,
    defaultSort,
    onRowActivate,
  );

  const sortColumns: readonly SortColumn[] = sort.map((level) => ({
    columnKey: COLUMN_BY_FIELD[level.field],
    direction: level.desc ? "DESC" : "ASC",
  }));

  function handleSortColumnsChange(next: SortColumn[]): void {
    const levels: SortLevelParam[] = [];
    for (const entry of next) {
      const field = FIELD_BY_COLUMN[entry.columnKey];
      if (field === undefined) {
        console.error("[LibraryTableView] sortable column has no wire mapping", entry.columnKey);
        continue;
      }
      levels.push({ field, desc: entry.direction === "DESC" });
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
    <div data-testid="library-table" {...keyboardProps}>
      {/* Sort-stack announcements come from LibraryPage's always-mounted
          live region; the grid carries no sort-summary association of its
          own. */}
      <div className="rv-grid h-[calc(100dvh-22rem)] min-h-96">
        <DataGrid
          aria-label="Library books"
          columns={columns}
          rows={items}
          getRowKey={(row) => row.id}
          sortColumns={sortColumns}
          onSortColumnsChange={handleSortColumnsChange}
          onRowsChange={(nextRows, { indexes, column }) => {
            const index = indexes[0];
            onCellEdit({ row: nextRows[index], previousRow: items[index], columnKey: column.key });
          }}
          onScroll={handleScroll}
          selectedRows={selectedIds}
          onSelectedRowsChange={onSelectionChange}
          rowHeight={ROW_HEIGHT_BY_DENSITY[density]}
          headerRowHeight={TABLE_HEADER_HEIGHT}
          renderers={{ renderCell, renderSortStatus, renderCheckbox }}
        />
      </div>
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
}: Readonly<PagingFooterProps>): ReactElement | null {
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
        <Button
          type="button"
          variant="link"
          size="xs"
          className="h-auto p-0 hover:text-accent underline"
          onPress={onLoadMore}
        >
          Retry
        </Button>
      </span>
    );
  }
  if (hasNextPage) {
    // Scroll is the primary paging path; the button is the escape hatch when
    // the viewport is too tall for a scrollbar, and the keyboard-reachable
    // path either way.
    return (
      <Button type="button" variant="outline" size="sm" onPress={onLoadMore}>
        Load more
      </Button>
    );
  }
  if (loadedCount > 0) return <span>{loadedCount} books loaded · end of list</span>;
  return null;
}
