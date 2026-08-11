import { QueryClient, QueryClientProvider, type InfiniteData } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { MouseEvent, ReactElement } from "react";
import { toast } from "sonner";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError, type BookListItem, type BookListResponse } from "@/api";
import type { FieldVersionChange, UpdateMetadataResponse } from "@/api/books";
import { updateBookMetadata } from "@/api/books";
import { revertField } from "@/api/metadata";
import { updateReadingState } from "@/api/reading";
import type { ReadingState } from "@/api/reading";
import type { CellEditReport, GridColumn } from "@/lib/grid/types";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import { useCellEdit } from "./useCellEdit";

// Factory mocks (not a bare automock): `@/api/reading` imports
// `ReadingStatusSchema` from `@/api/books` at module load and calls
// `.nullable()` on it, which an automocked schema object doesn't carry
// (automocking drops the zod prototype). Preserving the real module via
// `importOriginal` and overriding only the network-calling export sidesteps
// that.
vi.mock("@/api/books", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, updateBookMetadata: vi.fn() };
});
vi.mock("@/api/reading", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, updateReadingState: vi.fn() };
});
vi.mock("@/api/metadata", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, revertField: vi.fn() };
});

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

beforeEach(() => {
  vi.resetAllMocks();
});

const LIST_KEY: BooksListKey = queryKeys.books.list({});

const TEST_COLUMNS: readonly GridColumn<BookListItem>[] = [
  { key: "title", name: "Title", sortable: false, accessor: (row) => row.title },
  { key: "isbn_13", name: "ISBN", sortable: false, accessor: (row) => row.isbn_13 ?? "" },
  { key: "authors", name: "Authors", sortable: false, accessor: (row) => row.authors.join(", ") },
  {
    key: "status",
    name: "Status",
    sortable: false,
    accessor: (row) => row.reading_state?.status ?? "",
  },
  {
    key: "rating",
    name: "Rating",
    sortable: false,
    accessor: (row) => String(row.reading_state?.rating ?? ""),
  },
];

