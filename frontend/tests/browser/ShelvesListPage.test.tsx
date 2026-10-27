import type * as Api from "@/api";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { beforeEach, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";

import { ApiError, createShelf, deleteShelf, listShelves, renameShelf, type Shelf } from "@/api";
import { ShelvesListPage } from "@/pages/shelves/ShelvesListPage";
import { queryKeys } from "@/lib/query/keys";

vi.mock("@/api", async (importOriginal) => ({
  ...(await importOriginal<typeof Api>()),
  createShelf: vi.fn(),
  renameShelf: vi.fn(),
  deleteShelf: vi.fn(),
  listShelves: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const shelf: Shelf = {
  id: "shelf-1",
  name: "To read",
  is_system: false,
  item_count: 1,
  created_at: "2026-10-27T00:00:00Z",
  updated_at: "2026-10-27T00:00:00Z",
};

beforeEach(() => {
  vi.resetAllMocks();
});

async function renderShelves(shelves: Shelf[] = [shelf]): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.shelves.list(), shelves);
  vi.mocked(listShelves).mockResolvedValue(shelves);
  await render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ShelvesListPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("awaits creation, retains the name after failure and closes only after a successful retry", async () => {
  let reject: (error: Error) => void = () => {};
  vi.mocked(createShelf).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  await renderShelves([]);
  await userEvent.click(page.getByRole("button", { name: "New shelf" }));
  await userEvent.type(page.getByRole("textbox", { name: "Name" }), "  Favourites  ");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await expect.element(page.getByRole("button", { name: "Save" })).toHaveAttribute("data-pending");
  await expect.element(page.getByRole("textbox", { name: "Name" })).toHaveValue("  Favourites  ");
  expect(createShelf).toHaveBeenCalledExactlyOnceWith("Favourites");
  reject(new ApiError(422, null, "Unprocessable", "Name already exists."));
  await expect.element(page.getByRole("alert")).toMatchTextContent("Name already exists.");
  await expect
    .element(page.getByRole("textbox", { name: "Name" }))
    .not.toHaveAttribute("aria-invalid", "true");
  await expect.element(page.getByRole("textbox", { name: "Name" })).toHaveValue("  Favourites  ");
  vi.mocked(createShelf).mockResolvedValueOnce({ ...shelf, name: "Favourites" });
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  expect(createShelf).toHaveBeenCalledTimes(2);
});

test("keeps an empty name local and cancels without a write", async () => {
  await renderShelves();
  await userEvent.click(page.getByRole("button", { name: "New shelf" }));
  await userEvent.type(page.getByRole("textbox", { name: "Name" }), "   ");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await expect.element(page.getByRole("dialog", { name: "New shelf" })).toBeInTheDocument();
  expect(createShelf).not.toHaveBeenCalled();
  await userEvent.click(page.getByRole("button", { name: "Cancel" }));
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  expect(createShelf).not.toHaveBeenCalled();
});

test.each(["Cancel", "Escape"])(
  "starts a fresh creation after dismissing a failed attempt with %s",
  async (dismiss) => {
    vi.mocked(createShelf).mockRejectedValueOnce(
      new ApiError(422, null, "Unprocessable", "Name already exists."),
    );
    await renderShelves();
    await userEvent.click(page.getByRole("button", { name: "New shelf" }));
    await userEvent.type(page.getByRole("textbox", { name: "Name" }), "To read");
    await userEvent.click(page.getByRole("button", { name: "Save" }));
    await expect.element(page.getByRole("alert")).toHaveTextContent("Name already exists.");
    if (dismiss === "Cancel") {
      await userEvent.click(page.getByRole("button", { name: "Cancel" }));
    } else {
      await userEvent.keyboard("{Escape}");
    }
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "New shelf" }));
    await expect.element(page.getByRole("textbox", { name: "Name" })).toHaveValue("");
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
    expect(createShelf).toHaveBeenCalledTimes(1);
    vi.mocked(createShelf).mockResolvedValueOnce({ ...shelf, name: "Favourites" });
    await userEvent.type(page.getByRole("textbox", { name: "Name" }), "Favourites");
    await userEvent.click(page.getByRole("button", { name: "Save" }));
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    expect(createShelf).toHaveBeenLastCalledWith("Favourites");
    expect(createShelf).toHaveBeenCalledTimes(2);
  },
);

test("seeds rename from its target, retains a failed edit and sends the shelf identifier", async () => {
  vi.mocked(renameShelf).mockRejectedValueOnce(new Error("offline"));
  await renderShelves();
  await userEvent.click(page.getByRole("button", { name: "Rename shelf" }));
  const name = page.getByRole("textbox", { name: "Name" });
  await expect.element(name).toHaveValue("To read");
  await name.fill("Next reads");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await expect.element(page.getByRole("alert")).toMatchTextContent("Could not save shelf.");
  await expect.element(name).toHaveValue("Next reads");
  expect(renameShelf).toHaveBeenCalledExactlyOnceWith("shelf-1", "Next reads");
  await userEvent.click(page.getByRole("button", { name: "Cancel" }));
  await userEvent.click(page.getByRole("button", { name: "Rename shelf" }));
  await expect.element(page.getByRole("textbox", { name: "Name" })).toHaveValue("To read");
});

test("system shelves expose no rename or delete actions", async () => {
  await renderShelves([{ ...shelf, is_system: true }]);
  await expect.element(page.getByText("System")).toBeInTheDocument();
  await expect.element(page.getByRole("button", { name: "Rename shelf" })).not.toBeInTheDocument();
  await expect.element(page.getByRole("button", { name: "Delete shelf" })).not.toBeInTheDocument();
  expect(renameShelf).not.toHaveBeenCalled();
  expect(deleteShelf).not.toHaveBeenCalled();
});
