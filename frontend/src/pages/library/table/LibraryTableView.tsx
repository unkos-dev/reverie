/**
 * Read-only table view of the library: the production face of the grid
 * adapter. Presentational per the container split; `LibraryContent` owns the
 * infinite query and hands loaded rows down. Paging is fetch-on-scroll over
 * the keyset cursor, so the grid always operates on the loaded window: the
 * scrollbar, Ctrl+End, and the row count all reflect rows fetched so far,
 * never a pretend 50K extent. Sorting is single-column and fixed-direction,
 * bounded by the list contract's sort modes.
 */
import { Loader2 } from "lucide-react";
import { useState, type ReactElement, type UIEvent } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";

import type { BookListItem, ListSort } from "@/api";
import { ReactDataGridBinding } from "@/lib/grid/ReactDataGridBinding";
import type { GridColumn, SortState } from "@/lib/grid/types";

import {
  GridShortcutsDialog,
  GridShortcutsTrigger,
  useShortcutsHotkey,
} from "./GridShortcutsDialog";

const EMPTY_CELL = "—";

/**
 * Sortable columns map onto the list contract's sort modes; direction is
 * server-fixed per mode, so the header indicator always shows ascending and
 * a second activation keeps the same order rather than reversing it.
 */
const SORT_BY_COLUMN: Partial<Record<string, ListSort>> = {
  title: "title",
  authors: "author",
};

const COLUMNS: readonly GridColumn<BookListItem>[] = [
  {
    key: "title",
    name: "Title",
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
  },
  {
    key: "subtitle",
    name: "Subtitle",
    sortable: false,
    accessor: (row) => row.subtitle ?? EMPTY_CELL,
  },
  {
    key: "authors",
    name: "Authors",
    sortable: true,
    accessor: (row) => (row.authors.length > 0 ? row.authors.join(", ") : EMPTY_CELL),
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
  },
  {
    key: "pages",
    name: "Pages",
    sortable: false,
    width: 80,
    accessor: (row) => (row.pages === null ? EMPTY_CELL : String(row.pages)),
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
  },
];

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
  sort: ListSort;
  onSortChange: (sort: ListSort) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
};

export function LibraryTableView({
  items,
  sort,
  onSortChange,
  hasNextPage,
  isFetchingNextPage,
  isFetchNextPageError,
  onLoadMore,
}: Props): ReactElement {
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  useShortcutsHotkey(setShortcutsOpen);

  const COLUMN_BY_SORT: Partial<Record<ListSort, string>> = {
    title: "title",
    author: "authors",
  };
  const sortColumn = COLUMN_BY_SORT[sort];
  const sortState: SortState =
    sortColumn === undefined ? null : { columnKey: sortColumn, direction: "asc" };

  function handleSortChange(next: SortState): void {
    if (next === null) {
      onSortChange("recent");
      return;
    }
    const mapped = SORT_BY_COLUMN[next.columnKey];
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
    <div data-testid="library-table">
      <div className="mb-2 flex items-center justify-end">
        <GridShortcutsTrigger onOpenChange={setShortcutsOpen} />
      </div>
      <ReactDataGridBinding<BookListItem>
        rows={items}
        columns={COLUMNS}
        label="Library books"
        sort={sortState}
        onSortChange={handleSortChange}
        onCellFocus={() => undefined}
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
