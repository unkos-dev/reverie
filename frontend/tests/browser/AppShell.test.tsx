import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";

import { AppShell } from "@/components/shell/AppShell";

vi.mock("@/components/shell/LeftRail", () => ({
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

async function renderShell(
  initialEntry = "/library",
  extraRoutes: RouteObject[] = [],
): Promise<void> {
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
  await render(<RouterProvider router={router} />);
}

describe("AppShell — structure", () => {
  test("renders children inside the main landmark", async () => {
    await renderShell();
    const main = page.getByRole("main");
    await expect.element(main).toHaveAttribute("id", "main");
    await expect.element(main).toHaveTextContent("PAGE_CONTENT");
  });

  test("skip-to-content link is first in tab order and targets #main", async () => {
    await renderShell();
    const user = userEvent;
    await user.tab();
    const skip = page.getByRole("link", { name: /Skip to content/ });
    await expect.element(skip).toHaveFocus();
    await expect.element(skip).toHaveAttribute("href", "#main");
  });

  test("renders the navigation rail", async () => {
    await renderShell();
    await expect.element(page.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
  });
});

describe("AppShell — drawer (<1024px)", () => {
  test("menu button opens a focus-trapped drawer with the rail; esc closes and restores focus", async () => {
    await page.viewport(800, 800);
    await renderShell();
    const user = userEvent;
    const menuButton = page.getByRole("button", { name: /Open navigation/ });
    await user.click(menuButton);
    const drawer = page.getByRole("dialog", { name: /Navigation/ });
    await expect.element(drawer).toBeInTheDocument();

    await expect.element(drawer.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await expect.element(menuButton).toHaveFocus();
  });
});

describe("AppShell — admin zone", () => {
  test("admin routes flip the zone attribute", async () => {
    await renderShell("/admin/users");
    expect(document.querySelector('[data-zone="admin"]')).not.toBeNull();
  });

  test("non-admin routes carry no zone attribute", async () => {
    await renderShell("/library");
    expect(document.querySelector('[data-zone="admin"]')).toBeNull();
  });
});
