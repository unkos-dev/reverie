import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { createMemoryRouter, RouterProvider } from "react-router";
import type { ReactElement } from "react";

import { listShelves, type Shelf } from "@/api";
import { useAuthMe } from "@/hooks/useAuthMe";

import { LeftRail } from "./LeftRail";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/api")>();
  return { ...original, listShelves: vi.fn() };
});

vi.mock("@/hooks/useAuthMe", () => ({ useAuthMe: vi.fn() }));

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

const listShelvesMock = vi.mocked(listShelves);
const useAuthMeMock = vi.mocked(useAuthMe);

function makeShelf(i: number): Shelf {
  return {
    id: `shelf-${String(i)}`,
    name: `Shelf ${String(i)}`,
    is_system: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    item_count: i * 3,
  };
}

function authState(role: "admin" | "adult" | "child" | undefined): ReturnType<typeof useAuthMe> {
  if (role === undefined) return { data: undefined, isLoading: true, isError: false };
  return {
    data: {
      id: "u-1",
      display_name: "Reader",
      email: null,
      role,
      is_child: role === "child",
      theme_preference: "system",
      csrf_token: null,
    },
    isLoading: false,
    isError: false,
  };
}

function renderRail(initialEntry = "/library"): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter([{ path: "*", element: <LeftRail /> }], {
    initialEntries: [initialEntry],
  });
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
}

beforeEach(() => {
  listShelvesMock.mockResolvedValue([]);
  useAuthMeMock.mockReturnValue(authState("adult"));
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("LeftRail — primary navigation", () => {
  test("renders live destinations as links inside the Primary nav landmark", () => {
    renderRail();
    const nav = screen.getByRole("navigation", { name: "Primary" });
    expect(within(nav).getByRole("link", { name: "Library" })).toHaveAttribute("href", "/library");
    expect(within(nav).getByRole("link", { name: "Shelves" })).toHaveAttribute("href", "/shelves");
  });

  test("disabled entries are visible, non-links, out of tab order, with planned description", () => {
    renderRail();
    for (const label of ["Home", "Stats"]) {
      const item = screen.getByText(label);
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item.closest("a")).toBeNull();
      expect(item).not.toHaveAttribute("tabindex");
      expect(item).toHaveAccessibleDescription("Planned — not in this release");
    }
  });

  test("marks the active route with aria-current=page", () => {
    renderRail("/library");
    expect(screen.getByRole("link", { name: "Library" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Shelves" })).not.toHaveAttribute("aria-current");
  });

  test("renders the lockup linking to /library", () => {
    renderRail();
    expect(screen.getByRole("img", { name: "Reverie" }).closest("a")).toHaveAttribute(
      "href",
      "/library",
    );
  });
});

describe("LeftRail — shelves", () => {
  test("lists shelves with counts under the Shelves entry", async () => {
    listShelvesMock.mockResolvedValue([makeShelf(1), makeShelf(2)]);
    renderRail();
    expect(await screen.findByRole("link", { name: /Shelf 1/ })).toHaveAttribute(
      "href",
      "/shelves/shelf-1",
    );
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
  });

  test("caps visible shelves at 7 with an All-shelves overflow link", async () => {
    listShelvesMock.mockResolvedValue(Array.from({ length: 9 }, (_, i) => makeShelf(i + 1)));
    renderRail();
    await screen.findByRole("link", { name: /Shelf 1/ });
    expect(screen.queryByRole("link", { name: /Shelf 8/ })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: /All shelves/ })).toHaveAttribute("href", "/shelves");
  });

  test("shelf fetch failure leaves the nav intact with no shelf rows", async () => {
    listShelvesMock.mockRejectedValue(new Error("network down"));
    renderRail();
    await waitFor(() => {
      expect(listShelvesMock).toHaveBeenCalled();
    });
    expect(screen.getByRole("link", { name: "Library" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Shelf/ })).not.toBeInTheDocument();
  });
});

describe("LeftRail — admin cluster", () => {
  test("renders Users and Ingestion for an admin", () => {
    useAuthMeMock.mockReturnValue(authState("admin"));
    renderRail();
    const group = screen.getByRole("group", { name: "Admin" });
    expect(within(group).getByRole("link", { name: "Users" })).toHaveAttribute(
      "href",
      "/admin/users",
    );
    expect(within(group).getByRole("link", { name: "Ingestion" })).toHaveAttribute(
      "href",
      "/admin/dashboard",
    );
  });

  test.each(["adult", "child"] as const)("absent for role=%s", (role) => {
    useAuthMeMock.mockReturnValue(authState(role));
    renderRail();
    expect(screen.queryByRole("group", { name: "Admin" })).not.toBeInTheDocument();
  });

  test("absent while authn is pending (no admin flash)", () => {
    useAuthMeMock.mockReturnValue(authState(undefined));
    renderRail();
    expect(screen.queryByRole("group", { name: "Admin" })).not.toBeInTheDocument();
  });
});
