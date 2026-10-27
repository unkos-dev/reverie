import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { describe, expect, test, vi } from "vite-plus/test";

import { BrowseLayout } from "@/components/shell/BrowseLayout";

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

async function renderLayout(props: Parameters<typeof Harness>[0] = {}): Promise<void> {
  const router = createMemoryRouter([{ path: "/library", element: <Harness {...props} /> }], {
    initialEntries: ["/library"],
  });
  await render(<RouterProvider router={router} />);
}

describe("BrowseLayout", () => {
  test("renders page content alongside the filter rail column", async () => {
    await renderLayout();
    await expect.element(page.getByText("PAGE_CONTENT")).toBeInTheDocument();
    await expect.element(page.getByRole("complementary", { name: "Filters" })).toBeInTheDocument();
  });

  test("collapsing the rail removes the desktop column but keeps the page content", async () => {
    await renderLayout({ railCollapsed: true });
    await expect.element(page.getByText("PAGE_CONTENT")).toBeInTheDocument();
    await expect
      .element(page.getByRole("complementary", { name: "Filters" }))
      .not.toBeInTheDocument();
  });

  test("hosts no rail trigger of its own", async () => {
    await renderLayout();
    await expect.element(page.getByRole("button", { name: /Refine/ })).not.toBeInTheDocument();
  });

  test("the controlled sheet carries the same rail; esc closes through the callback", async () => {
    const onSheetOpenChange = vi.fn();
    await renderLayout({ initialSheetOpen: true, onSheetOpenChange });
    const user = userEvent;
    const sheet = page.getByRole("dialog", { name: /Filters/ });
    await expect.element(sheet.getByText("RAIL_CONTENT")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(onSheetOpenChange).toHaveBeenCalledWith(false);
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });
});
