import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { render } from "vitest-browser-react";
import { page } from "vite-plus/test/browser";
import { describe, expect, test } from "vite-plus/test";

import type { SeriesDetail, SeriesWorkManifestation } from "@/api";
import { queryKeys } from "@/lib/query/keys";
import { SeriesPage } from "@/pages/series/SeriesPage";

function manifestation(id: string): SeriesWorkManifestation {
  return {
    id,
    isbn_13: null,
    isbn_10: null,
    cover_url: "/brand/favicon-32.png",
    ingestion_status: "complete",
    validation_status: "clean",
    enrichment_status: "complete",
    created_at: "2026-01-01T00:00:00Z",
  };
}

function seriesFixture(): SeriesDetail {
  return {
    id: "00000000-0000-4000-8000-000000000001",
    name: "Recorded Series",
    sort_name: "recorded series",
    works: [
      {
        id: "00000000-0000-4000-8000-000000000002",
        title: "Second Book",
        authors: ["First Author", "Second Author"],
        position: 2,
        manifestations: [manifestation("00000000-0000-4000-8000-000000000003")],
      },
      {
        id: "00000000-0000-4000-8000-000000000004",
        title: "Fourth Book",
        authors: ["Fourth Author"],
        position: 4,
        manifestations: [manifestation("00000000-0000-4000-8000-000000000005")],
      },
    ],
  };
}

async function renderSeries(series: SeriesDetail): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  client.setQueryData(queryKeys.series.detail(series.id), series);
  const routes: RouteObject[] = [{ path: "/series/:id", element: <SeriesPage /> }];
  const router = createMemoryRouter(routes, { initialEntries: [`/series/${series.id}`] });
  await render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("SeriesPage", () => {
  test("preserves recorded metadata and book links without an ownership fraction", async () => {
    await renderSeries(seriesFixture());
    await expect
      .element(page.getByRole("heading", { name: "Recorded Series" }))
      .toBeInTheDocument();
    await expect
      .element(page.getByText("First Author, Second Author", { exact: true }))
      .toBeInTheDocument();
    await expect.element(page.getByText("Fourth Author", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText("#2", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText("#4", { exact: true })).toBeInTheDocument();
    await expect
      .element(page.getByRole("link", { name: "Second Book", exact: true }))
      .toHaveAttribute("href", "/b/00000000-0000-4000-8000-000000000003");
    await expect
      .element(page.getByRole("link", { name: "Fourth Book", exact: true }))
      .toHaveAttribute("href", "/b/00000000-0000-4000-8000-000000000005");
    await page.screenshot({ path: "../../test-results/browser/series-two-books.png" });
    await expect.element(page.getByText(/^[0-9]+ of [0-9]+ owned$/)).not.toBeInTheDocument();
  });

  test("preserves a work with multiple manifestations without an ownership fraction", async () => {
    const series = seriesFixture();
    series.works[0]?.manifestations.push(manifestation("00000000-0000-4000-8000-000000000006"));
    await renderSeries(series);
    await expect
      .element(page.getByRole("heading", { name: "Recorded Series" }))
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("link", { name: "Second Book", exact: true }))
      .toHaveAttribute("href", "/b/00000000-0000-4000-8000-000000000003");
    await expect
      .element(page.getByText("First Author, Second Author", { exact: true }))
      .toBeInTheDocument();
    await expect.element(page.getByText("#2", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText(/^[0-9]+ of [0-9]+ owned$/)).not.toBeInTheDocument();
  });

  test("preserves adult catalogue metadata with no visible manifestation without an ownership fraction", async () => {
    const series = seriesFixture();
    series.works = series.works.map((work, index) =>
      index === 0 ? { ...work, manifestations: [] } : work,
    );
    await renderSeries(series);
    await expect
      .element(page.getByRole("heading", { name: "Recorded Series" }))
      .toBeInTheDocument();
    await expect.element(page.getByText("Second Book", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText("Fourth Book", { exact: true })).toBeInTheDocument();
    await expect
      .element(page.getByText("First Author, Second Author", { exact: true }))
      .toBeInTheDocument();
    await expect.element(page.getByText("#2", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText("#4", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText(/^[0-9]+ of [0-9]+ owned$/)).not.toBeInTheDocument();
  });
});
