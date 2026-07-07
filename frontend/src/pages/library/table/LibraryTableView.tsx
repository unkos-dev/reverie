/**
 * Editable table view of the library: the production face of the grid
 * adapter. Presentational per the container split for data fetching:
 * `LibraryContent` owns the infinite query and hands loaded rows plus the
 * query's cache key down. Cell-edit orchestration (`useCellEdit`) lives
 * here as the table's own concern instead, since it is scoped entirely to
 * this view's columns and has no bearing on how the page fetches rows.
 * Paging is fetch-on-scroll over the keyset cursor, so the grid always
 * operates on the loaded window: the scrollbar, Ctrl+End, and the row count
 * all reflect rows fetched so far, never a pretend 50K extent. Sorting is
 * single-column and fixed-direction, bounded by the list contract's sort
 * modes.
 */
import { Loader2 } from "lucide-react";
import { useMemo, useState, type ReactElement, type ReactNode, type UIEvent } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";

import type { BookListItem } from "@/api";
import { ReactDataGridBinding } from "@/lib/grid/ReactDataGridBinding";
import type { BooksListKey } from "@/lib/query/keys";
import type { GridColumn, GridEditorProps, SortState } from "@/lib/grid/types";

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

/**
 * Suffix marking a work-scoped column header (title/subtitle/authors): a
 * committed edit fans out to every loaded sibling edition of the same work.
 * The grid contract's `name` is a plain string rendered as the column
 * header's accessible name, with no separate slot for a tooltip attribute
 * (that would need a contract change outside this column's ownership). A
 * plain-language suffix reads the same for a sighted user scanning the
 * header row and a screen reader announcing it, rather than a glyph that
 * would need the tooltip to explain itself.
 */
const WORK_SCOPED_SUFFIX = " (all editions)";

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
 * Sortable columns map onto the list contract's sort modes; direction is
 * server-fixed per mode, so the header indicator always shows ascending and
 * a second activation keeps the same order rather than reversing it.
 */
const SORT_BY_COLUMN: Partial<Record<string, string>> = {
  title: "title",
  authors: "author",
};

/**
 * Base column defs: accessor projections, sort wiring, and per-column
 * editors. `editable`/`renderCell` are layered on top per render in
 * {@link useEditableColumns} once pending-cell state exists to gate them;
 * `accessor` here stays the export-safe plain-text projection either way.
 */
const BASE_COLUMNS: readonly GridColumn<BookListItem>[] = [
  {
    key: "title",
    name: `Title${WORK_SCOPED_SUFFIX}`,
    sortable: true,
    accessor: (row) => row.title,
    renderCell: (row) => (
      // min-h-6 keeps the link's hit target at the 24px WCAG 2.2 AA floor;
      // an inline anchor's line box alone falls under it.
      <Link
        to={`/b/${row.id}`}
        className="text-fg hover:text-accent inline-flex min-h-6 items-center font-medium"
      >
        {row.title}
      </Link>
    ),
    renderEditCell: renderTitleEditCell,
  },
  {
    key: "subtitle",
    name: `Subtitle${WORK_SCOPED_SUFFIX}`,
    sortable: false,
    accessor: (row) => row.subtitle ?? EMPTY_CELL,
    renderEditCell: renderSubtitleEditCell,
  },
  {
    key: "authors",
    name: `Authors${WORK_SCOPED_SUFFIX}`,
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
    sortable: false,
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
];

/**
 * Layers pending-edit state onto {@link BASE_COLUMNS}: an editable column is
 * blocked from re-opening its editor while its own commit is in flight, and
 * renders the in-flight draft (muted, `aria-busy`) instead of the cached
 * value until the server confirms. Memoized on `pendingCells` alone: column
 * identity otherwise never changes, and a fresh array every render would
 * invalidate the binding's own `toRdgColumns` memo on every unrelated
 * re-render (scroll, sort) at up to 50K loaded rows.
 */
function useEditableColumns(
  pendingCells: ReadonlyMap<string, string>,
): readonly GridColumn<BookListItem>[] {
  return useMemo(
    () =>
      BASE_COLUMNS.map((col) => {
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
    [pendingCells],
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
  sort: string;
  onSortChange: (sort: string) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
  /** Exact key of the page's suspense list query (cursor stripped); cell
   *  edits patch this same cache slot rather than triggering a refetch. */
  listQueryKey: BooksListKey;
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
}: Props): ReactElement {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  useShortcutsHotkey(setShortcutsOpen);
  const { pendingCells, onCellEdit, onGridKeyDown } = useCellEdit({
    listKey: listQueryKey,
    columns: BASE_COLUMNS,
  });
  const columns = useEditableColumns(pendingCells);

  const COLUMN_BY_SORT: Partial<Record<string, string>> = {
    title: "title",
    author: "authors",
  };
  const sortColumn = COLUMN_BY_SORT[sort];
  const sortState: SortState =
    sortColumn === undefined ? [] : [{ columnKey: sortColumn, direction: "asc" }];

  function handleSortChange(next: SortState): void {
    if (next.length === 0) {
      onSortChange("recent");
      return;
    }
    const mapped = SORT_BY_COLUMN[next[0].columnKey];
    onSortChange(mapped ?? "recent");
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
      <div className="mb-2 flex items-center justify-end">
        <GridShortcutsTrigger onOpenChange={setShortcutsOpen} />
      </div>
      <ReactDataGridBinding<BookListItem>
        rows={items}
        columns={columns}
        label="Library books"
        sort={sortState}
        onSortChange={handleSortChange}
        onCellFocus={() => undefined}
        onCellEdit={onCellEdit}
        onScroll={handleScroll}
        rowKey={(row) => row.id}
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
