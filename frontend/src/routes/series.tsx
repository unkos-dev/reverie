/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";
import { z } from "zod";

import { encodeGraphQLId } from "@/api/graphql";
import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerSeriesQuery } from "@/lib/relay/reader";
import type { readerSeriesQuery as ReaderQuery } from "@/lib/relay/__generated__/readerSeriesQuery.graphql";

/** Prepare the route's reader query and breadcrumb before rendering. */
export async function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<(PreparedReader<ReaderQuery> & { title: string | null }) | null> {
  const parsed = z.uuid().safeParse(params.id);
  if (!parsed.success) throw new Response("Not found", { status: 404 });
  const id = encodeGraphQLId("Series", parsed.data);
  const prepared = await prepareReader<ReaderQuery>(readerSeriesQuery, { id }, request.signal);
  if (prepared !== null && prepared.data?.series == null) {
    prepared.dispose();
    throw new Response("Not found", { status: 404 });
  }
  return prepared === null ? null : { ...prepared, title: prepared.data?.series?.name ?? null };
}

export { SeriesPage as Component } from "@/pages/series/SeriesPage";
