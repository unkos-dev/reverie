/**
 * Column -> pipeline routing for in-place library table edits.
 *
 * The grid has exactly two write pipelines: canonical metadata
 * (`PATCH /api/v1/books/{id}/metadata`, journaled) and per-user reading
 * state (`PATCH /api/v1/books/{id}/reading`, no journal). Every editable
 * column resolves to exactly one entry here; the entry owns how a
 * commit becomes a patch body, how the server's response is folded back
 * into the row, whether the field fans out to sibling editions of the
 * same work, and (for metadata fields) how an edit undoes. There is no
 * bypass: a column with no entry in this registry is not editable.
 */
import type { BookListItem, UpdateBookMetadataFields } from "@/api";
import type { FieldVersionChange } from "@/api/books";
import type { ReadingState, UpdateReadingFields } from "@/api/reading";

/**
 * A column's write route. The `metadata` variant journals through the
 * manual-edit PATCH and undoes by reverting to the prior version (or,
 * when no prior version pointer exists, by a counter-patch the
 * orchestration layer builds itself). The `reading` variant has no
 * journal, so it carries no `undo` strategy here: undo is always a
 * counter-patch built from the row snapshot captured before the edit.
 */
export type EditRoute =
  | {
      pipeline: "metadata";
      field: string;
      workScoped: boolean;
      toPatch: (row: BookListItem, draft: BookListItem) => UpdateBookMetadataFields;
      applyToRow: (row: BookListItem, applied: FieldVersionChange) => BookListItem;
      undo: "revert" | "counter-patch";
    }
  | {
      pipeline: "reading";
      toPatch: (row: BookListItem, draft: BookListItem) => UpdateReadingFields;
      applyToRow: (row: BookListItem, response: ReadingState) => BookListItem;
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

function coerceString(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

function coerceNullableString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function coerceNullableInt(value: unknown): number | null {
  return typeof value === "number" ? value : null;
}

function coerceStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === "string");
}

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
    applyToRow: (row, applied) => ({
      ...row,
      title: coerceString(applied.value, row.title),
    }),
    undo: "revert",
  },
  // The subtitle pointer lives on `works`, not the manifestation, despite
  // reading like a per-edition field, so it fans out to sibling editions
  // like title does.
  subtitle: {
    pipeline: "metadata",
    field: "subtitle",
    workScoped: true,
    toPatch: (_row, draft) => ({ subtitle: draft.subtitle }),
    applyToRow: (row, applied) => ({
      ...row,
      subtitle: coerceNullableString(applied.value),
    }),
    undo: "revert",
  },
  isbn_13: {
    pipeline: "metadata",
    field: "isbn_13",
    workScoped: false,
    toPatch: (_row, draft) => ({ isbn_13: draft.isbn_13 }),
    applyToRow: (row, applied) => ({
      ...row,
      isbn_13: coerceNullableString(applied.value),
    }),
    undo: "revert",
  },
  pages: {
    pipeline: "metadata",
    field: "pages",
    workScoped: false,
    toPatch: (_row, draft) => ({ pages: draft.pages }),
    applyToRow: (row, applied) => ({
      ...row,
      pages: coerceNullableInt(applied.value),
    }),
    undo: "revert",
  },
  // Field key matches the response map's `contributors.<role>` shape; the
  // request body nests per `UpdateMetadataFields`. Revert is the declared
  // strategy here; the orchestration layer falls back to a counter-patch
  // when the response carries no previous version pointer for this role.
  authors: {
    pipeline: "metadata",
    field: "contributors.author",
    workScoped: true,
    toPatch: (_row, draft) => ({ contributors: { author: draft.authors } }),
    applyToRow: (row, applied) => ({
      ...row,
      authors: coerceStringArray(applied.value),
    }),
    undo: "revert",
  },
  status: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ status: draft.reading_state?.status ?? null }),
    applyToRow: (row, response) => ({
      ...row,
      reading_state: {
        status: response.status,
        rating: response.rating,
        progress_pct: response.progress_pct,
      },
    }),
  },
  rating: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ rating: draft.reading_state?.rating ?? null }),
    applyToRow: (row, response) => ({
      ...row,
      reading_state: {
        status: response.status,
        rating: response.rating,
        progress_pct: response.progress_pct,
      },
    }),
  },
};
