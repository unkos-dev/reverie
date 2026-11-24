/* oxlint-disable react/only-export-components */
import type { PreloadedQuery } from "react-relay";
import type { LoaderFunctionArgs } from "react-router";

import { preloadRouteQuery } from "@/lib/relay/environment";
import bookPageQuery, { type BookPageQuery } from "@/lib/relay/__generated__/BookPageQuery.graphql";

export function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<PreloadedQuery<BookPageQuery> | null> {
  const id = params.id;
  if (id === undefined) throw new Response("Not found", { status: 404 });
  return preloadRouteQuery<BookPageQuery>(bookPageQuery, { id }, request.signal);
}

export { BookPage as Component } from "@/pages/book/BookPage";
