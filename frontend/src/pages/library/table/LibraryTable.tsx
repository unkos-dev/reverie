import { ArrowDown, ArrowUp, ChevronDown, Loader2 } from "lucide-react";
import {
  createContext,
  use,
  useMemo,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import {
  Button as AriaButton,
  Cell,
  Collection,
  Column,
  ColumnResizer,
  Menu,
  MenuItem,
  MenuSection,
  Popover,
  ResizableTableContainer,
  Row,
  Separator,
  Table,
  TableBody,
  TableHeader,
  TableLayout,
  TableLoadMoreItem,
  Virtualizer,
  type ColumnProps,
  type ColumnRenderProps,
  type Key,
} from "react-aria-components";

import {
  MAX_SORT_LEVELS,
  type BookListItem,
  type Density,
  type SortField,
  type SortLevelParam,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import { cn } from "@/lib/utils";

import { LOCKED_COLUMN_KEYS, MOBILE_COLUMN_KEYS, TABLE_MOBILE_MEDIA_QUERY } from "../table-columns";

import {
  AuthorsCell,
  DetailsButton,
  RatingCell,
  StaticCell,
  StatusCell,
  TextCell,
  TitleCell,
  type CellState,
} from "./cells";
import "./library-table.css";
import { pendingKey, type UseCellEditResult } from "./useCellEdit";

type ColumnId =
  | "select"
  | "title"
  | "subtitle"
  | "authors"
  | "series"
  | "isbn_13"
  | "pages"
  | "status"
  | "rating"
  | "added"
  | "details";

type TableColumn = {
  id: ColumnId;
  name: string;
  field?: SortField;
  defaultWidth: ColumnProps["defaultWidth"];
  minWidth?: number;
  align?: "end";
};

const DATA_COLUMNS: readonly TableColumn[] = [
  { id: "title", name: "Title", field: "title", defaultWidth: "2.4fr", minWidth: 160 },
  { id: "subtitle", name: "Subtitle", defaultWidth: "1.2fr", minWidth: 96 },
  { id: "authors", name: "Authors", field: "author", defaultWidth: "1.3fr", minWidth: 96 },
  { id: "series", name: "Series", defaultWidth: "1.4fr", minWidth: 96 },
  { id: "isbn_13", name: "ISBN", defaultWidth: 148, minWidth: 112 },
  { id: "pages", name: "Pages", field: "pages", defaultWidth: 84, minWidth: 72, align: "end" },
  { id: "status", name: "Status", defaultWidth: 132, minWidth: 104 },
  { id: "rating", name: "Rating", defaultWidth: 120, minWidth: 112 },
  { id: "added", name: "Added", field: "created_at", defaultWidth: 120, minWidth: 96 },
];

const SELECT_COLUMN: TableColumn = { id: "select", name: "Select", defaultWidth: 44 };
const DETAILS_COLUMN: TableColumn = { id: "details", name: "Details", defaultWidth: 48 };

const COLUMN_BY_FIELD: Readonly<Record<SortField, ColumnId>> = {
  title: "title",
  author: "authors",
  created_at: "added",
  pages: "pages",
};

const SORT_WORDS: Readonly<Record<SortField, { asc: string; desc: string }>> = {
  title: { asc: "A to Z", desc: "Z to A" },
  author: { asc: "A to Z", desc: "Z to A" },
  pages: { asc: "shortest first", desc: "longest first" },
  created_at: { asc: "oldest first", desc: "newest first" },
};

const ESTIMATED_ROW: Record<Density, number> = { comfortable: 64, compact: 44 };
const HEADER_HEIGHT = 42;

const EMPTY_READING_STATE: NonNullable<BookListItem["reading_state"]> = {
  status: null,
  rating: null,
  progress_pct: null,
  started_at: null,
  finished_at: null,
};

const dateFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  year: "numeric",
});

const shortDateFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  year: "2-digit",
});

function formatAdded(iso: string, short: boolean): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return (short ? shortDateFormat : dateFormat).format(date);
}

type SortMode = "primary" | "then" | "remove";

function applySort(
  stack: readonly SortLevelParam[],
  field: SortField,
  desc: boolean,
  mode: SortMode,
): SortLevelParam[] {
  const rest = stack.filter((level) => level.field !== field);
  if (mode === "remove") return rest;
  if (mode === "primary") {
    return stack.at(0)?.field === field ? [{ field, desc }, ...rest] : [{ field, desc }];
  }
  const existing = stack.findIndex((level) => level.field === field);
  if (existing >= 0)
    return stack.map((level, index) => (index === existing ? { field, desc } : level));
  return [...rest.slice(0, MAX_SORT_LEVELS - 1), { field, desc }];
}

