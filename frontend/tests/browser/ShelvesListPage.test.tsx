import { cleanup, render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import { RelayEnvironmentProvider, loadQuery } from "react-relay";
import { createMockEnvironment } from "relay-test-utils";
import shelvesQuery, {
  type ShelvesListPageQuery,
} from "@/lib/relay/__generated__/ShelvesListPageQuery.graphql";
import { ShelvesListPage } from "@/pages/shelves/ShelvesListPage";

afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
});

// REV-REQ-0100 criterion 6.
test("shelf_name_conflicts_retain_input", async () => {
  const environment = createMockEnvironment();
  const writes = vi.spyOn(environment, "executeMutation");
  const router = createMemoryRouter(
    [
      {
        path: "/shelves",
        element: <ShelvesListPage />,
        loader: () => loadQuery<ShelvesListPageQuery>(environment, shelvesQuery, {}),
      },
    ],
    { initialEntries: ["/shelves"] },
  );
  await render(
    <RelayEnvironmentProvider environment={environment}>
      <RouterProvider router={router} />
    </RelayEnvironmentProvider>,
  );
  await vi.waitFor(() => {
    environment.mock.resolve(shelvesQuery, {
      data: {
        shelves: {
          edges: [
            {
              cursor: "other",
              node: {
                __typename: "Shelf",
                id: "other",
                name: "Other",
                isSystem: false,
                itemCount: 0,
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: "other",
            endCursor: "other",
          },
        },
      },
    });
  });
  let submissions = 0;
  for (const action of ["New shelf", "Rename shelf"]) {
    await userEvent.click(page.getByRole("button", { name: action, exact: true }));
    const dialog = page.getByRole("dialog", { name: action, exact: true });
    const input = dialog.getByRole("textbox", { name: "Name" });
    const save = dialog.getByRole("button", { name: "Save", exact: true });
    await userEvent.fill(input, "CAFE");
    await userEvent.click(save);
    await expect.element(save).toHaveAttribute("data-pending");
    submissions += 1;
    expect(writes).toHaveBeenCalledTimes(submissions);
    const operation = environment.mock.getMostRecentOperation();
    expect(operation.request.variables).toMatchObject({ input: { name: "CAFE" } });
    const field = action === "New shelf" ? "createShelf" : "renameShelf";
    environment.mock.resolve(operation, {
      data: {
        [field]: {
          __typename: "ShelfNameTaken",
          message: "You already have a shelf with this name.",
        },
      },
    });
    await expect
      .element(dialog.getByText("You already have a shelf with this name."))
      .toBeVisible();
    await expect.element(input).toHaveValue("CAFE");
    await expect.element(dialog).toBeVisible();
    await expect.element(save).not.toHaveAttribute("data-pending");
    await expect.element(save).toBeEnabled();
    expect(writes).toHaveBeenCalledTimes(submissions);
    expect(environment.mock.getAllOperations()).toHaveLength(0);
    await userEvent.click(dialog.getByRole("button", { name: "Cancel", exact: true }));
  }
});
