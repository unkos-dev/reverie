/* oxlint-disable react/only-export-components */
/**
 * Route module for `/shelves/:id`.
 */
import type { LoaderFunctionArgs } from "react-router";

import { getShelf } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { ShelfDetailPage } from "@/pages/shelves/ShelfDetailPage";

/** Loader for `/shelves/:id` — prefetches the shelf detail. */
export async function loader({ params }: LoaderFunctionArgs): Promise<null> {
  const id = params.id ?? "";
  await queryClient
    .query({
      queryKey: queryKeys.shelves.detail(id),
      queryFn: ({ signal }) => getShelf(id, signal),
    })
    .catch(() => {});
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = ShelfDetailPage;
