/**
 * Route module for `/series/:id` (sub-phase 11d). Exports `loader`
 * (prefetches the series detail into the shared `QueryClient`) and
 * `Component` (the page renderer).
 *
 * react-router data-mode requires loader and component in the same
 * module — the `react-refresh/only-export-components` rule is
 * disabled at file scope because data routes have no fast-refresh-
 * friendly alternative.
 */
/* eslint-disable react-refresh/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { getSeries } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { SeriesPage } from "@/pages/series/SeriesPage";

/**
 * Loader for `/series/:id`. Prefetches the series detail so the
 * page's `useSuspenseQuery` hits a hot cache.
 */
export async function loader({ params }: LoaderFunctionArgs): Promise<null> {
  const id = params.id ?? "";
  await queryClient.prefetchQuery({
    queryKey: queryKeys.series.detail(id),
    queryFn: ({ signal }) => getSeries(id, signal),
  });
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = SeriesPage;
