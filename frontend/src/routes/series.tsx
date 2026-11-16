/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { encodeGraphQLId } from "@/api/graphql";
import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerSeriesQuery } from "@/lib/relay/reader";
import type { readerSeriesQuery as ReaderQuery } from "@/lib/relay/__generated__/readerSeriesQuery.graphql";

/** Prepare the route's reader query and breadcrumb before rendering. */
export async function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<(PreparedReader<ReaderQuery> & { title: string | null }) | null> {
  const id = encodeGraphQLId("Series", params.id ?? "");
  const prepared = await prepareReader<ReaderQuery>(readerSeriesQuery, { id }, request.signal);
  return prepared === null ? null : { ...prepared, title: prepared.data?.series?.name ?? null };
}

export { SeriesPage as Component } from "@/pages/series/SeriesPage";
