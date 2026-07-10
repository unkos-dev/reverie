/**
 * Human-readable projection of the sort stack, shared by the rail's sort
 * editor (field labels) and the page-level live region. The live region
 * lives in `LibraryPage`, not the rail: the rail unmounts when collapsed
 * or when the sheet is closed, and a live region inside an unmounted or
 * `display:none` subtree never announces, silencing exactly the
 * header-driven sorts the region exists to voice.
 */
import type { SortField, SortLevelParam } from "@/api";

/** Sort-field labels, matching the table view's column header names. */
export const SORT_FIELD_LABELS: Record<SortField, string> = {
  title: "Title",
  author: "Authors",
  created_at: "Added",
  pages: "Pages",
};

/** One sentence naming every level of the stack in priority order. */
export function sortStackSummary(levels: readonly SortLevelParam[]): string {
  if (levels.length === 0) return "Not sorted";
  const parts = levels.map(
    (level) => `${SORT_FIELD_LABELS[level.field]} ${level.desc ? "descending" : "ascending"}`,
  );
  return `Sorted by ${parts.join(", then ")}`;
}
