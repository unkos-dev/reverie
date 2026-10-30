import { QueryClient, QueryClientProvider, type InfiniteData } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import type { MouseEvent, ReactElement } from "react";
import { toast } from "sonner";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import type { BookListItem, BookListResponse } from "@/api";
import type { FieldVersionChange } from "@/api/books";
import { updateBookMetadata } from "@/api/books";
import { revertField } from "@/api/metadata";
import type { CellEditReport, GridColumn } from "@/lib/grid/types";
import { queryKeys, type BooksListKey } from "@/lib/query/keys";

import { useCellEdit } from "@/pages/library/table/useCellEdit";

vi.mock("@/api/books", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, updateBookMetadata: vi.fn() };
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

function seedCache(client: QueryClient, items: BookListItem[]): void {
  client.setQueryData<InfiniteData<BookListResponse>>(LIST_KEY, {
    pages: [{ items, next_cursor: null }],
    pageParams: [undefined],
  });
}

function makeClient(): QueryClient {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function Harness({ reports }: { reports: readonly CellEditReport<BookListItem>[] }): ReactElement {
  const { onCellEdit, onGridKeyDown } = useCellEdit({ listKey: LIST_KEY, columns: TEST_COLUMNS });
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
    </div>
  );
}

async function renderHarness(
  client: QueryClient,
  reports: CellEditReport<BookListItem>[],
): Promise<void> {
  await render(
    <QueryClientProvider client={client}>
      <Harness reports={reports} />
    </QueryClientProvider>,
  );
}

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

describe("useCellEdit", () => {
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
    await renderHarness(client, [reportA, reportB]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "edit-0" }));
    await vi.waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(1);
    });
    await user.click(page.getByRole("button", { name: "edit-1" }));
    await vi.waitFor(() => {
      expect(updateBookMetadata).toHaveBeenCalledTimes(2);
    });

    getUndoAction(0)();
    await vi.waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "title", "v-a");
    });

    await userEvent.keyboard("{Control>}z{/Control}");
    await vi.waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "isbn_13", "v-b");
    });
    expect(revertField).toHaveBeenCalledTimes(2);
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
    await renderHarness(client, [report]);
    await userEvent.click(page.getByRole("button", { name: "edit-0" }));
    await vi.waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });

    await userEvent.keyboard("{Control>}z{/Control}");

    await vi.waitFor(() => {
      expect(revertField).toHaveBeenCalledWith(row.id, "subtitle", "v-prev");
    });
  });
});
