import { getLocalTimeZone, parseDate } from "@internationalized/date";
import { z } from "zod";

import { ReadingStatusSchema, type ArrayParamKey, type ListBooksParams } from "@/api";

export type { LibraryView } from "@/api";

export const MAX_FILTER_VALUES = 20;
export const TEXT_OPS = ["contains", "eq", "ne", "empty"] as const;
export type TextOp = (typeof TEXT_OPS)[number];

export function isTextOp(value: string): value is TextOp {
  return TEXT_OPS.some((op) => op === value);
}

const TEXT_COLUMNS = ["title", "subtitle", "isbn_13"] as const;
type TextColumn = (typeof TEXT_COLUMNS)[number];
type WireOp<C extends TextColumn> = {
  [O in TextOp]: `${C}_${O}` extends keyof ListBooksParams ? O : never;
}[TextOp];

export const TEXT_COLUMN_OPS: { readonly [C in TextColumn]: readonly WireOp<C>[] } = {
  title: ["contains", "eq", "ne"],
  subtitle: ["contains", "empty"],
  isbn_13: ["contains", "empty"],
};

const textFilterSchema = (ops: readonly TextOp[]) =>
  z.strictObject({
    op: z
      .enum(TEXT_OPS)
      .refine((op) => ops.includes(op))
      .optional(),
    text: z.string().optional(),
  });
const rangeFilterSchema = (min: number, max: number) =>
  z.strictObject({
    gte: z.number().int().min(min).max(max).optional(),
    lte: z.number().int().min(min).max(max).optional(),
    empty: z.boolean().optional(),
  });
const tokens = z.array(z.string());
const opaqueId = z.string().min(1);
const ids = z.array(opaqueId);
const facetIds = z.union([ids, opaqueId.transform((id) => [id])]).default([]);
const setFilterSchema = (values: z.ZodArray<z.ZodType<string>>) =>
  z.strictObject({ all: values, any: values, none: values });
const statuses = z.array(z.enum([...ReadingStatusSchema.options, "unread"]));

const LibraryPositionSchema = z.strictObject({
  anchorId: opaqueId.nullable(),
  anchorOffset: z.number(),
  offset: z.number().nonnegative(),
  extent: z.number().int().nonnegative(),
  view: z.enum(["grid", "table"]),
});
export type LibraryPosition = z.infer<typeof LibraryPositionSchema>;

export const LibraryVisitSchema = z.strictObject({
  filters: z.strictObject({
    q: z.string().optional(),
    title: textFilterSchema(TEXT_COLUMN_OPS.title),
    subtitle: textFilterSchema(TEXT_COLUMN_OPS.subtitle),
    isbn13: textFilterSchema(TEXT_COLUMN_OPS.isbn_13),
    needsReview: z.literal(true).optional(),
    enrichmentFailed: z.literal(true).optional(),
    noCover: z.literal(true).optional(),
    pages: rangeFilterSchema(0, 2147483647),
    rating: rangeFilterSchema(1, 5),
    addedAfter: z.iso.date().optional(),
    addedBefore: z.iso.date().optional(),
    status: z.strictObject({ any: statuses, none: statuses }),
    authors: setFilterSchema(ids),
    tags: setFilterSchema(tokens),
    genres: setFilterSchema(tokens),
    moods: setFilterSchema(tokens),
    series: facetIds,
    shelf: facetIds,
  }),
  view: z.enum(["grid", "table"]).optional(),
  position: LibraryPositionSchema.optional(),
});

export type LibraryVisit = z.infer<typeof LibraryVisitSchema>;
export type FilterState = LibraryVisit["filters"];
export type TextFilter = FilterState["title"];
export type RangeFilter = FilterState["pages"];
export type SetFilter = FilterState["tags"];
export type StatusFilter = FilterState["status"];

export function emptyFilterState(): FilterState {
  return {
    title: {},
    subtitle: {},
    isbn13: {},
    pages: {},
    rating: {},
    status: { any: [], none: [] },
    authors: { all: [], any: [], none: [] },
    tags: { all: [], any: [], none: [] },
    genres: { all: [], any: [], none: [] },
    moods: { all: [], any: [], none: [] },
    series: [],
    shelf: [],
  };
}

export function effectiveText(filter: TextFilter): string | undefined {
  const text = filter.text?.trim();
  return text === "" ? undefined : text;
}

