import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { addShelfItem, listShelves } from "@/api";

import { listShelves as realListShelves } from "@/api/shelves";
import { toast } from "sonner";
import { queryKeys } from "@/lib/query/keys";

import { BatchBar } from "@/pages/library/BatchBar";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, addShelfItem: vi.fn(), listShelves: vi.fn() };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

beforeEach(() => {
  vi.resetAllMocks();
});

const SHELF = { id: "shelf-1", name: "Favourites", item_count: 0 };

type BarProps = Partial<Parameters<typeof BatchBar>[0]>;

async function renderBar(
  overrides: BarProps = {},
  { seedShelves = true }: { seedShelves?: boolean } = {},
): Promise<{
  client: QueryClient;
  onCompleted: ReturnType<typeof vi.fn>;
  onClearSelection: ReturnType<typeof vi.fn>;
}> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });

  if (seedShelves) client.setQueryData(queryKeys.shelves.list(), [SHELF]);
  const onCompleted = vi.fn();
  const onClearSelection = vi.fn();
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <BatchBar
          selectedIds={new Set(["a", "b", "c"])}
          hasMorePages={false}
          onClearSelection={onClearSelection}
          onCompleted={onCompleted}
          {...overrides}
        />
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
  return { client, onCompleted, onClearSelection };
}

async function pickShelf(): Promise<void> {
  const user = userEvent;
  await user.click(page.getByRole("button", { name: /Add to shelf/ }));
  await user.click(page.getByRole("menuitem", { name: "Favourites" }));
}

describe("BatchBar add-to-shelf run", () => {
  test("posts serially: the second request starts only after the first settles", async () => {
    const resolvers: (() => void)[] = [];
    vi.mocked(addShelfItem).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    await renderBar();
    await pickShelf();
    await vi.waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });
    await page.screenshot({ path: "test-results/browser/library-batch-pending.png" });
    resolvers[0]();
    await vi.waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(2);
    });
  });

  test("full success reports every id, toasts success, and invalidates the shelves list", async () => {
    vi.mocked(addShelfItem).mockResolvedValue(undefined);
    const { client, onCompleted } = await renderBar();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    await pickShelf();
    await vi.waitFor(() => {
      expect(onCompleted).toHaveBeenCalledWith(["a", "b", "c"]);
    });
    expect(toast.success).toHaveBeenCalledWith("Added 3 books to Favourites");

    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.shelves.all });
  });

  test("partial failure reports only the succeeded ids and toasts the split", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(addShelfItem)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("409"))
      .mockResolvedValueOnce(undefined);
    const { onCompleted } = await renderBar();
    await pickShelf();
    await vi.waitFor(() => {
      expect(onCompleted).toHaveBeenCalledWith(["a", "c"]);
    });
    expect(toast.error).toHaveBeenCalledWith("Added 2 of 3 to Favourites; the rest stay selected");
    expect(toast.success).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  test("a second pick while a run is in flight does not double-post", async () => {
    const resolvers: (() => void)[] = [];
    vi.mocked(addShelfItem).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    await renderBar();
    await pickShelf();
    await vi.waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });

    await expect.element(page.getByRole("button", { name: /Add to shelf/ })).toBeDisabled();

    for (let index = 0; index < 3; index += 1) {
      await vi.waitFor(() => {
        expect(addShelfItem).toHaveBeenCalledTimes(index + 1);
      });
      resolvers[index]();
    }
    await expect.element(page.getByRole("button", { name: /Add to shelf/ })).toBeEnabled();
    expect(vi.mocked(addShelfItem).mock.calls).toHaveLength(3);
  });

  test("retry after a partial failure posts only the still-selected ids", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(addShelfItem).mockRejectedValue(new Error("500"));
    const { onCompleted } = await renderBar({ selectedIds: new Set(["b"]) });
    await pickShelf();
    await vi.waitFor(() => {
      expect(onCompleted).toHaveBeenCalledWith([]);
    });

    vi.mocked(addShelfItem).mockClear();
    vi.mocked(addShelfItem).mockResolvedValue(undefined);
    await pickShelf();
    await vi.waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });
    expect(addShelfItem).toHaveBeenCalledWith("shelf-1", "b");
    errorSpy.mockRestore();
  });

  test("renders nothing with an empty selection", async () => {
    await renderBar({ selectedIds: new Set() });
    await expect
      .element(page.getByRole("toolbar", { name: "Batch actions" }))
      .not.toBeInTheDocument();
  });
});

describe("BatchBar shelves-load failure", () => {
  test("a schema-rejected shelves response degrades to the disabled notice", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(listShelves).mockImplementation(realListShelves);
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              id: "shelf-1",
              name: "Favourites",
              is_system: false,

              created_at: [2026, 144, 1, 0, 0, 0, 0, 0, 0],
              updated_at: [2026, 144, 1, 0, 0, 0, 0, 0, 0],
              item_count: 0,
            },
          ],
          next_cursor: null,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    await renderBar({}, { seedShelves: false });
    const user = userEvent;
    const toolbar = page.getByRole("toolbar", { name: "Batch actions" });
    await user.click(page.getByRole("button", { name: /Add to shelf/ }));

    const notice = page.getByRole("menuitem", { name: "Couldn't load shelves" });
    await expect.element(notice).toHaveAttribute("aria-disabled", "true");

    await expect.element(toolbar).toBeInTheDocument();

    await vi.waitFor(() => {
      expect(errorSpy).toHaveBeenCalledWith("[BatchBar] shelves fetch failed", expect.any(Error));
    });
    errorSpy.mockRestore();
  });
});
