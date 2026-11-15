import { ArrowRight, ArrowDown, ArrowUp, ChevronDown, Loader2 } from "lucide-react";
import {
  createContext,
  use,
  useEffect,
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
  PageCountInputSchema,
  type BookListItem,
  type Density,
  type SortField,
  type SortLevelParam,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Link } from "@/components/ui/link";
import { Checkbox } from "@/components/ui/checkbox";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import { cn } from "@/lib/utils";

import { LOCKED_COLUMN_KEYS, MOBILE_COLUMN_KEYS, TABLE_MOBILE_MEDIA_QUERY } from "../table-columns";

import {
  AuthorsCell,
  RatingCell,
  StaticCell,
  StatusCell,
  TextCell,
  TitleCell,
  type CellState,
} from "./cells";
import "./library-table.css";
import { CellSaveState } from "../CellSaveState";
import { pendingKey, type UseCellEditResult } from "./useCellEdit";
import { EDIT_ROUTES, type EditableColumnKey } from "./edit-routing";

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
  | "open";

type TableColumn = {
  id: ColumnId;
  name: string;
  field?: SortField;
  defaultWidth: ColumnProps["defaultWidth"];
  minWidth?: number;
  align?: "end";
};

const DATA_COLUMNS: readonly TableColumn[] = [
  { id: "title", name: "Title", field: "title", defaultWidth: "1fr", minWidth: 240 },
  { id: "subtitle", name: "Subtitle", defaultWidth: 176, minWidth: 112 },
  { id: "authors", name: "Authors", field: "author", defaultWidth: 200, minWidth: 120 },
  { id: "series", name: "Series", defaultWidth: 176, minWidth: 112 },
  { id: "isbn_13", name: "ISBN", defaultWidth: 160, minWidth: 96 },
  { id: "pages", name: "Pages", field: "pages", defaultWidth: 104, minWidth: 104, align: "end" },
  { id: "status", name: "Status", defaultWidth: 144, minWidth: 112 },
  { id: "rating", name: "Rating", defaultWidth: 128, minWidth: 112 },
  { id: "added", name: "Added", field: "created_at", defaultWidth: 144, minWidth: 112 },
];

const SELECT_COLUMN: TableColumn = { id: "select", name: "Select", defaultWidth: 44 };
const OPEN_COLUMN: TableColumn = { id: "open", name: "Open", defaultWidth: 80 };

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

