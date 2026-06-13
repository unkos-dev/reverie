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
import type { TitleData } from "@/components/shell/crumbs";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { SeriesPage } from "@/pages/series/SeriesPage";

/**
 * Loader for `/series/:id`. Prefetches the series detail so the
 * page's `useSuspenseQuery` hits a hot cache, then returns `{ title }`
 * for the utility strip's breadcrumb. `prefetchQuery` swallows fetch
 * errors — a cold cache degrades to `null` (crumb renders "Library"
 * alone).
 *
 * Throws a `Response` on a missing id — react-router's documented
 * loader-bailout mechanism (same guard as the `/b/:id` loader); the
 * alternative would send `GET /series/` with an empty id segment.
 */
export async function loader({ params }: LoaderFunctionArgs): Promise<TitleData | null> {
  const id = params.id;
  if (typeof id !== "string" || id.length === 0) {
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- react-router's loader-bailout convention is `throw new Response(...)`.
    throw new Response("Missing series id", { status: 404 });
  }
  await queryClient.prefetchQuery({
    queryKey: queryKeys.series.detail(id),
    queryFn: ({ signal }) => getSeries(id, signal),
  });
  const detail = queryClient.getQueryData<SeriesDetail>(queryKeys.series.detail(id));
  return detail === undefined ? null : ({ title: detail.name } satisfies TitleData);
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = SeriesPage;
