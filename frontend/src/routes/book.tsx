/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerDetailQuery } from "@/lib/relay/reader";
import type { readerDetailQuery as ReaderQuery } from "@/lib/relay/__generated__/readerDetailQuery.graphql";

/** Prepare the route's reader query and breadcrumb before rendering. */
export async function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<(PreparedReader<ReaderQuery> & { title: string | null }) | null> {
  const id = params.id;
  if (id === undefined) throw new Response("Not found", { status: 404 });
  const prepared = await prepareReader<ReaderQuery>(readerDetailQuery, { id }, request.signal);
  if (prepared !== null && prepared.data?.manifestation == null) {
    prepared.dispose();
    throw new Response("Not found", { status: 404 });
  }
  return prepared === null
    ? null
    : { ...prepared, title: prepared.data?.manifestation?.work?.title ?? null };
}

export { BookPage as Component } from "@/pages/book/BookPage";
