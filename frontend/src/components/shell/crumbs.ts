/**
 * Breadcrumb contract between route definitions and the utility strip.
 *
 * A route opts into the strip's breadcrumb slot by declaring
 * `handle: { crumb }` (react-router `useMatches` exposes the handle +
 * loader data). The crumb function maps the route's loader data to the
 * trailing label; `null` degrades the trail to "Library" alone — the
 * strip must render on every route even when a prefetch failed and the
 * cache was cold (loaders return `null` then).
 */

/** Shape of `handle` on breadcrumb-bearing routes. */
export interface CrumbHandle {
  /** Maps loader data to the trailing crumb label (`null` = no label). */
  crumb: (data: unknown) => string | null;
}

/** Narrow an unknown route `handle` to {@link CrumbHandle}. */
export function isCrumbHandle(handle: unknown): handle is CrumbHandle {
  return (
    typeof handle === "object" &&
    handle !== null &&
    "crumb" in handle &&
    typeof handle.crumb === "function"
  );
}

/**
 * Crumb for routes whose loader returns `{ title } | null` (book and
 * series details). Tolerates any malformed shape by returning `null`.
 */
export function titleCrumb(data: unknown): string | null {
  if (typeof data !== "object" || data === null) return null;
  if (!("title" in data)) return null;
  return typeof data.title === "string" ? data.title : null;
}
