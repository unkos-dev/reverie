import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { addShelfItem, listShelves } from "@/api";
// The barrel is mocked below; this deep import is not, so it yields the
// real implementation (schema included) for the contract test.
import { listShelves as realListShelves } from "@/api/shelves";
import { toast } from "sonner";
import { queryKeys } from "@/lib/query/keys";

import { BatchBar } from "./BatchBar";

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

function renderBar(
  overrides: BarProps = {},
  { seedShelves = true }: { seedShelves?: boolean } = {},
): {
  client: QueryClient;
  onCompleted: ReturnType<typeof vi.fn>;
  onClearSelection: ReturnType<typeof vi.fn>;
} {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  // Seeding keeps the picker tests off the network; the contract test
  // below opts out so the real query (and its schema) actually runs.
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
  render(<Wrapper />);
  return { client, onCompleted, onClearSelection };
}

async function pickShelf(): Promise<void> {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /Add to shelf/ }));
  await user.click(await screen.findByRole("menuitem", { name: "Favourites" }));
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
    renderBar();
    await pickShelf();
    await waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });
    resolvers[0]();
    await waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(2);
    });
  });

  test("full success reports every id, toasts success, and invalidates the shelves list", async () => {
    vi.mocked(addShelfItem).mockResolvedValue(undefined);
    const { client, onCompleted } = renderBar();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    await pickShelf();
    await waitFor(() => {
      expect(onCompleted).toHaveBeenCalledWith(["a", "b", "c"]);
    });
    expect(toast.success).toHaveBeenCalledWith("Added 3 books to Favourites");
    // The whole shelves family: cached shelf details go stale too.
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.shelves.all });
  });

  test("partial failure reports only the succeeded ids and toasts the split", async () => {
    // Deliberate failure logs a console breadcrumb; keep the output clean.
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(addShelfItem)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("409"))
      .mockResolvedValueOnce(undefined);
    const { onCompleted } = renderBar();
    await pickShelf();
    await waitFor(() => {
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
    renderBar();
    await pickShelf();
    await waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });
    // The trigger is disabled during the run: opening the picker again is
    // impossible, which is the re-entry guard the user can see.
    expect(screen.getByRole("button", { name: /Add to shelf/ })).toBeDisabled();
    // Drain the serial run one settled promise at a time.
    for (let index = 0; index < 3; index += 1) {
      await waitFor(() => {
        expect(addShelfItem).toHaveBeenCalledTimes(index + 1);
      });
      resolvers[index]();
    }
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Add to shelf/ })).toBeEnabled();
    });
    expect(vi.mocked(addShelfItem).mock.calls).toHaveLength(3);
  });

  test("retry after a partial failure posts only the still-selected ids", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(addShelfItem).mockRejectedValue(new Error("500"));
    const { onCompleted } = renderBar({ selectedIds: new Set(["b"]) });
    await pickShelf();
    await waitFor(() => {
      expect(onCompleted).toHaveBeenCalledWith([]);
    });
    // Failed id stays selected (container drops only succeeded ids); the
    // retry run posts just that id.
    vi.mocked(addShelfItem).mockClear();
    vi.mocked(addShelfItem).mockResolvedValue(undefined);
    await pickShelf();
    await waitFor(() => {
      expect(addShelfItem).toHaveBeenCalledTimes(1);
    });
    expect(addShelfItem).toHaveBeenCalledWith("shelf-1", "b");
    errorSpy.mockRestore();
  });

  test("renders nothing with an empty selection", () => {
    renderBar({ selectedIds: new Set() });
    expect(screen.queryByRole("toolbar", { name: "Batch actions" })).not.toBeInTheDocument();
  });
});

describe("BatchBar shelves-load failure", () => {
  test("a schema-rejected shelves response degrades to the disabled notice", async () => {
    // This surface is the auxiliary tier: it degrades and logs instead of
    // throwing to the error boundary. Running the real listShelves over a
    // mocked fetch pins that behaviour to the actual schema, so tightening
    // or loosening the validator cannot silently promote this to a crash.
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
              // The shape the endpoint shipped before the wire format was
              // fixed: time's 9-element OffsetDateTime tuple.
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

    renderBar({}, { seedShelves: false });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Add to shelf/ }));

    const notice = await screen.findByRole("menuitem", { name: "Couldn't load shelves" });
    expect(notice).toHaveAttribute("aria-disabled", "true");
    // The toolbar is still mounted: degraded, not unmounted by an error.
    expect(screen.getByRole("toolbar", { name: "Batch actions" })).toBeInTheDocument();
    errorSpy.mockRestore();
  });
});
