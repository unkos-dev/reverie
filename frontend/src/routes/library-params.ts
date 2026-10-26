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
  isbn_13: ["contains", "eq", "empty"],
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
const ids = z.array(z.uuid());
const setFilterSchema = (values: typeof tokens | typeof ids) =>
  z.strictObject({ all: values, any: values, none: values });
const statuses = z.array(z.enum([...ReadingStatusSchema.options, "unread"]));

export const LibraryVisitSchema = z.strictObject({
  filters: z.strictObject({
    q: z.string().optional(),
    title: textFilterSchema(TEXT_COLUMN_OPS.title),
    subtitle: textFilterSchema(TEXT_COLUMN_OPS.subtitle),
    isbn13: textFilterSchema(TEXT_COLUMN_OPS.isbn_13),
    pages: rangeFilterSchema(0, 2147483647),
    rating: rangeFilterSchema(1, 5),
    addedAfter: z.iso.date().optional(),
    addedBefore: z.iso.date().optional(),
    status: z.strictObject({ any: statuses, none: statuses }),
    authors: setFilterSchema(ids),
    tags: setFilterSchema(tokens),
    genres: setFilterSchema(tokens),
    moods: setFilterSchema(tokens),
    series: z.uuid().optional(),
    shelf: z.uuid().optional(),
  }),
  view: z.enum(["grid", "table"]).optional(),
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
  };
}

export function effectiveText(filter: TextFilter): string | undefined {
  const text = filter.text?.trim();
  return text === "" ? undefined : text;
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
  if (prefix === "isbn_13" && op === "eq") params.isbn_13_eq = text;
}

function assignSet(
  params: ListBooksParams,
  keys: readonly [ArrayParamKey, ArrayParamKey, ArrayParamKey],
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
  if (state.pages.gte !== undefined) params.pages_gte = state.pages.gte;
  if (state.pages.lte !== undefined) params.pages_lte = state.pages.lte;
  if (state.pages.empty !== undefined) params.pages_empty = state.pages.empty;
  if (state.rating.gte !== undefined) params.rating_gte = state.rating.gte;
  if (state.rating.lte !== undefined) params.rating_lte = state.rating.lte;
  if (state.rating.empty !== undefined) params.rating_empty = state.rating.empty;
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
  if (state.series !== undefined) params.series = state.series;
  if (state.shelf !== undefined) params.shelf = state.shelf;
  return params;
}

export function hasActiveFilterState(state: FilterState): boolean {
  return Object.keys(filterStateToParams(state)).length > 0;
}
