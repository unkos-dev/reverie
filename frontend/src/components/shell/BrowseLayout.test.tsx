import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { createMemoryRouter, RouterProvider } from "react-router";

import { BrowseLayout } from "./BrowseLayout";

function renderLayout(initialEntry = "/library"): void {
  const router = createMemoryRouter(
    [
      {
        path: "/library",
        element: (
          <BrowseLayout rail={<aside aria-label="Filters">RAIL_CONTENT</aside>}>
            <p>PAGE_CONTENT</p>
          </BrowseLayout>
        ),
      },
    ],
    { initialEntries: [initialEntry] },
  );
  render(<RouterProvider router={router} />);
}

describe("BrowseLayout", () => {
  test("renders page content alongside the filter rail", () => {
    renderLayout();
    expect(screen.getByText("PAGE_CONTENT")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Filters" })).toBeInTheDocument();
  });

  test("Refine button shows the active-filter dot only when ?series= is set", () => {
    renderLayout("/library?series=s-1");
    const refine = screen.getByRole("button", { name: /Refine/ });
    expect(refine.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });

  test("Refine button carries no active-filter dot without ?series=", () => {
    renderLayout();
    const refine = screen.getByRole("button", { name: /Refine/ });
    expect(refine.querySelector('[aria-hidden="true"]')).toBeNull();
  });

  test("Refine button opens a sheet carrying the same rail; esc closes", async () => {
    renderLayout();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Refine/ }));
    const sheet = await screen.findByRole("dialog", { name: /Filters/ });
    expect(within(sheet).getByText("RAIL_CONTENT")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
