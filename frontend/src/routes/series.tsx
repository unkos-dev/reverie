/**
 * Route module for `/series/:id`. Exports `loader`
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

import { getSeries, type SeriesDetail } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { SeriesPage } from "@/pages/series/SeriesPage";

/**
 * Loader for `/series/:id`. Prefetches the series detail so the
 * page's `useSuspenseQuery` hits a hot cache, then returns `{ title }`
 * for the utility strip's breadcrumb. `prefetchQuery` swallows fetch
 * errors — a cold cache degrades to `null` (crumb renders "Library"
 * alone).
 */
export async function loader({ params }: LoaderFunctionArgs): Promise<{ title: string } | null> {
  const id = params.id ?? "";
  await queryClient.prefetchQuery({
    queryKey: queryKeys.series.detail(id),
    queryFn: ({ signal }) => getSeries(id, signal),
  });
  const detail = queryClient.getQueryData<SeriesDetail>(queryKeys.series.detail(id));
  return detail === undefined ? null : { title: detail.name };
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = SeriesPage;
