import { z } from "zod";

const ReadingStatusSchema = z.enum(["want_to_read", "reading", "on_hold", "finished", "abandoned"]);
/** Per-user reading lifecycle state. Matches `backend/src/models/reading_status.rs`. */
export type ReadingStatus = z.infer<typeof ReadingStatusSchema>;

export { ReadingStatusSchema };

export const ResourceEtagSchema = z.string().regex(/^"[A-Za-z0-9_-]{22}"$/);

/** The sortable wire fields, in the order sort pickers offer them. */
export const SORT_FIELDS = [
  "title",
  "subtitle",
  "author",
  "series",
  "isbn_13",
  "pages",
  "status",
  "rating",
  "created_at",
] as const;

/** Sortable column names accepted by `GET /api/v1/books?sort=…`. */
export type SortField = (typeof SORT_FIELDS)[number];

/** One level of a sort stack: a wire field name plus its direction. */
export type SortLevelParam = { field: SortField; desc: boolean };

/** Server-side cap on stack length; `SortSpec::parse` enforces the same limit. */
export const MAX_SORT_LEVELS = 3;

function isSortField(value: string): value is SortField {
  return (SORT_FIELDS as readonly string[]).includes(value);
}

/**
 * Parse a `?sort=` value into an ordered stack. Tolerant of stale or
 * hand-crafted input: unknown fields and repeated columns are dropped
 * silently, the stack is capped at {@link MAX_SORT_LEVELS}, and an
 * all-invalid or empty input degrades to `[]` (the server's default order).
 */
export function parseSortParam(raw: string): SortLevelParam[] {
  const levels: SortLevelParam[] = [];
  const seen = new Set<SortField>();
  for (const token of raw.split(",")) {
    const desc = token.startsWith("-");
    const field = desc ? token.slice(1) : token;
    if (!isSortField(field) || seen.has(field)) continue;
    seen.add(field);
    levels.push({ field, desc });
    if (levels.length === MAX_SORT_LEVELS) break;
  }
  return levels;
}

/**
 * Serialize a sort stack into the JSON:API `?sort=` wire string (`-` prefix
 * for descending). Inverse of {@link parseSortParam} for a stack that
 * already passed through it.
 */
export function serializeSortParam(levels: readonly SortLevelParam[]): string {
  return levels.map((level) => (level.desc ? `-${level.field}` : level.field)).join(",");
}

/**
 * Query parameters for `GET /api/v1/books`. Every field is optional; the
 * flat suffix grammar mirrors the backend `ListParams` one-to-one (the
 * operator is a suffix on the key, e.g. `pages_gte`, `title_contains`). Array
 * fields serialize as a repeated key (`?author=a&author=b`). The Library visit
 * model projects its grouped filters into this flat wire shape.
 */
export type ListBooksParams = {
  cursor?: string;
  /** Pre-validated JSON:API sort string; build via {@link serializeSortParam}. */
  sort?: string;
  q?: string;
  // Author id filters (all-of / any-of / none-of), role-scoped to `author`.
  author?: string[];
  author_any?: string[];
  author_none?: string[];
  series?: string;
  shelf?: string;
  series_any?: string[];
  shelf_any?: string[];
  // Vocabulary set filters (all-of / any-of / none-of).
  tag?: string[];
  tag_any?: string[];
  tag_none?: string[];
  genre?: string[];
  genre_any?: string[];
  genre_none?: string[];
  mood?: string[];
  mood_any?: string[];
  mood_none?: string[];
  // Reading-status set filters; values are wire status names or `unread`.
  status_any?: (ReadingStatus | "unread")[];
  status_none?: (ReadingStatus | "unread")[];
  // Text column filters.
  title_contains?: string;
  title_eq?: string;
  title_ne?: string;
  subtitle_contains?: string;
  subtitle_empty?: boolean;
  isbn_13_contains?: string;
  isbn_13_eq?: string;
  isbn_13_empty?: boolean;
  needs_review?: boolean;
  enrichment_failed?: boolean;
  cover_empty?: boolean;
  // Numeric column filters.
  pages_gte?: number;
  pages_lte?: number;
  pages_empty?: boolean;
  rating_gte?: number;
  rating_lte?: number;
  rating_empty?: boolean;
  // Added timestamps: inclusive lower and exclusive upper RFC 3339 instants.
  created_at_gte?: string;
  created_at_lt?: string;
};

/**
 * The {@link ListBooksParams} keys whose value is a string array (the
 * vocab/author and status set filters). Derived from the wire type so the
 * consumers that enumerate array-valued params (query-key normalization, the
 * set-filter projection) share one definition that tracks the wire shape.
 */
export type ArrayParamKey = {
  [K in keyof ListBooksParams]-?: NonNullable<ListBooksParams[K]> extends readonly string[]
    ? K
    : never;
}[keyof ListBooksParams];

/**
 * Per-role contributor replace/clear, nested under `contributors`.
 * Absent role = untouched; `null` or `[]` clears the role; a non-empty
 * array replaces it wholesale in the given order. Strict: the backend
 * rejects unknown role keys with 422.
 */
const ContributorsPatchSchema = z.strictObject({
  author: z.array(z.string()).nullable().optional(),
  editor: z.array(z.string()).nullable().optional(),
  translator: z.array(z.string()).nullable().optional(),
});

const PageCountSchema = z.number().int().positive();

/** A page count typed as text: digits only, then the page count rule the metadata endpoint accepts. */
export const PageCountInputSchema = z
  .string()
  .regex(/^\d+$/)
  .transform(Number)
  .pipe(PageCountSchema);

const UpdateBookMetadataFieldsSchema = z.object({
  title: z.string().nullable().optional(),
  subtitle: z.string().nullable().optional(),
  description: z.string().nullable().optional(),
  language: z.string().nullable().optional(),
  publisher: z.string().nullable().optional(),
  pub_date: z.string().nullable().optional(),
  isbn_10: z.string().nullable().optional(),
  isbn_13: z.string().nullable().optional(),
  pages: PageCountSchema.nullable().optional(),
  contributors: ContributorsPatchSchema.nullable().optional(),
});
/** Form values used to build the GraphQL metadata patch. Omission leaves a field untouched and null clears it. */
export type UpdateBookMetadataFields = z.infer<typeof UpdateBookMetadataFieldsSchema>;

export { UpdateBookMetadataFieldsSchema };
