/**
 * Route module for `/library`. Exports `loader` (prefetches the first
 * page of the library list into the shared `QueryClient`) and
 * `Component` (the page renderer that consumes the prefetched data
 * via `useSuspenseInfiniteQuery`).
 *
 * react-router data-mode requires the loader and the component to
 * live in the same module so a single dynamic `import()` resolves
 * both. The mixed export shape conflicts with
 * `react/only-export-components`; the rule is disabled at the file scope
 * because data routes have no fast-refresh-friendly alternative.
 */
/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { listBooks, type ListBooksParams } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { LibraryPage } from "@/pages/library/LibraryPage";

import { paramsFromSearch } from "./library-params";

/**
 * Loader for `/library`. Prefetches the first page so the component's
 * `useInfiniteQuery` hits a hot cache. Pagination after the first
 * page is driven by the component (Load more button) — the loader
 * only seeds page 1.
 */
export async function loader({ request }: LoaderFunctionArgs): Promise<null> {
  const url = new URL(request.url);
  const params = paramsFromSearch(url.searchParams);
  // Drop `cursor` from the seed page — the loader always primes page 1.
  const seedParams: ListBooksParams = { ...params };
  delete seedParams.cursor;
  const initialPageParam: string | undefined = undefined;
  await queryClient.prefetchInfiniteQuery({
    queryKey: queryKeys.books.list(seedParams),
    // Loader only seeds page 1 — `pageParam` is always the initial
    // value here. Subsequent pages are driven by `fetchNextPage` in
    // the component, which carries its own queryFn closure.
    queryFn: ({ signal }) => listBooks(seedParams, signal),
    initialPageParam,
  });
  return null;
}

export { LibraryPage as Component };
