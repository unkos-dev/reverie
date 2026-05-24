/* eslint-disable react-refresh/only-export-components */
/**
 * Route module for `/shelves`.
 */
import { listShelves } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { ShelvesListPage } from "@/pages/shelves/ShelvesListPage";

/** Loader for `/shelves` — prefetches the shelf list. */
export async function loader(): Promise<null> {
  await queryClient.prefetchQuery({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
  });
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = ShelvesListPage;
