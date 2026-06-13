import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vitest";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";

import { openCommandPalette } from "@/lib/command-palette";

import { titleCrumb } from "./crumbs";
import { UtilityStrip } from "./UtilityStrip";

vi.mock("@/lib/command-palette", () => ({
  openCommandPalette: vi.fn(),
  searchHintLabel: () => "Ctrl K",
}));

const openMock = vi.mocked(openCommandPalette);

afterEach(() => {
  vi.clearAllMocks();
});

function renderStrip(routes: RouteObject[], initialEntry: string): void {
  const router = createMemoryRouter(routes, { initialEntries: [initialEntry] });
  render(<RouterProvider router={router} />);
}

describe("UtilityStrip — search affordance", () => {
  test("renders the field-shaped button with hint and opens the palette on click", async () => {
    renderStrip([{ path: "/library", element: <UtilityStrip /> }], "/library");
    const user = userEvent.setup();
    const button = screen.getByRole("button", { name: /Search the library/ });
    expect(button).toHaveTextContent("Ctrl K");
    await user.click(button);
    expect(openMock).toHaveBeenCalledTimes(1);
  });
});

describe("UtilityStrip — breadcrumbs", () => {
  test("renders Library › title when the matched route carries a crumb handle", async () => {
    renderStrip(
      [
        {
          path: "/b/:id",
          element: <UtilityStrip />,
          loader: () => ({ title: "Stoner" }),
          handle: { crumb: titleCrumb },
        },
      ],
      "/b/abc",
    );
    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(nav).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Library" })).toHaveAttribute("href", "/library");
    expect(screen.getByText("Stoner")).toBeInTheDocument();
  });

  test("no breadcrumb on routes without a crumb handle", () => {
    renderStrip([{ path: "/library", element: <UtilityStrip /> }], "/library");
    expect(screen.queryByRole("navigation", { name: "Breadcrumb" })).not.toBeInTheDocument();
  });

  test("degrades to Library alone and logs when the crumb function throws", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    renderStrip(
      [
        {
          path: "/b/:id",
          element: <UtilityStrip />,
          loader: () => ({ title: "Ignored" }),
          handle: {
            crumb: () => {
              throw new Error("malformed loader data");
            },
          },
        },
      ],
      "/b/abc",
    );
    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(nav).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Library" })).toBeInTheDocument();
    expect(screen.queryByText("Ignored")).not.toBeInTheDocument();
    expect(errorSpy).toHaveBeenCalledWith("[UtilityStrip] crumb function threw", expect.any(Error));
  });

  test("degrades to Library alone when loader data is null (cold cache)", async () => {
    renderStrip(
      [
        {
          path: "/b/:id",
          element: <UtilityStrip />,
          loader: () => null,
          handle: { crumb: titleCrumb },
        },
      ],
      "/b/abc",
    );
    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(nav).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Library" })).toBeInTheDocument();
    expect(screen.queryByText("Stoner")).not.toBeInTheDocument();
  });
});
