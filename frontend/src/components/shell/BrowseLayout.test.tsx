import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { describe, expect, test, vi } from "vite-plus/test";

import { BrowseLayout } from "./BrowseLayout";

function Harness({
  railCollapsed = false,
  initialSheetOpen = false,
  onSheetOpenChange,
}: {
  railCollapsed?: boolean;
  initialSheetOpen?: boolean;
  onSheetOpenChange?: (open: boolean) => void;
}): ReactElement {
  const [sheetOpen, setSheetOpen] = useState(initialSheetOpen);
  return (
    <BrowseLayout
      rail={<aside aria-label="Filters">RAIL_CONTENT</aside>}
      railCollapsed={railCollapsed}
      sheetOpen={sheetOpen}
      onSheetOpenChange={(open) => {
        onSheetOpenChange?.(open);
        setSheetOpen(open);
      }}
    >
      <p>PAGE_CONTENT</p>
    </BrowseLayout>
  );
}

function renderLayout(props: Parameters<typeof Harness>[0] = {}): void {
  const router = createMemoryRouter([{ path: "/library", element: <Harness {...props} /> }], {
    initialEntries: ["/library"],
  });
  render(<RouterProvider router={router} />);
}

describe("BrowseLayout", () => {
  test("renders page content alongside the filter rail column", () => {
    renderLayout();
    expect(screen.getByText("PAGE_CONTENT")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Filters" })).toBeInTheDocument();
  });

  test("collapsing the rail removes the desktop column but keeps the page content", () => {
    renderLayout({ railCollapsed: true });
    expect(screen.getByText("PAGE_CONTENT")).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Filters" })).not.toBeInTheDocument();
  });

  test("hosts no rail trigger of its own", () => {
    renderLayout();
    expect(screen.queryByRole("button", { name: /Refine/ })).not.toBeInTheDocument();
  });

  test("the controlled sheet carries the same rail; esc closes through the callback", async () => {
    const onSheetOpenChange = vi.fn();
    renderLayout({ initialSheetOpen: true, onSheetOpenChange });
    const user = userEvent.setup();
    const sheet = await screen.findByRole("dialog", { name: /Filters/ });
    expect(within(sheet).getByText("RAIL_CONTENT")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(onSheetOpenChange).toHaveBeenCalledWith(false);
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
