/**
 * Column model for the grid perf harness, retyped against the promoted
 * `GridColumn<R>` contract (`@/lib/grid/types`) so the harness exercises the
 * exact shape production callers use. `SpikeBookRow` stays a harness-local
 * type (see `./types`): the promoted contract is generic and has no opinion
 * on row shape.
 */
import type { GridColumn } from "@/lib/grid/types";

import type { SpikeBookRow } from "./types";

function readingStateLabel(row: SpikeBookRow): string {
  const rs = row.reading_state;
  if (rs === null || rs.status === null) return "";
  if (rs.rating === null) return rs.status;
  return `${rs.status} (${String(rs.rating)}/5)`;
}

export const SPIKE_COLUMNS: readonly GridColumn<SpikeBookRow>[] = [
  {
    key: "title",
    name: "Title",
    sortable: true,
    width: 280,
    accessor: (row) => row.title,
  },
  {
    key: "subtitle",
    name: "Subtitle",
    sortable: true,
    width: 240,
    accessor: (row) => row.subtitle ?? "",
  },
  {
    key: "authors",
    name: "Authors",
    sortable: true,
    width: 220,
    accessor: (row) => row.authors.join(", "),
  },
  {
    key: "isbn_13",
    name: "ISBN-13",
    sortable: false,
    width: 160,
    accessor: (row) => row.isbn_13 ?? "",
  },
  {
    key: "pages",
    name: "Pages",
    sortable: true,
    width: 90,
    accessor: (row) => (row.pages === null ? "" : String(row.pages)),
  },
  {
    key: "reading_state",
    name: "Reading",
    sortable: false,
    width: 160,
    accessor: readingStateLabel,
  },
];