const ESTIMATED_ROW: Record<Density, number> = { comfortable: 52, compact: 40 };
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
  commit: (book: BookListItem, columnKey: EditableColumnKey, next: BookListItem) => void;
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
  const { density, isMobile, stateFor, commit } = useTableEdit();
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
    case "open":
      return (
        <Link
          href={`/b/${book.id}`}
          className="mx-2 h-7 px-2 opacity-0 group-data-hovered/row:opacity-100 focus-visible:opacity-100"
        >
          Open <ArrowRight className="size-3" aria-hidden="true" />
        </Link>
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
            commit(book, "pages", {
              ...book,
              pages: pages === null ? null : PageCountInputSchema.parse(pages),
            });
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

function sortLevelOf(
  column: TableColumn,
  sort: readonly SortLevelParam[],
): { level: SortLevelParam | undefined; index: number } {
  const index =
    column.field === undefined ? -1 : sort.findIndex((level) => level.field === column.field);
  return { level: index >= 0 ? sort[index] : undefined, index };
}

function SortIndicator({
  level,
  index,
  levelCount,
}: Readonly<{ level: SortLevelParam; index: number; levelCount: number }>): ReactElement {
  return (
    <span className="text-fg flex shrink-0 items-center">
      {level.desc ? (
        <ArrowDown aria-hidden="true" className="size-3.5" />
      ) : (
        <ArrowUp aria-hidden="true" className="size-3.5" />
      )}
      {levelCount > 1 ? (
        <span className="text-fg-muted font-mono text-xs leading-none tabular-nums">
          {index + 1}
          <span className="sr-only">{`, sort level ${String(index + 1)}`}</span>
        </span>
      ) : null}
    </span>
  );
}

function HeaderContent({
  column,
  render,
  sort,
  canHide,
  showMenu,
  onSort,
  onHide,
}: Readonly<HeaderContentProps>): ReactElement {
  const { level, index } = sortLevelOf(column, sort);
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
        <SortIndicator level={level} index={index} levelCount={sort.length} />
      )}
      {showMenu ? (
        <ColumnMenu
          column={column}
          render={render}
          sort={sort}
          canHide={canHide}
          onSort={onSort}
          onHide={onHide}
        />
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

function ColumnMenu({
  column,
  render,
  sort,
  canHide,
  onSort,
  onHide,
}: Readonly<Omit<HeaderContentProps, "showMenu">>): ReactElement {
  const menuRef = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const field = column.field;
  const { level } = sortLevelOf(column, sort);
  const words = field === undefined ? undefined : SORT_WORDS[field];
  const othersSorted = sort.some((entry) => entry.field !== field);

  function act(key: Key): void {
    setMenuOpen(false);
    if (!isHeaderMenuAction(key)) return;
    if (key === "resize") render.startResize();
    else if (key === "hide") onHide();
    else if (field !== undefined) onSort(field, ...SORT_ACTIONS[key]);
  }

  return (
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
            <SortMenuSection
              name={column.name}
              words={words}
              othersSorted={othersSorted}
              sorted={level !== undefined}
            />
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
  );
}

const SORT_ACTIONS: Readonly<
  Record<Exclude<HeaderMenuAction, "resize" | "hide">, [desc: boolean, mode: SortMode]>
> = {
  asc: [false, "primary"],
  desc: [true, "primary"],
  "then-asc": [false, "then"],
  "then-desc": [true, "then"],
  remove: [false, "remove"],
};

function SortMenuSection({
  name,
  words,
  othersSorted,
  sorted,
}: Readonly<{
  name: string;
  words: { asc: string; desc: string };
  othersSorted: boolean;
  sorted: boolean;
}>): ReactElement {
  return (
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
            Then by {name.toLowerCase()}, {words.asc}
          </MenuItem>
          <MenuItem id="then-desc" className={MENU_ITEM}>
            Then by {name.toLowerCase()}, {words.desc}
          </MenuItem>
        </>
      ) : null}
      {sorted ? (
        <MenuItem id="remove" className={MENU_ITEM}>
          Remove from sort
        </MenuItem>
      ) : null}
    </MenuSection>
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
  onScrollChange: (scrolled: boolean) => void;
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
  onScrollChange,
}: Readonly<Props>): ReactElement {
  const isMobile = useMediaQuery(TABLE_MOBILE_MEDIA_QUERY);
  const shiftHeld = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const [availableWidth, setAvailableWidth] = useState(0);
  const [resized, setResized] = useState<{
    widths: Map<Key, ColumnProps["width"]>;
    availableWidth: number;
    columns: readonly TableColumn[];
    hasIsbn: boolean;
    hasRating: boolean;
  }>();

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) return;
    const observer = new ResizeObserver(() => {
      setAvailableWidth(Math.max(0, container.clientWidth - 2));
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
    };
  }, []);
  const [saveAttempts, setSaveAttempts] = useState<ReadonlyMap<string, number>>(new Map());

  const columns = useMemo(
    () => [
      SELECT_COLUMN,
      ...DATA_COLUMNS.filter((column) => {
        if (LOCKED_COLUMN_KEYS.has(column.id)) return true;
        if (isMobile) return MOBILE_COLUMN_KEYS.has(column.id);
        return !hiddenColumns.has(column.id);
      }),
      ...(isMobile ? [] : [OPEN_COLUMN]),
    ],
    [hiddenColumns, isMobile],
  );

  const hasIsbn = items.some((book) => Boolean(book.isbn_13));
  const hasRating = items.some((book) => book.reading_state?.rating != null);
  const compactColumns = new Set([
    ...(!hasIsbn ? ["isbn_13"] : []),
    ...(!hasRating ? ["rating"] : []),
  ]);
  const preferredWidths = columns.map((column) => {
    const minimum = column.minWidth ?? Number(column.defaultWidth);
    if (compactColumns.has(column.id) || column.id === "title") return minimum;
    const newlyPopulated =
      (column.id === "isbn_13" && !resized?.hasIsbn) ||
      (column.id === "rating" && !resized?.hasRating);
    const previous = newlyPopulated ? undefined : resized?.widths.get(column.id);
    return Math.max(minimum, typeof previous === "number" ? previous : Number(column.defaultWidth));
  });
  const minimumTotal = columns.reduce(
    (total, column) => total + (column.minWidth ?? Number(column.defaultWidth)),
    0,
  );
  const preferredTotal = preferredWidths.reduce((total, width) => total + width, 0);
  const shrink =
    preferredTotal > minimumTotal
      ? Math.min(
          1,
          Math.max(0, (preferredTotal - availableWidth) / (preferredTotal - minimumTotal)),
        )
      : 0;
  const widths = new Map(
    columns.map((column, index) => {
      const minimum = column.minWidth ?? Number(column.defaultWidth);
      const preferred = preferredWidths[index] ?? minimum;
      let width = preferred - (preferred - minimum) * shrink;
      if (column.id === "title") width += Math.max(0, availableWidth - preferredTotal);
      const manual =
        resized?.availableWidth === availableWidth &&
        resized.columns === columns &&
        resized.hasIsbn === hasIsbn &&
        resized.hasRating === hasRating
          ? resized.widths.get(column.id)
          : undefined;
      return [column.id, compactColumns.has(column.id) ? minimum : (manual ?? width)];
    }),
  );

  const context: TableEditContextValue = {
    density,
    isMobile,
    stateFor: (rowId, columnKey) => {
      const key = pendingKey(rowId, columnKey);
      return {
        pending: edits.pendingCells.get(key),
        failed: edits.feedback.some((entry) => pendingKey(entry.rowId, entry.columnKey) === key),
      };
    },
    commit: (book, columnKey, next) => {
      const route = EDIT_ROUTES[columnKey];
      if (JSON.stringify(route.toPatch(book, book)) !== JSON.stringify(route.toPatch(book, next))) {
        const key = pendingKey(book.id, columnKey);
        setSaveAttempts((current) => new Map(current).set(key, (current.get(key) ?? 0) + 1));
      }
      edits.onCellEdit({ row: next, previousRow: book, columnKey });
    },
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

  const canLoadMore = hasNextPage && !isFetchingNextPage && !isFetchNextPageError;

  function renderColumn(column: TableColumn): ReactElement {
    if (column.id === "select") {
      return (
        <Column
          id="select"
          width={isMobile ? 40 : 44}
          minWidth={isMobile ? 40 : 44}
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
    if (column.id === "open") {
      return (
        <Column
          id="open"
          width={80}
          minWidth={80}
          className="border-border-strong size-full border-b outline-none"
        >
          <span className="sr-only">Open</span>
        </Column>
      );
    }
    let minWidth = column.minWidth;
    if (isMobile) minWidth = column.id === "title" ? 0 : 84;
    return (
      <Column
        id={column.id}
        isRowHeader={column.id === "title"}
        allowsSorting={column.field !== undefined}
        width={isMobile ? undefined : widths.get(column.id)}
        defaultWidth={isMobile && column.id === "added" ? 84 : column.defaultWidth}
        maxWidth={!isMobile && compactColumns.has(column.id) ? minWidth : undefined}
        minWidth={minWidth}
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
      className="rv-table flex h-full min-h-0 flex-col"
      {...edits.keyboardProps}
      onPointerDownCapture={(event) => {
        shiftHeld.current = event.shiftKey;
      }}
      onKeyDownCapture={(event) => {
        shiftHeld.current = event.shiftKey;
      }}
    >
      <TableEditContext value={context}>
        <ResizableTableContainer
          ref={containerRef}
          onResize={(nextWidths) => {
            if (!isMobile)
              setResized({ widths: nextWidths, availableWidth, columns, hasIsbn, hasRating });
          }}
          className="relative min-h-0 w-full flex-1"
        >
          <Virtualizer
            layout={TableLayout}
            layoutOptions={{
              rowHeight: ESTIMATED_ROW[density],
              estimatedRowHeight: ESTIMATED_ROW[density],
              headingHeight: HEADER_HEIGHT,
              loaderHeight: 48,
            }}
          >
            <Table
              keyboardNavigationBehavior="tab"
              onScroll={(event) => {
                onScrollChange(event.currentTarget.scrollTop > 0);
              }}
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
              className="border-border bg-canvas h-[calc(100dvh-20rem)] min-h-40 lg:h-full lg:min-h-0 w-full scroll-pt-[42px] overflow-auto rounded-md border outline-none"
            >
              <TableHeader
                columns={columns}
                dependencies={[sort, hiddenColumns, isMobile, widths]}
                className="bg-surface-2 size-full"
              >
                {renderColumn}
              </TableHeader>
              <TableBody>
                <Collection
                  items={items}
                  dependencies={[
                    columns,
                    density,
                    edits.pendingCells,
                    edits.feedback,
                    saveAttempts,
                  ]}
                >
                  {(book) => (
                    <Row
                      id={book.id}
                      data-book-id={book.id}
                      textValue={book.title}
                      columns={columns}
                      dependencies={[edits.pendingCells, edits.feedback, saveAttempts]}
                      className="group/row text-fg data-hovered:bg-surface data-selected:bg-accent-soft [height:inherit] [width:inherit] cursor-default outline-none"
                    >
                      {(column) => (
                        <Cell
                          textValue={column.id === "title" ? book.title : undefined}
                          className="border-border size-full overflow-hidden border-b outline-none"
                        >
                          <div className="relative flex size-full items-center">
                            <BookCell book={book} columnId={column.id} />
                            <CellSaveState
                              attempt={saveAttempts.get(pendingKey(book.id, column.id)) ?? 0}
                              pending={edits.pendingCells.has(pendingKey(book.id, column.id))}
                              feedback={edits.feedback.find(
                                (entry) => entry.rowId === book.id && entry.columnKey === column.id,
                              )}
                            />
                          </div>
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