type TableEditContextValue = {
  density: Density;
  isMobile: boolean;
  stateFor: (rowId: string, columnKey: string) => CellState;
  commit: (book: BookListItem, columnKey: string, next: BookListItem) => void;
  openDetails: (book: BookListItem) => void;
};

const TableEditContext = createContext<TableEditContextValue | null>(null);

function useTableEdit(): TableEditContextValue {
  const value = use(TableEditContext);
  if (value === null) throw new Error("Library table cells render inside LibraryTable.");
  return value;
}

function BookCell({
  book,
  columnId,
}: Readonly<{ book: BookListItem; columnId: ColumnId }>): ReactElement | null {
  const { density, isMobile, stateFor, commit, openDetails } = useTableEdit();
  const state = stateFor(book.id, columnId);
  const reading = book.reading_state ?? EMPTY_READING_STATE;
  switch (columnId) {
    case "select":
      return (
        <div className="flex h-full items-center justify-center">
          <Checkbox
            slot="selection"
            aria-label={`Select ${book.title}`}
            className="size-full justify-center"
          />
        </div>
      );
    case "details":
      return (
        <DetailsButton
          book={book}
          onOpen={() => {
            openDetails(book);
          }}
        />
      );
    case "title":
      return (
        <TitleCell
          book={book}
          density={density}
          showAuthors={isMobile && density === "comfortable"}
          state={state}
          onCommit={(title) => {
            commit(book, "title", { ...book, title });
          }}
        />
      );
    case "subtitle":
      return (
        <TextCell
          density={density}
          field="subtitle"
          value={book.subtitle}
          kind="text"
          state={state}
          onCommit={(subtitle) => {
            commit(book, "subtitle", { ...book, subtitle });
          }}
        />
      );
    case "authors":
      return (
        <AuthorsCell
          book={book}
          density={density}
          state={state}
          onCommit={(authors) => {
            commit(book, "authors", { ...book, authors });
          }}
        />
      );
    case "series":
      return (
        <StaticCell density={density} className="text-fg-muted">
          {book.series === null ? null : (
            <>
              {book.series.name}
              {book.series.position === null ? null : (
                <span className="text-fg-faint ml-1.5 font-mono text-xs tabular-nums">
                  #{book.series.position}
                </span>
              )}
            </>
          )}
        </StaticCell>
      );
    case "isbn_13":
      return (
        <TextCell
          density={density}
          field="ISBN"
          value={book.isbn_13}
          kind="text"
          mono
          state={state}
          onCommit={(isbn) => {
            commit(book, "isbn_13", { ...book, isbn_13: isbn });
          }}
        />
      );
    case "pages":
      return (
        <TextCell
          density={density}
          field="pages"
          value={book.pages === null ? null : String(book.pages)}
          kind="pages"
          align="end"
          state={state}
          onCommit={(pages) => {
            commit(book, "pages", { ...book, pages: pages === null ? null : Number(pages) });
          }}
        />
      );
    case "status":
      return (
        <StatusCell
          density={density}
          status={reading.status}
          state={state}
          onCommit={(status) => {
            commit(book, "status", { ...book, reading_state: { ...reading, status } });
          }}
        />
      );
    case "rating":
      return (
        <RatingCell
          density={density}
          rating={reading.rating}
          state={state}
          onCommit={(rating) => {
            commit(book, "rating", { ...book, reading_state: { ...reading, rating } });
          }}
        />
      );
    case "added":
      return (
        <StaticCell
          density={density}
          className={cn("text-fg-muted tabular-nums", isMobile && "px-2 text-xs")}
        >
          {formatAdded(book.created_at, isMobile)}
        </StaticCell>
      );
  }
}

type HeaderMenuAction = "asc" | "desc" | "then-asc" | "then-desc" | "remove" | "resize" | "hide";

function isHeaderMenuAction(key: Key): key is HeaderMenuAction {
  return ["asc", "desc", "then-asc", "then-desc", "remove", "resize", "hide"].includes(String(key));
}

type HeaderContentProps = {
  column: TableColumn;
  render: ColumnRenderProps;
  sort: readonly SortLevelParam[];
  canHide: boolean;
  showMenu: boolean;
  onSort: (field: SortField, desc: boolean, mode: SortMode) => void;
  onHide: () => void;
};

