/**
 * Barrel for the `src/api/` module. Re-exports the public surface so
 * consumers can write `import { listBooks } from "@/api"` without
 * naming the submodule. Internal helpers (request building, CSRF
 * cache mutation) stay private to their own files.
 */
export { ApiError } from "./errors";
export { apiFetch } from "./fetch";
export { getCsrfToken, refreshCsrfToken } from "./csrf";
export {
  listBooks,
  getBook,
  getWork,
  updateBookMetadata,
  UpdateBookMetadataFieldsSchema,
} from "./books";
export type {
  BookListItem,
  BookListResponse,
  BookDetail,
  WorkDetail,
  WorkManifestation,
  SeriesRef,
  MetadataVersionSummary,
  MetadataVersionRow,
  IngestionStatus,
  EnrichmentStatus,
  ListSort,
  ListBooksParams,
  UpdateBookMetadataFields,
} from "./books";
export { searchLibrary } from "./search";
export type { SearchHit, SearchHitKind, SearchResponse } from "./search";
export { acceptVersion, rejectVersion, revertField } from "./metadata";
