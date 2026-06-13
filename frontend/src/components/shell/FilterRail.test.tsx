import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";
import { createMemoryRouter, RouterProvider } from "react-router";

import { FilterRail } from "./FilterRail";

afterEach(() => {
  vi.clearAllMocks();
});

const SERIES = [
  { id: "s-1", name: "Discworld" },
  { id: "s-2", name: "Culture" },
];
const AUTHORS = ["Terry Pratchett", "Iain M. Banks"];

function renderRail(initialEntry = "/library"): ReturnType<typeof createMemoryRouter> {
  const router = createMemoryRouter(
    [
      {
        path: "/library",
        element: <FilterRail seriesOptions={SERIES} authorNames={AUTHORS} />,
      },
    ],
    { initialEntries: [initialEntry] },
  );
  render(<RouterProvider router={router} />);
  return router;
}

describe("FilterRail — series facet (live)", () => {
  test("selecting a series sets ?series= with radio semantics", async () => {
    const router = renderRail();
    const user = userEvent.setup();
    const rail = screen.getByRole("complementary", { name: "Filters" });
    const radio = within(rail).getByRole("radio", { name: "Discworld" });
    await user.click(radio);
    expect(new URLSearchParams(router.state.location.search).get("series")).toBe("s-1");
    expect(radio).toBeChecked();
  });

  test("selecting the active series again clears the filter", async () => {
    const router = renderRail("/library?series=s-1");
    const user = userEvent.setup();
    const radio = screen.getByRole("radio", { name: "Discworld" });
    expect(radio).toBeChecked();
    await user.click(radio);
    expect(new URLSearchParams(router.state.location.search).get("series")).toBeNull();
  });

  test("Clear resets the series param and drops any cursor", async () => {
    const router = renderRail("/library?series=s-2&cursor=abc");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Clear/ }));
    const search = new URLSearchParams(router.state.location.search);
    expect(search.get("series")).toBeNull();
    expect(search.get("cursor")).toBeNull();
  });

  test("series radios are keyboard operable", async () => {
    const router = renderRail();
    const user = userEvent.setup();
    await user.tab();
    expect(screen.getByRole("radio", { name: "Discworld" })).toHaveFocus();
    await user.keyboard(" ");
    expect(new URLSearchParams(router.state.location.search).get("series")).toBe("s-1");
  });
});

describe("FilterRail — author placeholder (deferred to UNK-387)", () => {
  test("renders real author names disabled, out of tab order, with planned description", () => {
    renderRail();
    for (const name of AUTHORS) {
      const row = screen.getByText(name).closest('[aria-disabled="true"]');
      expect(row).not.toBeNull();
      expect(row).not.toHaveAttribute("tabindex");
      // Descendant query — closest() walks up and would pass even with
      // a nested interactive control inside the row.
      expect(
        row?.querySelector('a,button,input,select,textarea,[tabindex]:not([tabindex="-1"])'),
      ).toBeNull();
      expect(row).toHaveAccessibleDescription("Planned — not in this release");
    }
  });
});