function HeaderContent({
  column,
  render,
  sort,
  canHide,
  showMenu,
  onSort,
  onHide,
}: Readonly<HeaderContentProps>): ReactElement {
  const menuRef = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const field = column.field;
  const levelIndex = field === undefined ? -1 : sort.findIndex((level) => level.field === field);
  const level = levelIndex >= 0 ? sort[levelIndex] : undefined;
  const words = field === undefined ? undefined : SORT_WORDS[field];
  const othersSorted = sort.some((entry) => entry.field !== field);

  function act(key: Key): void {
    setMenuOpen(false);
    if (!isHeaderMenuAction(key)) return;
    if (key === "resize") {
      render.startResize();
      return;
    }
    if (key === "hide") {
      onHide();
      return;
    }
    if (field === undefined) return;
    if (key === "remove") onSort(field, false, "remove");
    else if (key === "asc" || key === "desc") onSort(field, key === "desc", "primary");
    else onSort(field, key === "then-desc", "then");
  }

  return (
    <div
      className={cn(
        "group/header flex h-full items-center gap-1 pr-2 pl-3",
        column.align === "end" && "flex-row-reverse pr-3 pl-1",
      )}
    >
      <span
        className={cn(
          "truncate text-xs font-medium tracking-[0.02em]",
          level === undefined ? "text-fg-muted" : "text-fg",
        )}
      >
        {column.name}
      </span>
      {level === undefined ? null : (
        <span className="text-fg flex shrink-0 items-center">
          {level.desc ? (
            <ArrowDown aria-hidden="true" className="size-3.5" />
          ) : (
            <ArrowUp aria-hidden="true" className="size-3.5" />
          )}
          {sort.length > 1 ? (
            <span className="text-fg-muted font-mono text-xs leading-none tabular-nums">
              {levelIndex + 1}
              <span className="sr-only">{`, sort level ${String(levelIndex + 1)}`}</span>
            </span>
          ) : null}
        </span>
      )}
      {showMenu ? (
        <>
          <AriaButton
            ref={menuRef}
            aria-label={`${column.name} column options`}
            onPress={() => {
              setMenuOpen(true);
            }}
            className={cn(
              "text-fg-muted data-hovered:bg-surface-3 data-hovered:text-fg flex size-6 shrink-0 items-center justify-center rounded-md outline-none",
              "opacity-0 transition-opacity duration-150 group-hover/header:opacity-100 data-focus-visible:opacity-100 pointer-coarse:opacity-100",
              menuOpen && "opacity-100",
              column.align === "end" ? "mr-auto" : "ml-auto",
            )}
          >
            <ChevronDown aria-hidden="true" className="size-3.5" />
          </AriaButton>
          <Popover
            triggerRef={menuRef}
            isOpen={menuOpen}
            onOpenChange={setMenuOpen}
            placement="bottom end"
            className="bg-surface text-fg border-border-strong min-w-52 rounded-lg border p-1 shadow-md outline-none"
          >
            <Menu
              aria-label={`${column.name} column options`}
              autoFocus="first"
              onAction={act}
              className="outline-none"
            >
              {words === undefined ? null : (
                <MenuSection>
                  <MenuItem id="asc" className={MENU_ITEM}>
                    Sort {words.asc}
                  </MenuItem>
                  <MenuItem id="desc" className={MENU_ITEM}>
                    Sort {words.desc}
                  </MenuItem>
                  {othersSorted ? (
                    <>
                      <MenuItem id="then-asc" className={MENU_ITEM}>
                        Then by {column.name.toLowerCase()}, {words.asc}
                      </MenuItem>
                      <MenuItem id="then-desc" className={MENU_ITEM}>
                        Then by {column.name.toLowerCase()}, {words.desc}
                      </MenuItem>
                    </>
                  ) : null}
                  {level === undefined ? null : (
                    <MenuItem id="remove" className={MENU_ITEM}>
                      Remove from sort
                    </MenuItem>
                  )}
                </MenuSection>
              )}
              {words === undefined ? null : <Separator className="bg-border -mx-1 my-1 h-px" />}
              <MenuSection>
                <MenuItem id="resize" className={MENU_ITEM}>
                  Resize column
                </MenuItem>
                {canHide ? (
                  <MenuItem id="hide" className={MENU_ITEM}>
                    Hide column
                  </MenuItem>
                ) : null}
              </MenuSection>
            </Menu>
          </Popover>
        </>
      ) : null}
      <ColumnResizer
        aria-label={`Resize ${column.name}`}
        className={cn(
          "absolute top-2 right-0 bottom-2 flex w-3 translate-x-1/2 cursor-col-resize touch-none justify-center outline-none",
          "after:bg-border-strong after:w-px after:opacity-0 after:transition-opacity group-hover/header:after:opacity-100",
          "data-resizing:after:bg-accent-text data-resizing:after:w-0.5 data-resizing:after:opacity-100 data-focused:after:bg-accent-text data-focused:after:opacity-100",
        )}
      />
    </div>
  );
}

