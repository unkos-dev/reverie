/**
 * URL-search → API-params parser for the `/library` route.
 *
 * Lives outside `routes/library.tsx` so the route module can keep
 * its single-component export shape (the `react/only-export-components`
 * rule requires component-only exports).
 */
import type { ListBooksParams, ListSort } from "@/api";

/** Type guard narrowing an arbitrary string into the `ListSort` union. */
function isListSort(value: string): value is ListSort {
  return value === "recent" || value === "title" || value === "author";
}

/**
 * Parse the URL search params into a {@link ListBooksParams}. Unknown
 * keys are dropped; an out-of-range `sort` value falls through to the
 * backend default (`recent`).
 */
export function paramsFromSearch(search: URLSearchParams): ListBooksParams {
  const sortRaw = search.get("sort");
  const sort = sortRaw !== null && isListSort(sortRaw) ? sortRaw : undefined;
  const params: ListBooksParams = {};
  if (sort !== undefined) params.sort = sort;
  const cursor = search.get("cursor");
  if (cursor !== null) params.cursor = cursor;
  const author = search.get("author");
  if (author !== null) params.author = author;
  const series = search.get("series");
  if (series !== null) params.series = series;
  const shelf = search.get("shelf");
  if (shelf !== null) params.shelf = shelf;
  const q = search.get("q");
  if (q !== null) params.q = q;
  return params;
}
