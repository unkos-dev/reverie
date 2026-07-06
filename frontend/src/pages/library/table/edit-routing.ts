/**
 * Column -> pipeline routing for in-place library table edits.
 *
 * The grid has exactly two write pipelines: canonical metadata
 * (`PATCH /api/v1/books/{id}/metadata`, journaled) and per-user reading
 * state (`PATCH /api/v1/books/{id}/reading`, no journal). Every editable
 * column resolves to exactly one entry here; the entry owns how a
 * commit becomes a patch body, how the server's response is folded back
 * into the row, and whether the field fans out to sibling editions of
 * the same work. There is no bypass: a column with no entry in this
 * registry is not editable.
 */
import type { BookListItem, UpdateBookMetadataFields } from "@/api";
import type { FieldVersionChange } from "@/api/books";
import type { ReadingState, UpdateReadingFields } from "@/api/reading";

/**
 * A column's write route. The `metadata` variant journals through the
 * manual-edit PATCH; the `reading` variant has no journal. Undo strategy
 * selection (revert vs. counter-patch, and which counter-patch) is not
 * declared here: the orchestration layer derives it per edit from the
 * server's response and the row snapshot.
 */
export type EditRoute =
  | {
      pipeline: "metadata";
      field: string;
      workScoped: boolean;
      toPatch: (row: BookListItem, draft: BookListItem) => UpdateBookMetadataFields;
      applyToRow: (row: BookListItem, applied: FieldVersionChange) => BookListItem;
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

function logUnexpectedType(context: string, value: unknown): void {
  console.error(`[edit-routing] unexpected applied-value type for ${context}`, value);
}

function coerceString(value: unknown, fallback: string, context: string): string {
  if (typeof value === "string") return value;
  logUnexpectedType(context, value);
  return fallback;
}

/** `null` is a valid input here (a cleared field), so it never logs. */
function coerceNullableString(value: unknown, context: string): string | null {
  if (value === null) return null;
  if (typeof value === "string") return value;
  logUnexpectedType(context, value);
  return null;
}

/** `null` is a valid input here (a cleared field), so it never logs. */
function coerceNullableInt(value: unknown, context: string): number | null {
  if (value === null) return null;
  if (typeof value === "number") return value;
  logUnexpectedType(context, value);
  return null;
}

/** `null` is a valid input here (a cleared field), so it never logs. */
function coerceStringArray(value: unknown, context: string): string[] {
  if (value === null) return [];
  if (Array.isArray(value))
    return value.filter((entry): entry is string => typeof entry === "string");
  logUnexpectedType(context, value);
  return [];
}

/** Shared `applyToRow` for the reading pipeline: status and rating both
 *  patch the same summary subset from the full `ReadingState` response. */
function applyReadingResponse(row: BookListItem, response: ReadingState): BookListItem {
  return {
    ...row,
    reading_state: {
      status: response.status,
      rating: response.rating,
      progress_pct: response.progress_pct,
    },
  };
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
      title: coerceString(applied.value, row.title, "title"),
    }),
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
      subtitle: coerceNullableString(applied.value, "subtitle"),
    }),
  },
  isbn_13: {
    pipeline: "metadata",
    field: "isbn_13",
    workScoped: false,
    toPatch: (_row, draft) => ({ isbn_13: draft.isbn_13 }),
    applyToRow: (row, applied) => ({
      ...row,
      isbn_13: coerceNullableString(applied.value, "isbn_13"),
    }),
  },
  pages: {
    pipeline: "metadata",
    field: "pages",
    workScoped: false,
    toPatch: (_row, draft) => ({ pages: draft.pages }),
    applyToRow: (row, applied) => ({
      ...row,
      pages: coerceNullableInt(applied.value, "pages"),
    }),
  },
  // Field key matches the response map's `contributors.<role>` shape; the
  // request body nests per `UpdateMetadataFields`. Revert is the default
  // undo strategy for this field; the orchestration layer falls back to a
  // counter-patch when the response carries no previous version pointer
  // for this role.
  authors: {
    pipeline: "metadata",
    field: "contributors.author",
    workScoped: true,
    toPatch: (_row, draft) => ({ contributors: { author: draft.authors } }),
    applyToRow: (row, applied) => ({
      ...row,
      authors: coerceStringArray(applied.value, "contributors.author"),
    }),
  },
  status: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ status: draft.reading_state?.status ?? null }),
    applyToRow: applyReadingResponse,
  },
  rating: {
    pipeline: "reading",
    toPatch: (_row, draft) => ({ rating: draft.reading_state?.rating ?? null }),
    applyToRow: applyReadingResponse,
  },
};
