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

const LIBRARY_VIEWS = ["grid", "list", "table"] as const;
/** The library's browse presentation modes. */
export type LibraryView = (typeof LIBRARY_VIEWS)[number];

/** Type guard narrowing an arbitrary string into the `LibraryView` union. */
export function isLibraryView(value: string): value is LibraryView {
  return LIBRARY_VIEWS.some((view) => view === value);
}

/**
 * Parse `?view=` into a {@link LibraryView}, or `null` when the param is
 * absent or out of range. `null` lets the caller fall back through its own
 * default chain (persisted preference, then `grid`).
 */
export function viewFromSearch(search: URLSearchParams): LibraryView | null {
  const raw = search.get("view");
  return raw !== null && isLibraryView(raw) ? raw : null;
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
