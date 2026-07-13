/**
 * Column catalog shared between the lazily loaded table chunk and the
 * main-bundle toolbar (Columns popover). Keys and display names only: no
 * grid or vendor imports may appear here, or the popover would drag the
 * table chunk into the route's critical path.
 */

/**
 * Below this width the table trims to its mobile column set (selection,
 * details, title with the author stacked under it, added). One constant
 * drives both the column trim and the stacked author line, so the two
 * cannot disagree; the masthead's reduced-height rule in `motion.css`
 * shares the same 899px boundary.
 */
export const TABLE_MOBILE_MEDIA_QUERY = "(max-width: 899px)";

/** Columns that survive the mobile trim (plus the selection and details
 *  columns, which are never trimmed). */
export const MOBILE_COLUMN_KEYS: ReadonlySet<string> = new Set(["title", "added"]);

/** Columns the Columns popover may not hide: the title anchors every row
 *  and the details column is the guaranteed drawer path. */
export const LOCKED_COLUMN_KEYS: ReadonlySet<string> = new Set(["title", "details"]);

/** The user-hideable columns, in table order, for the Columns popover. */
export const HIDEABLE_COLUMNS: readonly { key: string; name: string }[] = [
  { key: "subtitle", name: "Subtitle" },
  { key: "authors", name: "Authors" },
  { key: "series", name: "Series" },
  { key: "isbn_13", name: "ISBN" },
  { key: "pages", name: "Pages" },
  { key: "status", name: "Status" },
  { key: "rating", name: "Rating" },
  { key: "added", name: "Added" },
];
