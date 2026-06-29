import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";

import { AppShell } from "./AppShell";

vi.mock("./LeftRail", () => ({
  LeftRail: () => (
    <nav aria-label="Primary">
      <a href="/library">Library</a>
    </nav>
  ),
}));

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

afterEach(() => {
  vi.clearAllMocks();
});

function renderShell(initialEntry = "/library", extraRoutes: RouteObject[] = []): void {
  const routes: RouteObject[] = [
    {
      path: "/",
      element: (
        <AppShell>
          <p>PAGE_CONTENT</p>
        </AppShell>
      ),
      children: [
        { path: "library", element: <span /> },
        { path: "admin/users", element: <span />, handle: { zone: "admin" } },
        ...extraRoutes,
      ],
    },
  ];
  const router = createMemoryRouter(routes, { initialEntries: [initialEntry] });
  render(<RouterProvider router={router} />);
}

describe("AppShell — structure", () => {
  test("renders children inside the main landmark", () => {
    renderShell();
    const main = screen.getByRole("main");
    expect(main).toHaveAttribute("id", "main");
    expect(main).toHaveTextContent("PAGE_CONTENT");
  });

  test("skip-to-content link is first in tab order and targets #main", async () => {
    renderShell();
    const user = userEvent.setup();
    await user.tab();
    const skip = screen.getByRole("link", { name: /Skip to content/ });
    expect(skip).toHaveFocus();
    expect(skip).toHaveAttribute("href", "#main");
  });

  test("renders the navigation rail", () => {
    renderShell();
    expect(screen.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
  });
});

describe("AppShell — drawer (<1024px)", () => {
  test("menu button opens a focus-trapped drawer with the rail; esc closes and restores focus", async () => {
    renderShell();
    const user = userEvent.setup();
    const menuButton = screen.getByRole("button", { name: /Open navigation/ });
    await user.click(menuButton);
    const drawer = await screen.findByRole("dialog", { name: /Navigation/ });
    expect(drawer).toBeInTheDocument();
    // The modal drawer aria-hides everything outside it (incl. the
    // desktop rail), so the only reachable Primary nav is the drawer's.
    expect(within(drawer).getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(menuButton).toHaveFocus();
  });

  test("clicking a nav link inside the drawer closes it", async () => {
    renderShell();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Open navigation/ }));
    const drawer = await screen.findByRole("dialog", { name: /Navigation/ });
    await user.click(within(drawer).getByRole("link", { name: "Library" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});

describe("AppShell — admin zone", () => {
  test("admin routes flip the zone attribute", () => {
    renderShell("/admin/users");
    expect(document.querySelector('[data-zone="admin"]')).not.toBeNull();
  });

  test("non-admin routes carry no zone attribute", () => {
    renderShell("/library");
    expect(document.querySelector('[data-zone="admin"]')).toBeNull();
  });
});
