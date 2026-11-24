/* oxlint-disable react/only-export-components */
import type { PreloadedQuery } from "react-relay";
import type { LoaderFunctionArgs } from "react-router";

import { preloadRouteQuery } from "@/lib/relay/environment";
import shelfDetailPageQuery, {
  type ShelfDetailPageQuery,
} from "@/lib/relay/__generated__/ShelfDetailPageQuery.graphql";

export function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<PreloadedQuery<ShelfDetailPageQuery> | null> {
  const id = params.id;
  if (id === undefined) throw new Response("Not found", { status: 404 });
  return preloadRouteQuery<ShelfDetailPageQuery>(shelfDetailPageQuery, { id }, request.signal);
}

export { ShelfDetailPage as Component } from "@/pages/shelves/ShelfDetailPage";
