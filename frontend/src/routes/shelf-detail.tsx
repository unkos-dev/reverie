/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { encodeGraphQLId } from "@/api/graphql";
import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerShelfQuery } from "@/lib/relay/reader";
import type { readerShelfQuery as ReaderQuery } from "@/lib/relay/__generated__/readerShelfQuery.graphql";

/** Prepare the caller-owned shelf read before rendering. */
export async function loader({
  params,
  request,
}: LoaderFunctionArgs): Promise<PreparedReader<ReaderQuery> | null> {
  return prepareReader<ReaderQuery>(
    readerShelfQuery,
    { id: encodeGraphQLId("Shelf", params.id ?? "") },
    request.signal,
  );
}

export { ShelfDetailPage as Component } from "@/pages/shelves/ShelfDetailPage";
