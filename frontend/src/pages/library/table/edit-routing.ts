/**
 * Column -> pipeline routing for in-place library table edits.
 *
 * The grid has exactly two write pipelines: canonical metadata
 * (`PATCH /api/v1/books/{id}/metadata`, journaled) and per-user reading
 * state (`PATCH /api/v1/books/{id}/reading`, no journal). Every editable
 * column resolves to exactly one entry here; the entry owns how a
 * commit becomes a patch body and whether a field belongs to the work. There is no bypass: a column with no entry in this
 * registry is not editable.
 */
import type { BookListItem, UpdateBookMetadataFields } from "@/api";
import type { UpdateReadingFields } from "@/api/reading";

export type EditRoute =
  | {
      pipeline: "metadata";
      field: string;
      workScoped: boolean;
      toPatch: (row: BookListItem, draft: BookListItem) => UpdateBookMetadataFields;
    }
  | {
      pipeline: "reading";
      toPatch: (row: BookListItem, draft: BookListItem) => UpdateReadingFields;
    };

/** Column keys the table currently wires to an edit route. */
export type EditableColumnKey =
  | "title"
  | "subtitle"
  | "isbn_13"
  | "pages"
  | "authors"
  | "status"
  | "rating";

/**
 * Registry keyed by grid column key. `series` has no entry because no
 * metadata field backs it, so the table never opens an editor for that
 * cell.
 */
export const EDIT_ROUTES: Readonly<Record<EditableColumnKey, EditRoute>> = {
  title: {
    pipeline: "metadata",
    field: "title",
    workScoped: true,
    toPatch: (_row, draft) => ({ title: draft.title }),
  },
  // The subtitle pointer lives on `works`, not the manifestation, despite
  // reading like a per-edition field, so it fans out to sibling editions
  // like title does.
  subtitle: {
    pipeline: "metadata",
    field: "subtitle",
    workScoped: true,
    toPatch: (_row, draft) => ({ subtitle: draft.subtitle }),
  },
  isbn_13: {
    pipeline: "metadata",
    field: "isbn_13",
    workScoped: false,
    toPatch: (_row, draft) => ({ isbn_13: draft.isbn_13 }),
  },
  pages: {
    pipeline: "metadata",
    field: "pages",
    workScoped: false,
    toPatch: (_row, draft) => ({ pages: draft.pages }),
  },
  authors: {
    pipeline: "metadata",
    field: "contributors.author",
    workScoped: true,
    toPatch: (_row, draft) => ({ contributors: { author: draft.authors } }),
  },
  status: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ status: draft.reading_state?.status ?? null }),
  },
  rating: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ rating: draft.reading_state?.rating ?? null }),
  },
};
