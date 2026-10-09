import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import type { ReactElement } from "react";

import type { ShelfWithItems } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { ShelfDetailPage } from "./ShelfDetailPage";

const SHELF_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

function shelfFixture(itemIds: string[]): ShelfWithItems {
  return {
    id: SHELF_ID,
    name: "To be read",
    is_system: false,
    created_at: "2026-05-24T01:00:00Z",
    updated_at: "2026-05-24T01:00:00Z",
    items: itemIds.map((manifestation_id) => ({
      manifestation_id,
      added_at: "2026-05-24T02:00:00Z",
    })),
  };
}

function renderShelf(shelf: ShelfWithItems): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.shelves.detail(SHELF_ID), shelf);
  const router = createMemoryRouter([{ path: "/shelves/:id", element: <ShelfDetailPage /> }], {
    initialEntries: [`/shelves/${SHELF_ID}`],
  });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  render(<Wrapper />);
}

describe("ShelfDetailPage", () => {
  test("lists the items in the order the server returned them", async () => {
    renderShelf(shelfFixture(["22222222-aaaa", "11111111-bbbb", "33333333-cccc"]));
    expect(await screen.findByRole("heading", { name: "To be read" })).toBeInTheDocument();
    expect(screen.getByText("3 items")).toBeInTheDocument();
    const rows = screen.getAllByRole("listitem").map((row) => row.textContent);
    expect(rows).toEqual(["22222222", "11111111", "33333333"]);
  });

  test("offers no reorder control", async () => {
    renderShelf(shelfFixture(["22222222-aaaa", "11111111-bbbb"]));
    await screen.findByRole("heading", { name: "To be read" });
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  test("uses the singular for a single item", async () => {
    renderShelf(shelfFixture(["22222222-aaaa"]));
    expect(await screen.findByText("1 item")).toBeInTheDocument();
  });

  test("renders an empty shelf with no rows", async () => {
    renderShelf(shelfFixture([]));
    expect(await screen.findByText("0 items")).toBeInTheDocument();
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });
});
