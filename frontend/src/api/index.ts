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
  parseSortParam,
  serializeSortParam,
  MAX_SORT_LEVELS,
  ReadingStatusSchema,
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
  ReadingStatus,
  SortField,
  SortLevelParam,
  ListBooksParams,
  UpdateBookMetadataFields,
} from "./books";
export { searchLibrary } from "./search";
export type { SearchHit, SearchHitKind, SearchResponse } from "./search";
export { suggestVocab, resolveAuthors, MAX_RESOLVE_IDS } from "./suggest";
export type { Suggestion, SuggestKind } from "./suggest";
export { acceptVersion, rejectVersion, revertField } from "./metadata";
export { getSeries } from "./series";
export type { SeriesDetail, SeriesWork, SeriesWorkManifestation } from "./series";
export {
  buildEtag,
  listShelves,
  getShelf,
  createShelf,
  renameShelf,
  deleteShelf,
  addShelfItem,
  removeShelfItem,
  reorderShelfItems,
} from "./shelves";
export type { Shelf, ShelfItem, ShelfWithItems } from "./shelves";
export {
  listUsers,
  updateUserRole,
  updateUserChildStatus,
  updateUser,
  createUser,
  setAccountStatus,
  adminResetPassword,
} from "./users";
export type { User, Role as UserRole, UpdateUserFields, CreateUserInput } from "./users";
export { listTokens, createToken, revokeToken, SCOPE_VALUES } from "./tokens";
export type { Token, Scope as TokenScope, CreateTokenInput, CreateTokenResponse } from "./tokens";
