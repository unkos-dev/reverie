/* oxlint-disable react/only-export-components */
import type { PreloadedQuery } from "react-relay";
import type { LoaderFunctionArgs } from "react-router";

import { preloadRouteQuery } from "@/lib/relay/environment";
import shelvesListPageQuery, {
  type ShelvesListPageQuery,
} from "@/lib/relay/__generated__/ShelvesListPageQuery.graphql";

export function loader({
  request,
}: LoaderFunctionArgs): Promise<PreloadedQuery<ShelvesListPageQuery> | null> {
  return preloadRouteQuery<ShelvesListPageQuery>(shelvesListPageQuery, {}, request.signal);
}

export { ShelvesListPage as Component } from "@/pages/shelves/ShelvesListPage";
