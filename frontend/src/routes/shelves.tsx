/* oxlint-disable react/only-export-components */
/**
 * Route module for `/shelves`.
 */
import { listShelves } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

/** Loader for `/shelves` — prefetches the shelf list. */
export async function loader(): Promise<null> {
  await queryClient
    .query({
      queryKey: queryKeys.shelves.list(),
      queryFn: ({ signal }) => listShelves(signal),
    })
    .catch(() => {});
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export { ShelvesListPage as Component } from "@/pages/shelves/ShelvesListPage";
