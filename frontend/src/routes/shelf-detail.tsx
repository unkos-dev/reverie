/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";
import { z } from "zod";

import { encodeGraphQLId } from "@/api/graphql";
import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerShelfQuery } from "@/lib/relay/reader";
import type { readerShelfQuery as ReaderQuery } from "@/lib/relay/__generated__/readerShelfQuery.graphql";

/** Prepare the caller-owned shelf read before rendering. */
export async function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<PreparedReader<ReaderQuery> | null> {
  const parsed = z.uuid().safeParse(params.id);
  if (!parsed.success) throw new Response("Not found", { status: 404 });
  const prepared = await prepareReader<ReaderQuery>(
    readerShelfQuery,
    { id: encodeGraphQLId("Shelf", parsed.data) },
    request.signal,
  );
  if (prepared !== null && prepared.data?.shelf == null) {
    prepared.dispose();
    throw new Response("Not found", { status: 404 });
  }
  return prepared;
}

export { ShelfDetailPage as Component } from "@/pages/shelves/ShelfDetailPage";
