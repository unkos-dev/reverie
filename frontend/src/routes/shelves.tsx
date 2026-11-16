/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { prepareReader, type PreparedReader } from "@/lib/relay/environment";
import { readerShelvesQuery } from "@/lib/relay/reader";
import type { readerShelvesQuery as ReaderQuery } from "@/lib/relay/__generated__/readerShelvesQuery.graphql";

/** Prepare the caller-owned shelf read before rendering. */
export async function loader({
  request,
}: LoaderFunctionArgs): Promise<PreparedReader<ReaderQuery> | null> {
  return prepareReader<ReaderQuery>(readerShelvesQuery, {}, request.signal);
}

export { ShelvesListPage as Component } from "@/pages/shelves/ShelvesListPage";
