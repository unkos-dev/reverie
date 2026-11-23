import type { ReadingStatus } from "@/api/books";
import type { ReadingStatus as GraphQLReadingStatus } from "@/lib/relay/__generated__/useCellEditReadingMutation.graphql";

export const READING_STATUS_INPUT = {
  want_to_read: "WANT_TO_READ",
  reading: "READING",
  on_hold: "ON_HOLD",
  finished: "FINISHED",
  abandoned: "ABANDONED",
} as const satisfies Record<ReadingStatus, GraphQLReadingStatus>;

const statusByEnum = new Map<GraphQLReadingStatus, ReadingStatus>(
  Object.entries(READING_STATUS_INPUT).map(([status, value]) => [value, status as ReadingStatus]),
);

export function readingStatus(
  value: GraphQLReadingStatus | null | undefined,
): ReadingStatus | null {
  return value == null ? null : (statusByEnum.get(value) ?? null);
}

export type EditableColumnKey =
  | "title"
  | "subtitle"
  | "isbn_13"
  | "pages"
  | "authors"
  | "status"
  | "rating";