export function isIsbn13FilterActive(filter: TextFilter): boolean {
  return filter.op === "empty" || (filter.text?.replace(/[ -]/g, "").trim() ?? "") !== "";
}

export function libraryTextError(value: string | undefined): string | undefined {
  return Array.from(value?.trim() ?? "").length > 200
    ? "Use 200 characters or fewer, excluding surrounding spaces."
    : undefined;
}

export function hasOverLimitSelection(params: ListBooksParams): boolean {
  return Object.values(params).some(
    (value) => Array.isArray(value) && value.length > MAX_FILTER_VALUES,
  );
}

export function isLibraryRequestEligible(params: ListBooksParams): boolean {
  if (hasOverLimitSelection(params)) return false;
  return [
    params.q,
    params.title_contains,
    params.title_eq,
    params.title_ne,
    params.subtitle_contains,
    params.isbn_13_contains,
    params.isbn_13_eq,
  ].every((value) => libraryTextError(value) === undefined);
}

function assignText(params: ListBooksParams, prefix: TextColumn, filter: TextFilter): void {
  const op = filter.op ?? "contains";
  if (op === "empty") {
    if (prefix === "subtitle" || prefix === "isbn_13") params[`${prefix}_empty`] = true;
    return;
  }
  const text = effectiveText(filter);
  if (text === undefined) return;
  if (op === "contains") params[`${prefix}_contains`] = text;
  if (prefix === "title" && op === "eq") params.title_eq = text;
  if (prefix === "title" && op === "ne") params.title_ne = text;
}

type FacetParamKey = Exclude<ArrayParamKey, "status_any" | "status_none">;

function assignRange(
  params: ListBooksParams,
  prefix: "pages" | "rating",
  filter: RangeFilter,
): void {
  if (filter.gte !== undefined) params[`${prefix}_gte`] = filter.gte;
  if (filter.lte !== undefined) params[`${prefix}_lte`] = filter.lte;
  if (filter.empty !== undefined) params[`${prefix}_empty`] = filter.empty;
}

function assignSet(
  params: ListBooksParams,
  keys: readonly [FacetParamKey, FacetParamKey, FacetParamKey],
  filter: SetFilter,
): void {
  const [all, any, none] = keys;
  if (filter.all.length > 0) params[all] = filter.all;
  if (filter.any.length > 0) params[any] = filter.any;
  if (filter.none.length > 0) params[none] = filter.none;
}

export function filterStateToParams(
  state: FilterState,
  timeZone = getLocalTimeZone(),
): ListBooksParams {
  const params: ListBooksParams = {};
  const query = state.q?.trim();
  if (query) params.q = query;
  assignText(params, "title", state.title);
  assignText(params, "subtitle", state.subtitle);
  assignText(params, "isbn_13", state.isbn13);
  if (state.needsReview) params.needs_review = true;
  if (state.enrichmentFailed) params.enrichment_failed = true;
  if (state.noCover) params.cover_empty = true;
  assignRange(params, "pages", state.pages);
  assignRange(params, "rating", state.rating);
  if (state.addedAfter !== undefined)
    params.created_at_gte = parseDate(state.addedAfter).toDate(timeZone).toISOString();
  if (state.addedBefore !== undefined)
    params.created_at_lt = parseDate(state.addedBefore)
      .add({ days: 1 })
      .toDate(timeZone)
      .toISOString();
  if (state.status.any.length > 0) params.status_any = state.status.any;
  if (state.status.none.length > 0) params.status_none = state.status.none;
  assignSet(params, ["author", "author_any", "author_none"], state.authors);
  assignSet(params, ["tag", "tag_any", "tag_none"], state.tags);
  assignSet(params, ["genre", "genre_any", "genre_none"], state.genres);
  assignSet(params, ["mood", "mood_any", "mood_none"], state.moods);
  if (state.series.length > 0) params.series_any = state.series;
  if (state.shelf.length > 0) params.shelf_any = state.shelf;
  return params;
}

export function countActiveFilters(state: FilterState): number {
  const params = filterStateToParams(state);
  if (!isIsbn13FilterActive(state.isbn13)) delete params.isbn_13_contains;
  return Object.keys(params).length;
}

export function hasActiveFilterState(state: FilterState): boolean {
  return countActiveFilters(state) > 0;
}
