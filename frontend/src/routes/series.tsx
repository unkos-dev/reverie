/* oxlint-disable react/only-export-components */
import type { PreloadedQuery } from "react-relay";
import type { LoaderFunctionArgs } from "react-router";

import { preloadRouteQuery } from "@/lib/relay/environment";
import seriesPageQuery, {
  type SeriesPageQuery,
} from "@/lib/relay/__generated__/SeriesPageQuery.graphql";

export function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<PreloadedQuery<SeriesPageQuery> | null> {
  const id = params.id;
  if (id === undefined) throw new Response("Not found", { status: 404 });
  return preloadRouteQuery<SeriesPageQuery>(seriesPageQuery, { id }, request.signal);
}

export { SeriesPage as Component } from "@/pages/series/SeriesPage";
