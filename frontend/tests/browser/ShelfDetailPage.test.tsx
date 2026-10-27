import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page } from "vite-plus/test/browser";
import { describe, expect, test } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import type { ReactElement } from "react";

import type { ShelfWithItems } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { ShelfDetailPage } from "@/pages/shelves/ShelfDetailPage";

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

async function renderShelf(shelf: ShelfWithItems): Promise<void> {
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

  await render(<Wrapper />);
}

describe("ShelfDetailPage", () => {
  test("lists the items in the order the server returned them", async () => {
    await renderShelf(shelfFixture(["22222222-aaaa", "11111111-bbbb", "33333333-cccc"]));
    await expect.element(page.getByRole("heading", { name: "To be read" })).toBeInTheDocument();
    await expect.element(page.getByText("3 items")).toBeInTheDocument();
    const rows = page
      .getByRole("listitem")
      .elements()
      .map((row) => row.textContent);
    expect(rows).toEqual(["22222222", "11111111", "33333333"]);
  });

  test("offers no reorder control", async () => {
    await renderShelf(shelfFixture(["22222222-aaaa", "11111111-bbbb"]));
    await expect.element(page.getByRole("heading", { name: "To be read" })).toBeInTheDocument();
    await expect.element(page.getByRole("button")).not.toBeInTheDocument();
  });

  test("uses the singular for a single item", async () => {
    await renderShelf(shelfFixture(["22222222-aaaa"]));
    await expect.element(page.getByText("1 item")).toBeInTheDocument();
  });

  test("renders an empty shelf with no rows", async () => {
    await renderShelf(shelfFixture([]));
    await expect.element(page.getByText("0 items")).toBeInTheDocument();
    expect(page.getByRole("listitem").elements()).toHaveLength(0);
  });
});