const MENU_ITEM =
  "text-fg flex cursor-default items-center rounded-md px-2.5 py-1.5 text-sm outline-none data-focused:bg-surface-2";

type Props = {
  items: readonly BookListItem[];
  sort: readonly SortLevelParam[];
  defaultSort: readonly SortLevelParam[];
  onSortChange: (levels: readonly SortLevelParam[]) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
  edits: UseCellEditResult;
  density: Density;
  hiddenColumns: ReadonlySet<string>;
  onHideColumn: (key: string) => void;
  selectedIds: ReadonlySet<string>;
  onSelectionChange: (ids: ReadonlySet<string>) => void;
  onRowActivate: (book: BookListItem) => void;
};

export function LibraryTable({
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
  onHideColumn,
  selectedIds,
  onSelectionChange,
  onRowActivate,
}: Readonly<Props>): ReactElement {
  const isMobile = useMediaQuery(TABLE_MOBILE_MEDIA_QUERY);
  const shiftHeld = useRef(false);
  const byId = useMemo(() => new Map(items.map((book) => [book.id, book])), [items]);

  const columns = useMemo(
    () => [
      SELECT_COLUMN,
      ...DATA_COLUMNS.filter((column) => {
        if (LOCKED_COLUMN_KEYS.has(column.id)) return true;
        if (isMobile) return MOBILE_COLUMN_KEYS.has(column.id);
        return !hiddenColumns.has(column.id);
      }),
      DETAILS_COLUMN,
    ],
    [hiddenColumns, isMobile],
  );

  const failed = new Set(edits.feedback.map((entry) => pendingKey(entry.rowId, entry.columnKey)));
  const context: TableEditContextValue = {
    density,
    isMobile,
    stateFor: (rowId, columnKey) => {
      const key = pendingKey(rowId, columnKey);
      return { pending: edits.pendingCells.get(key), failed: failed.has(key) };
    },
    commit: (book, columnKey, next) => {
      edits.onCellEdit({ row: next, previousRow: book, columnKey });
    },
    openDetails: onRowActivate,
  };

  const primary = sort.at(0);
  const sortDescriptor =
    primary === undefined
      ? undefined
      : {
          column: COLUMN_BY_FIELD[primary.field],
          direction: primary.desc ? ("descending" as const) : ("ascending" as const),
        };

  function descendingFirst(field: SortField): boolean {
    return defaultSort.some((level) => level.field === field && level.desc);
  }

  function sortBy(field: SortField, desc: boolean, mode: SortMode): void {
    onSortChange(applySort(sort, field, desc, mode));
  }

  function handleHeaderSort(columnKey: Key): void {
    const field = DATA_COLUMNS.find((column) => column.id === columnKey)?.field;
    if (field === undefined) return;
    const existing = sort.find((level) => level.field === field);
    if (shiftHeld.current) {
      sortBy(field, existing === undefined ? descendingFirst(field) : !existing.desc, "then");
      return;
    }
    sortBy(field, primary?.field === field ? !primary.desc : descendingFirst(field), "primary");
  }

  const canLoadMore = hasNextPage && !isFetchNextPageError;

  function renderColumn(column: TableColumn): ReactElement {
    if (column.id === "select") {
      return (
        <Column
          id="select"
          width={isMobile ? 40 : 44}
          minWidth={40}
          className="border-border-strong size-full border-b outline-none"
        >
          <div className="flex h-full items-center justify-center">
            <Checkbox
              slot="selection"
              aria-label="Select all loaded books"
              className="size-full justify-center"
            />
          </div>
        </Column>
      );
    }
    if (column.id === "details") {
      return (
        <Column
          id="details"
          width={isMobile ? 40 : 48}
          minWidth={40}
          className="border-border-strong size-full border-b outline-none"
        >
          <span className="sr-only">Details</span>
        </Column>
      );
    }
    return (
      <Column
        id={column.id}
        isRowHeader={column.id === "title"}
        allowsSorting={column.field !== undefined}
        defaultWidth={isMobile && column.id === "added" ? 84 : column.defaultWidth}
        minWidth={isMobile && column.id === "added" ? 80 : column.minWidth}
        textValue={column.name}
        className="border-border-strong relative size-full border-b text-left outline-none"
      >
        {(render) => (
          <HeaderContent
            column={column}
            render={render}
            sort={sort}
            canHide={!LOCKED_COLUMN_KEYS.has(column.id)}
            showMenu={!isMobile}
            onSort={sortBy}
            onHide={() => {
              onHideColumn(column.id);
            }}
          />
        )}
      </Column>
    );
  }

  return (
    <div
      data-testid="library-table"
      className="rv-table"
      {...edits.keyboardProps}
      onPointerDownCapture={(event) => {
        shiftHeld.current = event.shiftKey;
      }}
      onKeyDownCapture={(event) => {
        shiftHeld.current = event.shiftKey;
      }}
    >
      <TableEditContext value={context}>
        <ResizableTableContainer className="relative w-full">
          <Virtualizer
            layout={TableLayout}
            layoutOptions={{
              estimatedRowHeight: ESTIMATED_ROW[density],
              headingHeight: HEADER_HEIGHT,
              loaderHeight: 48,
            }}
          >
            <Table
              aria-label="Library books"
              selectionMode="multiple"
              selectedKeys={selectedIds}
              onSelectionChange={(keys) => {
                onSelectionChange(
                  keys === "all"
                    ? new Set(items.map((book) => book.id))
                    : new Set([...keys].map(String)),
                );
              }}
              sortDescriptor={sortDescriptor}
              onSortChange={(descriptor) => {
                handleHeaderSort(descriptor.column);
              }}
              onRowAction={(key) => {
                const book = byId.get(String(key));
                if (book !== undefined) onRowActivate(book);
              }}
              className="border-border bg-canvas h-[calc(100dvh-22rem)] min-h-96 w-full scroll-pt-[42px] overflow-auto rounded-md border outline-none"
            >
              <TableHeader
                columns={columns}
                dependencies={[sort, hiddenColumns]}
                className="bg-surface-2 size-full"
              >
                {renderColumn}
              </TableHeader>
              <TableBody>
                <Collection items={items} dependencies={[columns, density]}>
                  {(book) => (
                    <Row
                      id={book.id}
                      textValue={book.title}
                      columns={columns}
                      className="group/row text-fg data-hovered:bg-surface data-selected:bg-accent-soft [height:inherit] [width:inherit] cursor-default outline-none"
                    >
                      {(column) => (
                        <Cell
                          textValue={column.id === "title" ? book.title : undefined}
                          className="border-border size-full overflow-hidden border-b outline-none"
                        >
                          <BookCell book={book} columnId={column.id} />
                        </Cell>
                      )}
                    </Row>
                  )}
                </Collection>
                <TableLoadMoreItem
                  onLoadMore={canLoadMore ? onLoadMore : undefined}
                  isLoading={isFetchingNextPage}
                  className="text-fg-muted flex items-center justify-center gap-2 text-sm"
                >
                  <Loader2
                    aria-hidden="true"
                    className="size-4 animate-spin motion-reduce:animate-none"
                  />
                  Loading more books…
                </TableLoadMoreItem>
              </TableBody>
            </Table>
          </Virtualizer>
        </ResizableTableContainer>
      </TableEditContext>
      <div className="text-fg-muted mt-3 flex min-h-8 items-center justify-center gap-3 text-sm">
        <PagingStatus
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

type PagingStatusProps = {
  loadedCount: number;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  isFetchNextPageError: boolean;
  onLoadMore: () => void;
};

function PagingStatus({
  loadedCount,
  hasNextPage,
  isFetchingNextPage,
  isFetchNextPageError,
  onLoadMore,
}: Readonly<PagingStatusProps>): ReactNode {
  const count = `${loadedCount.toLocaleString()} ${loadedCount === 1 ? "book" : "books"}`;
  if (isFetchingNextPage) return <output>{count} loaded, loading more…</output>;
  if (isFetchNextPageError) {
    return (
      <span role="alert" className="flex items-center gap-3">
        Couldn&apos;t load more books.
        <Button variant="outline" size="sm" onPress={onLoadMore}>
          Retry
        </Button>
      </span>
    );
  }
  if (hasNextPage) {
    return (
      <>
        <span>{count} loaded</span>
        <Button variant="ghost" size="sm" onPress={onLoadMore}>
          Load more
        </Button>
      </>
    );
  }
  return loadedCount > 0 ? <span>All {count} shown</span> : null;
}