function rowFixture(overrides: Partial<BookListItem> = {}): BookListItem {
  return {
    id: "book-1",
    work_id: "work-1",
    title: "Original Title",
    subtitle: "Original Subtitle",
    authors: ["Original Author"],
    contributors: [],
    series: null,
    isbn_13: "9780000000001",
    pages: 100,
    tags: [],
    genres: [],
    moods: [],
    content_rating: null,
    cover_url: "",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    reading_state: {
      status: "reading",
      rating: 3,
      progress_pct: 40,
      started_at: null,
      finished_at: null,
    },
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function fieldChange(
  value: unknown,
  previousVersionId: string | null = "prev-version",
): FieldVersionChange {
  return { value, version_id: "new-version", previous_version_id: previousVersionId };
}

function readingStateFixture(overrides: Partial<ReadingState> = {}): ReadingState {
  return {
    status: "reading",
    rating: 3,
    notes: null,
    progress_pct: 40,
    started_at: null,
    finished_at: null,
    last_read_at: null,
    ...overrides,
  };
}

function seedCache(client: QueryClient, items: BookListItem[]): void {
  client.setQueryData<InfiniteData<BookListResponse>>(LIST_KEY, {
    pages: [{ items, next_cursor: null }],
    pageParams: [undefined],
  });
}

function readCache(client: QueryClient): BookListItem[] {
  const data = client.getQueryData<InfiniteData<BookListResponse>>(LIST_KEY);
  return data === undefined ? [] : data.pages.flatMap((page) => page.items);
}

function makeClient(): QueryClient {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

/** Test harness: one button per report, firing `onCellEdit` on click, and
 *  a wrapper carrying `onGridKeyDown` so Ctrl+Z can be exercised the same
 *  way the real table's wrapper div does. */
function Harness({ reports }: { reports: readonly CellEditReport<BookListItem>[] }): ReactElement {
  const { pendingCells, onCellEdit, onGridKeyDown } = useCellEdit({
    listKey: LIST_KEY,
    columns: TEST_COLUMNS,
  });
  return (
    <div data-testid="wrapper" onKeyDown={onGridKeyDown}>
      {reports.map((report, index) => (
        <button
          key={`edit-${String(index)}`}
          type="button"
          onClick={() => {
            onCellEdit(report);
          }}
        >
          {`edit-${String(index)}`}
        </button>
      ))}
      <span data-testid="pending-keys">{[...pendingCells.keys()].join(",")}</span>
    </div>
  );
}

function renderHarness(client: QueryClient, reports: CellEditReport<BookListItem>[]): void {
  render(
    <QueryClientProvider client={client}>
      <Harness reports={reports} />
    </QueryClientProvider>,
  );
}

/**
 * Extracts the toast action's `onClick` from a captured `toast.success`
 * call. Narrowed by hand rather than typed through sonner's own
 * `ExternalToast["action"]` (`Action | ReactNode`), since our code only ever
 * passes the `Action` shape.
 */
function getUndoAction(callIndex: number): () => void {
  const call = vi.mocked(toast.success).mock.calls[callIndex];
  const action = call[1]?.action;
  if (
    action === undefined ||
    action === null ||
    typeof action !== "object" ||
    !("onClick" in action)
  ) {
    throw new Error("expected an undo action on this toast");
  }
  const { onClick } = action;
  return () => {
    onClick({} as MouseEvent<HTMLButtonElement>);
  };
}

/** Same extraction as {@link getUndoAction}, for the conflict toast raised
 *  through `toast.error` rather than `toast.success`. */
function getErrorAction(callIndex: number): () => void {
  const call = vi.mocked(toast.error).mock.calls[callIndex];
  const action = call[1]?.action;
  if (
    action === undefined ||
    action === null ||
    typeof action !== "object" ||
    !("onClick" in action)
  ) {
    throw new Error("expected a reload action on this toast");
  }
  const { onClick } = action;
  return () => {
    onClick({} as MouseEvent<HTMLButtonElement>);
  };
}

describe("useCellEdit", () => {
  test("a metadata column commits through updateBookMetadata, never updateReadingState", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledWith(row.id, { title: "New Title" });
    });
    expect(updateReadingState).not.toHaveBeenCalled();
  });

  test("a reading column commits through updateReadingState, never updateBookMetadata", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateReadingState).mockResolvedValue(readingStateFixture({ rating: 5 }));
    const report: CellEditReport<BookListItem> = {
      row: {
        ...row,
        reading_state: {
          status: "reading",
          rating: 5,
          progress_pct: 40,
          started_at: null,
          finished_at: null,
        },
      },
      previousRow: row,
      columnKey: "rating",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(updateReadingState).toHaveBeenCalledWith(row.id, { rating: 5 });
    });
    expect(updateBookMetadata).not.toHaveBeenCalled();
  });

  test("patches the cache from the server's normalized value, not the draft, for isbn_13", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    // Server echoes the checksum-normalized value; the draft carried the
    // hyphenated input the user actually typed.
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { isbn_13: fieldChange("9780000000001") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...row, isbn_13: "978-0-00-000000-1" },
      previousRow: row,
      columnKey: "isbn_13",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(readCache(client)[0].isbn_13).toBe("9780000000001");
    });
  });

  test("a title edit fans out to loaded sibling editions of the same work", async () => {
    const client = makeClient();
    const rowA = rowFixture({ id: "book-1", work_id: "work-1" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1" });
    const other = rowFixture({ id: "book-3", work_id: "work-2", title: "Unrelated Work" });
    seedCache(client, [rowA, rowB, other]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, title: "New Title" },
      previousRow: rowA,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(readCache(client).find((item) => item.id === "book-2")?.title).toBe("New Title");
    });
    expect(readCache(client).find((item) => item.id === "book-3")?.title).toBe("Unrelated Work");
  });

  test("an isbn_13 edit does not fan out to sibling editions of the same work", async () => {
    const client = makeClient();
    const rowA = rowFixture({ id: "book-1", work_id: "work-1", isbn_13: "9780000000001" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1", isbn_13: "9780000000002" });
    seedCache(client, [rowA, rowB]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { isbn_13: fieldChange("9781111111111") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, isbn_13: "9781111111111" },
      previousRow: rowA,
      columnKey: "isbn_13",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(readCache(client).find((item) => item.id === "book-1")?.isbn_13).toBe("9781111111111");
    });
    expect(readCache(client).find((item) => item.id === "book-2")?.isbn_13).toBe("9780000000002");
  });

  test("marks the cell pending with the draft display value, then clears it on success", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    let resolveMutation: (value: UpdateMetadataResponse) => void = () => undefined;
    vi.mocked(updateBookMetadata).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveMutation = resolve;
        }),
    );
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "Draft Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(screen.getByTestId("pending-keys")).toHaveTextContent(`${row.id}:title`);
    });
    resolveMutation({ fields: { title: fieldChange("Draft Title", "v-1") } });
    await waitFor(() => {
      expect(screen.getByTestId("pending-keys")).toHaveTextContent("");
    });
  });

  test("the toast action for a revert-strategy edit calls revertField with the captured previous_version_id", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title", "prev-version-id") },
    });
    vi.mocked(revertField).mockResolvedValue(undefined);
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });

    getUndoAction(0)();

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "title", "prev-version-id");
    });
  });

  test("undoing a reading edit counter-patches through updateReadingState and applies its response, not the raw snapshot", async () => {
    const client = makeClient();
    const row = rowFixture({
      reading_state: {
        status: "reading",
        rating: 2,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    seedCache(client, [row]);
    vi.mocked(updateReadingState).mockResolvedValueOnce(readingStateFixture({ rating: 5 }));
    const report: CellEditReport<BookListItem> = {
      row: {
        ...row,
        reading_state: {
          status: "reading",
          rating: 5,
          progress_pct: 40,
          started_at: null,
          finished_at: null,
        },
      },
      previousRow: row,
      columnKey: "rating",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(updateReadingState).toHaveBeenCalledTimes(1);
    });

    // The counter-patch response carries a server-computed side effect
    // (progress_pct 55) that diverges from the pre-edit snapshot's 40.
    // The applied cache value must come from this response, not the snapshot.
    vi.mocked(updateReadingState).mockResolvedValueOnce(
      readingStateFixture({ rating: 2, progress_pct: 55 }),
    );
    fireEvent.keyDown(screen.getByTestId("wrapper"), { key: "z", ctrlKey: true });

    await waitFor(() => {
      expect(updateReadingState).toHaveBeenNthCalledWith(2, row.id, { rating: 2 });
    });
    await waitFor(() => {
      expect(readCache(client)[0].reading_state).toEqual({
        status: "reading",
        rating: 2,
        progress_pct: 55,
        started_at: null,
        finished_at: null,
      });
    });
  });

  test("Ctrl+Z pops the undo stack LIFO across two edits", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(revertField).mockResolvedValue(undefined);
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { title: fieldChange("Title A", "v-a") },
    });
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { isbn_13: fieldChange("9781111111111", "v-b") },
    });
    const rowAfterA = { ...row, title: "Title A" };
    const reportA: CellEditReport<BookListItem> = {
      row: rowAfterA,
      previousRow: row,
      columnKey: "title",
    };
    const reportB: CellEditReport<BookListItem> = {
      row: { ...rowAfterA, isbn_13: "9781111111111" },
      previousRow: rowAfterA,
      columnKey: "isbn_13",
    };
    renderHarness(client, [reportA, reportB]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(1);
    });
    await user.click(screen.getByRole("button", { name: "edit-1" }));
    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(2);
    });

    const wrapper = screen.getByTestId("wrapper");
    fireEvent.keyDown(wrapper, { key: "z", ctrlKey: true });
    await waitFor(() => {
      expect(revertField).toHaveBeenNthCalledWith(1, row.id, "isbn_13", "v-b");
    });
    fireEvent.keyDown(wrapper, { key: "z", ctrlKey: true });
    await waitFor(() => {
      expect(revertField).toHaveBeenNthCalledWith(2, row.id, "title", "v-a");
    });
  });

  test("a toast action undoes its own entry even when a later edit has since become the LIFO top", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(revertField).mockResolvedValue(undefined);
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { title: fieldChange("Title A", "v-a") },
    });
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { isbn_13: fieldChange("9781111111111", "v-b") },
    });
    const rowAfterA = { ...row, title: "Title A" };
    const reportA: CellEditReport<BookListItem> = {
      row: rowAfterA,
      previousRow: row,
      columnKey: "title",
    };
    const reportB: CellEditReport<BookListItem> = {
      row: { ...rowAfterA, isbn_13: "9781111111111" },
      previousRow: rowAfterA,
      columnKey: "isbn_13",
    };
    renderHarness(client, [reportA, reportB]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(1);
    });
    await user.click(screen.getByRole("button", { name: "edit-1" }));
    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(2);
    });

    // Undo A directly via its own toast action, though B is now the LIFO top.
    getUndoAction(0)();
    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "title", "v-a");
    });

    // Ctrl+Z now reaches B, since A was spliced out of the stack rather than
    // merely shadowed.
    fireEvent.keyDown(screen.getByTestId("wrapper"), { key: "z", ctrlKey: true });
    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "isbn_13", "v-b");
    });
    expect(revertField).toHaveBeenCalledTimes(2);
  });

  test("the undo stack caps at 10 entries, evicting the oldest", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(revertField).mockResolvedValue(undefined);

    const reports: CellEditReport<BookListItem>[] = [];
    let current = row;
    for (let i = 0; i < 11; i += 1) {
      const nextRow = { ...current, title: `Title ${String(i)}` };
      vi.mocked(updateBookMetadata).mockResolvedValueOnce({
        fields: { title: fieldChange(`Title ${String(i)}`, `v-${String(i)}`) },
      });
      reports.push({ row: nextRow, previousRow: current, columnKey: "title" });
      current = nextRow;
    }
    renderHarness(client, reports);
    const user = userEvent.setup();
    // Sequential and awaited: each edit must land before the next fires, or
    // push order into the undo stack races.
    for (let i = 0; i < 11; i += 1) {
      await user.click(screen.getByRole("button", { name: `edit-${String(i)}` }));
      await waitFor(() => {
        expect(updateBookMetadata).toHaveBeenCalledTimes(i + 1);
      });
    }

    const wrapper = screen.getByTestId("wrapper");
    for (let i = 0; i < 11; i += 1) {
      fireEvent.keyDown(wrapper, { key: "z", ctrlKey: true });
    }

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledTimes(10);
    });
    // The oldest entry (Title 0's edit) was evicted at the cap, so it was
    // never reachable by Ctrl+Z.
    expect(revertField).not.toHaveBeenCalledWith(row.id, "title", "v-0");
  });

  test("undoing a metadata edit invalidates the book-detail query so a stale detail view can't outlive the revert", async () => {
    const client = makeClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title", "prev-version-id") },
    });
    vi.mocked(revertField).mockResolvedValue(undefined);
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });
    // Drop the commit's own detail invalidation so only undo's call remains below.
    invalidateSpy.mockClear();

    getUndoAction(0)();

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "title", "prev-version-id");
    });
    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
    });
  });

  test("undoing the first authors edit on an authorless work is not offered, since the counter-patch would violate the last-author guard", async () => {
    const client = makeClient();
    const row = rowFixture({ authors: [] });
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { "contributors.author": fieldChange(["New Author"], null) },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...row, authors: ["New Author"] },
      previousRow: row,
      columnKey: "authors",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      // A single-argument call (no options object) means announceSuccess
      // took its no-undo-entry branch: no toast action, nothing pushed.
      expect(toast.success).toHaveBeenCalledWith("Authors updated.");
    });
  });

  test("undoing an authors counter-patch edit calls updateBookMetadata with the prior author list, not revertField", async () => {
    const client = makeClient();
    const row = rowFixture({ authors: ["Original Author"] });
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { "contributors.author": fieldChange(["New Author"], null) },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...row, authors: ["New Author"] },
      previousRow: row,
      columnKey: "authors",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });

    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { "contributors.author": fieldChange(["Original Author"], "v-restore") },
    });
    getUndoAction(0)();

    await waitFor(() => {
      expect(updateBookMetadata).toHaveBeenNthCalledWith(2, row.id, {
        contributors: { author: ["Original Author"] },
      });
    });
    expect(revertField).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(readCache(client)[0].authors).toEqual(["Original Author"]);
    });
  });

  test("a failed undo toasts an error, leaves the cache at the edited value, and consumes the entry so a second Ctrl+Z is a no-op", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title", "prev-version-id") },
    });
    vi.mocked(revertField).mockRejectedValueOnce(
      new ApiError(404, null, "Not Found", "Version prev-version-id no longer exists."),
    );
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(readCache(client)[0].title).toBe("New Title");
    });

    const wrapper = screen.getByTestId("wrapper");
    fireEvent.keyDown(wrapper, { key: "z", ctrlKey: true });

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "Not Found: Version prev-version-id no longer exists.",
      );
    });
    expect(readCache(client)[0].title).toBe("New Title");

    fireEvent.keyDown(wrapper, { key: "z", ctrlKey: true });
    await waitFor(() => {
      expect(revertField).toHaveBeenCalledTimes(1);
    });
  });

  test("a failed metadata edit leaves the cache untouched, clears the pending entry, and toasts", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Title cannot be cleared."),
    );
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "Broken" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Validation Error: Title cannot be cleared.");
    });
    expect(readCache(client)[0].title).toBe("Original Title");
    await waitFor(() => {
      expect(screen.getByTestId("pending-keys")).toHaveTextContent("");
    });
  });

  test("a failed reading-pipeline edit leaves the cache untouched, clears the pending entry, and toasts", async () => {
    const client = makeClient();
    const row = rowFixture({
      reading_state: {
        status: "reading",
        rating: 3,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    seedCache(client, [row]);
    vi.mocked(updateReadingState).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Rating must be between 1 and 5."),
    );
    const report: CellEditReport<BookListItem> = {
      row: {
        ...row,
        reading_state: {
          status: "reading",
          rating: 5,
          progress_pct: 40,
          started_at: null,
          finished_at: null,
        },
      },
      previousRow: row,
      columnKey: "rating",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Validation Error: Rating must be between 1 and 5.");
    });
    expect(readCache(client)[0].reading_state).toEqual({
      status: "reading",
      rating: 3,
      progress_pct: 40,
      started_at: null,
      finished_at: null,
    });
    await waitFor(() => {
      expect(screen.getByTestId("pending-keys")).toHaveTextContent("");
    });
  });

  test("a no-op commit (an unchanged value) fires no mutation", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    const report: CellEditReport<BookListItem> = {
      row: { ...row },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    expect(updateBookMetadata).not.toHaveBeenCalled();
    expect(updateReadingState).not.toHaveBeenCalled();
  });

  test("a metadata response missing the echoed field logs a contract violation, toasts, and leaves the cache untouched", async () => {
    const client = makeClient();
    const row = rowFixture();
    seedCache(client, [row]);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    // The write succeeded (2xx), but the echoed `fields` map is missing the
    // `title` key this route reads: a malformed response, not a rejection.
    vi.mocked(updateBookMetadata).mockResolvedValue({ fields: {} });
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("The server response was missing the edited field.");
    });
    expect(readCache(client)[0].title).toBe("Original Title");
    expect(errorSpy).toHaveBeenCalledWith(
      "[useCellEdit.onCellEdit] metadata response missing echoed field",
      "title",
    );
    await waitFor(() => {
      expect(screen.getByTestId("pending-keys")).toHaveTextContent("");
    });
    errorSpy.mockRestore();
  });

  test("a work-scoped edit invalidates the whole book-detail family, covering siblings the list never loaded", async () => {
    const client = makeClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const rowA = rowFixture({ id: "book-1", work_id: "work-1" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1" });
    seedCache(client, [rowA, rowB]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, title: "New Title" },
      previousRow: rowA,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
    });
  });

  test("an isbn_13 edit invalidates only the edited manifestation's detail query", async () => {
    const client = makeClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const rowA = rowFixture({ id: "book-1", work_id: "work-1", isbn_13: "9780000000001" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1", isbn_13: "9780000000002" });
    seedCache(client, [rowA, rowB]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { isbn_13: fieldChange("9781111111111") },
    });
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, isbn_13: "9781111111111" },
      previousRow: rowA,
      columnKey: "isbn_13",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detail("book-1") });
    });
    expect(invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.books.detail("book-2") });
    expect(invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
  });

  test("undoing a work-scoped metadata edit invalidates the whole book-detail family too", async () => {
    const client = makeClient();
    const rowA = rowFixture({ id: "book-1", work_id: "work-1" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1" });
    seedCache(client, [rowA, rowB]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { title: fieldChange("New Title", "prev-version-id") },
    });
    vi.mocked(revertField).mockResolvedValue(undefined);
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, title: "New Title" },
      previousRow: rowA,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });

    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    getUndoAction(0)();

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith("book-1", "title", "prev-version-id");
    });
    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
    });
  });

  test("undo restores sibling editions: a title edit fanned out to a sibling is restored on undo too", async () => {
    const client = makeClient();
    const rowA = rowFixture({ id: "book-1", work_id: "work-1", title: "Original Title" });
    const rowB = rowFixture({ id: "book-2", work_id: "work-1", title: "Original Title" });
    seedCache(client, [rowA, rowB]);
    vi.mocked(updateBookMetadata).mockResolvedValueOnce({
      fields: { title: fieldChange("New Title", "prev-version-id") },
    });
    vi.mocked(revertField).mockResolvedValue(undefined);
    const report: CellEditReport<BookListItem> = {
      row: { ...rowA, title: "New Title" },
      previousRow: rowA,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(readCache(client).find((item) => item.id === "book-2")?.title).toBe("New Title");
    });

    fireEvent.keyDown(screen.getByTestId("wrapper"), { key: "z", ctrlKey: true });

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith("book-1", "title", "prev-version-id");
    });
    await waitFor(() => {
      expect(readCache(client).find((item) => item.id === "book-2")?.title).toBe("Original Title");
    });
  });

  test("undo of a cleared field revert-calls with the response's previous_version_id, not null", async () => {
    const client = makeClient();
    const row = rowFixture({ subtitle: "Original Subtitle" });
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockResolvedValue({
      fields: { subtitle: fieldChange(null, "v-prev") },
    });
    vi.mocked(revertField).mockResolvedValue(undefined);
    const report: CellEditReport<BookListItem> = {
      row: { ...row, subtitle: null },
      previousRow: row,
      columnKey: "subtitle",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });

    fireEvent.keyDown(screen.getByTestId("wrapper"), { key: "z", ctrlKey: true });

    await waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "subtitle", "v-prev");
    });
  });

  test("a 412 metadata conflict offers a reload action instead of the generic error, and does not push an undo entry", async () => {
    const client = makeClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const row = rowFixture();
    seedCache(client, [row]);
    vi.mocked(updateBookMetadata).mockRejectedValue(
      new ApiError(
        412,
        "https://reverie.example/probs/if-match-mismatch",
        "Precondition Failed",
        "",
      ),
    );
    const report: CellEditReport<BookListItem> = {
      row: { ...row, title: "New Title" },
      previousRow: row,
      columnKey: "title",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "This book changed elsewhere. Reload to see the latest values before retrying.",
        expect.objectContaining({ action: expect.anything() as unknown }),
      );
    });

    getErrorAction(0)();
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: LIST_KEY });
    // "title" is work-scoped, so the reload fans out to the whole detail
    // family rather than a single manifestation's key.
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
    // No Ctrl+Z affordance for a write that never applied.
    fireEvent.keyDown(screen.getByTestId("wrapper"), { key: "z", ctrlKey: true });
    expect(updateBookMetadata).toHaveBeenCalledTimes(1);
  });

  test("a 412 reading conflict offers a reload action scoped to the manifestation's own detail query", async () => {
    const client = makeClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const row = rowFixture({
      reading_state: {
        status: "reading",
        rating: 3,
        progress_pct: 40,
        started_at: null,
        finished_at: null,
      },
    });
    seedCache(client, [row]);
    vi.mocked(updateReadingState).mockRejectedValue(
      new ApiError(
        412,
        "https://reverie.example/probs/if-match-mismatch",
        "Precondition Failed",
        "",
      ),
    );
    const report: CellEditReport<BookListItem> = {
      row: {
        ...row,
        reading_state: {
          status: "reading",
          rating: 5,
          progress_pct: 40,
          started_at: null,
          finished_at: null,
        },
      },
      previousRow: row,
      columnKey: "rating",
    };
    renderHarness(client, [report]);
    await userEvent.setup().click(screen.getByRole("button", { name: "edit-0" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "This book changed elsewhere. Reload to see the latest values before retrying.",
        expect.objectContaining({ action: expect.anything() as unknown }),
      );
    });

    getErrorAction(0)();
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: LIST_KEY });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detail(row.id) });
    expect(invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.books.detailsAll });
  });
});
