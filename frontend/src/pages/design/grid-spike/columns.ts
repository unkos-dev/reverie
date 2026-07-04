/**
 * Shared column model for the bake-off. Both bindings consume this identical
 * list, so display projection (authors join, reading-state flattening) lives
 * in one place and the two grids render byte-identical cell text.
 */
import type { GridColumn, SpikeBookRow } from "./types";

function readingStateLabel(row: SpikeBookRow): string {
  const rs = row.reading_state;
  if (rs === null || rs.status === null) return "";
  if (rs.rating === null) return rs.status;
  return `${rs.status} (${String(rs.rating)}/5)`;
}

export const SPIKE_COLUMNS: readonly GridColumn[] = [
  {
    key: "title",
    name: "Title",
    sortable: true,
    editable: true,
    width: 280,
    accessor: (row) => row.title,
  },
  {
    key: "subtitle",
    name: "Subtitle",
    sortable: true,
    editable: false,
    width: 240,
    accessor: (row) => row.subtitle ?? "",
  },
  {
    key: "authors",
    name: "Authors",
    sortable: true,
    editable: false,
    width: 220,
    accessor: (row) => row.authors.join(", "),
  },
  {
    key: "isbn_13",
    name: "ISBN-13",
    sortable: false,
    editable: false,
    width: 160,
    accessor: (row) => row.isbn_13 ?? "",
  },
  {
    key: "pages",
    name: "Pages",
    sortable: true,
    editable: false,
    width: 90,
    accessor: (row) => (row.pages === null ? "" : String(row.pages)),
  },
  {
    key: "reading_state",
    name: "Reading",
    sortable: false,
    editable: false,
    width: 160,
    accessor: readingStateLabel,
  },
];
