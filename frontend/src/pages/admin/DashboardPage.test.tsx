import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { DashboardStats, DashboardActivity } from "@/api/dashboard";
import { getDashboardStats, getDashboardActivity } from "@/api/dashboard";
import { ACTIVITY_LIMIT } from "./DashboardPage";

import { DashboardPage } from "./DashboardPage";

// Spy on the API client so error states can be driven deterministically.
// Both default to rejecting: seeded-cache tests render from the primed cache,
// and their background refetch rejecting (not resolving) keeps that data in
// place rather than overwriting it with `undefined`.
vi.mock("@/api/dashboard", () => ({
  getDashboardStats: vi.fn().mockRejectedValue(new Error("unmocked stats call")),
  getDashboardActivity: vi.fn().mockRejectedValue(new Error("unmocked activity call")),
}));

const ADMIN_ME = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin" as const,
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
};

const ADULT_ME = { ...ADMIN_ME, role: "adult" as const };

function statsFixture(overrides: Partial<DashboardStats> = {}): DashboardStats {
  return {
    total_manifestations: 1204,
    total_works: 980,
    storage_total_bytes: 3_650_000_000,
    storage_cover_bytes: 41_000_000,
    storage_by_format: [{ format: "epub", count: 1100, bytes: 3_200_000_000 }],
    validation_breakdown: [
      { status: "pending", count: 2 },
      { status: "clean", count: 1190 },
      { status: "repaired", count: 7 },
      { status: "degraded", count: 5 },
    ],
    clean_non_epub_count: 104,
    enrichment_breakdown: [
      { status: "pending", count: 10 },
      { status: "in_progress", count: 0 },
      { status: "complete", count: 1180 },
      { status: "failed", count: 9 },
      { status: "skipped", count: 5 },
    ],
    metadata_coverage: {
      total: 1204,
      has_description: 1100,
      has_language: 1190,
      has_isbn_13: 940,
      has_cover: 1112,
    },
    ...overrides,
  };
}

function activityFixture(): DashboardActivity {
  return {
    batches: [
      {
        batch_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        started_at: "2026-06-05T10:00:00Z",
        ended_at: "2026-06-05T10:02:11Z",
        total: 42,
        completed: 40,
        failed: 1,
        skipped: 1,
        in_progress: 0,
      },
    ],
  };
}

function renderDashboard(
  meData: typeof ADMIN_ME | typeof ADULT_ME | null,
  stats: DashboardStats | null,
  activity: DashboardActivity | null,
): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (meData !== null) client.setQueryData(queryKeys.auth.me(), meData);
  if (stats !== null) client.setQueryData(queryKeys.dashboard.stats(), stats);
  if (activity !== null)
    client.setQueryData(queryKeys.dashboard.activity(ACTIVITY_LIMIT), activity);

  const routes: RouteObject[] = [
    { path: "/admin/dashboard", element: <DashboardPage /> },
    { path: "/library", element: <div data-testid="library-page">Library</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/admin/dashboard"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
}

describe("DashboardPage", () => {
  test("renders heading and book total for admin", async () => {
    renderDashboard(ADMIN_ME, statsFixture(), activityFixture());
    expect(await screen.findByRole("heading", { name: /library health/i })).toBeInTheDocument();
    expect(screen.getByText("1,204")).toBeInTheDocument();
  });

  test("redirects non-admin to /library", async () => {
    renderDashboard(ADULT_ME, null, null);
    expect(await screen.findByTestId("library-page")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /library health/i })).not.toBeInTheDocument();
  });

  test("renders recent ingestion batch row", async () => {
    renderDashboard(ADMIN_ME, statsFixture(), activityFixture());
    // Batch total (42) appears in the activity table.
    expect(await screen.findByText("42")).toBeInTheDocument();
  });

  test("footnotes the clean bucket with non-epub count", async () => {
    renderDashboard(ADMIN_ME, statsFixture(), activityFixture());
    expect(await screen.findByText(/104/)).toBeInTheDocument();
  });

  test("surfaces an error when stats fail to load", async () => {
    vi.mocked(getDashboardStats).mockRejectedValueOnce(new Error("stats boom"));
    renderDashboard(ADMIN_ME, null, { batches: [] });
    expect(await screen.findByText(/failed to load metrics/i)).toBeInTheDocument();
  });

  test("surfaces an error when ingestion activity fails to load", async () => {
    vi.mocked(getDashboardActivity).mockRejectedValueOnce(new Error("activity boom"));
    renderDashboard(ADMIN_ME, statsFixture(), null);
    expect(await screen.findByText(/failed to load ingestion activity/i)).toBeInTheDocument();
  });

  test("renders empty library without dividing by zero", async () => {
    const empty = statsFixture({
      total_manifestations: 0,
      total_works: 0,
      storage_total_bytes: 0,
      storage_by_format: [],
      metadata_coverage: {
        total: 0,
        has_description: 0,
        has_language: 0,
        has_isbn_13: 0,
        has_cover: 0,
      },
    });
    renderDashboard(ADMIN_ME, empty, { batches: [] });
    expect(await screen.findByRole("heading", { name: /library health/i })).toBeInTheDocument();
    // Coverage shows 0% (not NaN%) on an empty library.
    expect(screen.queryByText(/NaN/)).not.toBeInTheDocument();
  });
});
