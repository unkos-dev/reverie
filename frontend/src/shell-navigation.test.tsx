/**
 * Click-reachability integration test:
 * every routed screen reachable by click from `/library`, over the
 * REAL production route set with the network stubbed at `fetch`.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { createMemoryRouter, Navigate, RouterProvider, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import App from "./App";
import { queryClient, setUnauthenticatedHandler } from "./lib/query/client";
import {
  adminDashboardRoute,
  adminUsersRoute,
  bookRoute,
  libraryRoute,
  seriesRoute,
  shelfDetailRoute,
  shelvesRoute,
} from "./routes/production";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ preference: "system", effective: "dark", setPreference: vi.fn() }),
}));

const ADMIN_ME = {
  // Must parse under Zod 4's z.uuid(), which enforces RFC 4122
  // version/variant bits — an all-zeros id fails silently into the
  // "logged out" state.
  id: "11111111-1111-4111-8111-111111111111",
  display_name: "Root Admin",
  email: null,
  role: "admin",
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
};

const SHELF = {
  id: "shelf-1",
  name: "Reading List",
  is_system: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  item_count: 3,
};

const BOOK_LIST_ITEM = {
  id: "b1",
  work_id: "w1",
  title: "Guards! Guards!",
  authors: ["Terry Pratchett"],
  series: { id: "series-1", name: "Discworld", position: 8 },
  isbn_13: null,
  cover_url: "",
  ingestion_status: "complete",
  validation_status: "clean",
  enrichment_status: "complete",
};

const FILTERED_BOOK_LIST_ITEM = {
  ...BOOK_LIST_ITEM,
  id: "b2",
  work_id: "w2",
  title: "Mort",
};

const BOOK_DETAIL = {
  ...BOOK_LIST_ITEM,
  description: "A dragon appears.",
  language: "en",
  isbn_10: null,
  publisher: null,
  pub_date: null,
  tags: [],
  metadata_version_summary: { pending: 0, accepted: 0 },
  metadata_versions: [],
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const STATS = {
  total_manifestations: 1,
  total_works: 1,
  storage_total_bytes: 0,
  storage_cover_bytes: 0,
  storage_by_format: [],
  validation_breakdown: [],
  clean_non_epub_count: 0,
  enrichment_breakdown: [],
  metadata_coverage: {
    total: 1,
    has_description: 1,
    has_language: 1,
    has_isbn_13: 0,
    has_cover: 0,
  },
};

function bodyFor(url: URL, role: string): unknown {
  const pathname = url.pathname;
  if (pathname === "/auth/me") return { ...ADMIN_ME, role };
  if (pathname.startsWith("/api/v1/books/")) return BOOK_DETAIL;
  if (pathname.startsWith("/api/v1/books")) {
    // A series filter returns a distinct item so the seam test can
    // tell a refetch from a cached re-render.
    if (url.searchParams.get("series") === "series-1") {
      return { items: [FILTERED_BOOK_LIST_ITEM], next_cursor: null };
    }
    return { items: [BOOK_LIST_ITEM], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/shelves/")) {
    return { ...SHELF, items: [], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/shelves")) {
    return { items: [SHELF], next_cursor: null };
  }
  if (pathname.startsWith("/api/v1/series/")) {
    return { id: "series-1", name: "Discworld", sort_name: "discworld", works: [] };
  }
  if (pathname === "/api/v1/users") return [{ ...ADMIN_ME, ...USER_TIMESTAMPS }];
  if (pathname === "/api/v1/dashboard/stats") return STATS;
  if (pathname.startsWith("/api/v1/dashboard/activity")) return { batches: [] };
  // Fail fast — a silent `{}` for an unknown path would green-light
  // unintended network calls and hide route/data regressions.
  throw new Error(`Unhandled fetch path in shell-navigation.test.tsx: ${pathname}`);
}

const USER_TIMESTAMPS = {
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

function stubFetch(role: string): void {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = new URL(
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
      "http://localhost",
    );
    const body = bodyFor(url, role);
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
  });
}

function renderApp(): { router: ReturnType<typeof createMemoryRouter> } {
  const routes: RouteObject[] = [
    {
      path: "/",
      element: <App />,
      children: [
        { index: true, element: <Navigate to="/library" replace /> },
        libraryRoute,
        bookRoute,
        seriesRoute,
        shelvesRoute,
        shelfDetailRoute,
        adminUsersRoute,
        adminDashboardRoute,
      ],
    },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/library"] });
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return { router };
}

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  vi.restoreAllMocks();
});

describe("shell navigation — click reachability (admin)", () => {
  test("every routed screen is reachable by click from /library", async () => {
    stubFetch("admin");
    const { router } = renderApp();
    const user = userEvent.setup();

    // Library grid renders.
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();

    // Book detail via grid tile.
    await user.click(await screen.findByRole("link", { name: /Guards! Guards!/ }));
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/b/b1");
      },
      { timeout: 5000 },
    );

    // Series detail via the book's series label. The heading settle
    // alone is insufficient — the library tile's own h3 satisfies it
    // before the book page commits, so the series link itself must be
    // awaited (a sync getByRole races the post-navigation render
    // flush and loses on slower machines).
    await screen.findByRole("heading", { name: /Guards! Guards!/ });
    await user.click(
      await screen.findByRole("link", { name: /Discworld · #8/ }, { timeout: 5000 }),
    );
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/series/series-1");
      },
      { timeout: 5000 },
    );

    // Shelves index via the rail.
    await user.click(screen.getByRole("link", { name: "Shelves" }));
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/shelves");
      },
      { timeout: 5000 },
    );

    // Shelf detail via the rail's shelf row (scoped to the rail
    // landmark — the shelves index page lists the same shelf).
    const rail = screen.getByRole("navigation", { name: "Primary" });
    await user.click(
      await within(rail).findByRole("link", { name: /Reading List/ }, { timeout: 5000 }),
    );
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/shelves/shelf-1");
      },
      { timeout: 5000 },
    );

    // Admin: users + ingestion via the admin cluster.
    await user.click(await screen.findByRole("link", { name: "Users" }));
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/admin/users");
      },
      { timeout: 5000 },
    );
    await user.click(await screen.findByRole("link", { name: "Ingestion" }));
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/admin/dashboard");
      },
      { timeout: 5000 },
    );

    // Back to the library via the rail.
    await user.click(screen.getByRole("link", { name: "Library" }));
    await waitFor(
      () => {
        expect(router.state.location.pathname).toBe("/library");
      },
      { timeout: 5000 },
    );
  });
});

describe("filter rail — param-write to loader-refetch seam", () => {
  test("selecting a series facet refetches the grid with the filter applied", async () => {
    stubFetch("admin");
    const { router } = renderApp();
    const user = userEvent.setup();

    expect(await screen.findByRole("link", { name: /Guards! Guards!/ })).toBeInTheDocument();

    // Select the Discworld checkbox in the filter rail.
    await user.click(await screen.findByRole("checkbox", { name: "Discworld" }));
    await waitFor(
      () => {
        expect(new URLSearchParams(router.state.location.search).get("series")).toBe("series-1");
      },
      { timeout: 5000 },
    );

    // The query re-fired with the param and the grid re-rendered from
    // the filtered response — not from the unfiltered cache entry.
    expect(await screen.findByRole("link", { name: /Mort/ })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Guards! Guards!/ })).not.toBeInTheDocument();

    // Toggle-off: re-clicking the active checkbox clears the param and
    // restores the unfiltered grid (spec §10 active-filter clearing).
    await user.click(screen.getByRole("checkbox", { name: "Discworld" }));
    await waitFor(
      () => {
        expect(new URLSearchParams(router.state.location.search).get("series")).toBeNull();
      },
      { timeout: 5000 },
    );
    expect(await screen.findByRole("link", { name: /Guards! Guards!/ })).toBeInTheDocument();
  });
});

describe("shell navigation — role gating", () => {
  test("non-admin never sees the admin cluster", async () => {
    stubFetch("adult");
    renderApp();
    expect(await screen.findByRole("heading", { name: "Library" })).toBeInTheDocument();
    // Shelves resolved (rail settled) — admin links must still be absent.
    const rail = screen.getByRole("navigation", { name: "Primary" });
    await within(rail).findByRole("link", { name: /Reading List/ }, { timeout: 5000 });
    expect(screen.queryByRole("link", { name: "Users" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ingestion" })).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Admin" })).not.toBeInTheDocument();
  });
});
