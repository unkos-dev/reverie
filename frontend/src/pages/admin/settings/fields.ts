/**
 * The settings screen's field catalogue: which areas exist, which fields each
 * holds, their copy, and how raw input maps to and from the API's values.
 */
import { z } from "zod";

import type { SettingsKey, SettingsValues } from "@/api/settings";

export const AREA_IDS = ["acquisition", "enrichment", "covers", "writeback", "catalogue"] as const;
export type AreaId = (typeof AREA_IDS)[number];

export const AreaIdSchema = z.enum(AREA_IDS);

export type Area = { id: AreaId; title: string; description: string };

export const AREAS: readonly Area[] = [
  {
    id: "acquisition",
    title: "Acquisition",
    description:
      "What Reverie accepts from the ingestion folder, and what it does with the source files afterwards.",
  },
  {
    id: "enrichment",
    title: "Enrichment",
    description: "Looks up metadata for your books from external sources.",
  },
  {
    id: "covers",
    title: "Covers",
    description: "Limits applied when Reverie downloads cover images.",
  },
  {
    id: "writeback",
    title: "Writeback",
    description: "Writes corrected metadata back into the book files.",
  },
  {
    id: "catalogue",
    title: "Catalogue feed",
    description: "The OPDS feed that reading apps use to browse and download your books.",
  },
];

export function areaById(id: AreaId): Area {
  return AREAS.find((area) => area.id === id) ?? AREAS[0];
}

export const BYTES_PER_MIB = 1_048_576;
const INT32_MAX = 2_147_483_647;
const MIB_MAX = Math.floor(Number.MAX_SAFE_INTEGER / BYTES_PER_MIB);

export const FORMATS_KEY = "accepted_formats" satisfies SettingsKey;

export const BOOLEAN_KEYS = [
  "cleanup_imported",
  "cleanup_duplicates",
  "enrichment_enabled",
  "writeback_enabled",
  "opds_enabled",
] as const satisfies readonly SettingsKey[];
export type BooleanKey = (typeof BOOLEAN_KEYS)[number];

export const NUMBER_KEYS = [
  "enrichment_concurrency",
  "enrichment_poll_idle_secs",
  "enrichment_fetch_budget_secs",
  "cover_max_bytes",
  "cover_download_timeout_secs",
  "cover_min_long_edge_px",
  "cover_redirect_limit",
  "writeback_concurrency",
  "writeback_poll_idle_secs",
  "writeback_max_attempts",
  "opds_page_size",
] as const satisfies readonly SettingsKey[];
export type NumberKey = (typeof NUMBER_KEYS)[number];

export function isBooleanKey(key: SettingsKey): key is BooleanKey {
  return BOOLEAN_KEYS.some((k) => k === key);
}

export function isNumberKey(key: SettingsKey): key is NumberKey {
  return NUMBER_KEYS.some((k) => k === key);
}

type FieldBase = { key: SettingsKey; area: AreaId; label: string; help: string };

export type SwitchField = FieldBase & { control: "switch"; key: BooleanKey; destructive: boolean };
export type FormatsField = FieldBase & { control: "formats"; key: typeof FORMATS_KEY };
export type IntField = FieldBase & {
  control: "int";
  key: Exclude<NumberKey, "cover_max_bytes">;
  unit: string;
  min: number;
  max: number | null;
};
export type MibField = FieldBase & { control: "mib"; key: "cover_max_bytes" };
export type Field = SwitchField | FormatsField | IntField | MibField;

const WHOLE_1_OR_MORE = "Enter a whole number of 1 or more.";

export const FIELDS: readonly Field[] = [
  {
    key: "accepted_formats",
    area: "acquisition",
    control: "formats",
    label: "Accepted formats",
    help: "Only these formats are imported. EPUB is the only format Reverie supports at present.",
  },
  {
    key: "cleanup_imported",
    area: "acquisition",
    control: "switch",
    destructive: true,
    label: "Remove the source file after a successful import",
    help: "Deletes the original file from the ingestion folder once the book is in the library. This cannot be undone.",
  },
  {
    key: "cleanup_duplicates",
    area: "acquisition",
    control: "switch",
    destructive: true,
    label: "Remove the source file when the book is a duplicate",
    help: "Deletes the original file from the ingestion folder when the library already holds the same book. This cannot be undone.",
  },
  {
    key: "enrichment_enabled",
    area: "enrichment",
    control: "switch",
    destructive: false,
    label: "Fetch metadata from external sources",
    help: "When off, no new lookups run and books keep the metadata they arrived with.",
  },
  {
    key: "enrichment_concurrency",
    area: "enrichment",
    control: "int",
    unit: "lookups",
    min: 1,
    max: 10,
    label: "Concurrent lookups",
    help: "How many books are looked up at the same time. Higher is faster but sends more requests to external sources. Between 1 and 10.",
  },
  {
    key: "enrichment_poll_idle_secs",
    area: "enrichment",
    control: "int",
    unit: "seconds",
    min: 1,
    max: null,
    label: "Idle check interval",
    help: "How long the enrichment worker waits before checking an empty queue again.",
  },
  {
    key: "enrichment_fetch_budget_secs",
    area: "enrichment",
    control: "int",
    unit: "seconds",
    min: 1,
    max: null,
    label: "Time allowed per source",
    help: "The longest Reverie waits on one external source for one book.",
  },
  {
    key: "cover_max_bytes",
    area: "covers",
    control: "mib",
    label: "Largest cover file",
    help: "Covers larger than this are rejected. Stored as bytes; the exact value is shown below the field.",
  },
  {
    key: "cover_download_timeout_secs",
    area: "covers",
    control: "int",
    unit: "seconds",
    min: 1,
    max: null,
    label: "Download timeout",
    help: "How long Reverie waits for a cover to finish downloading.",
  },
  {
    key: "cover_min_long_edge_px",
    area: "covers",
    control: "int",
    unit: "pixels",
    min: 1,
    max: null,
    label: "Smallest acceptable cover",
    help: "Covers whose longest side is shorter than this are rejected.",
  },
  {
    key: "cover_redirect_limit",
    area: "covers",
    control: "int",
    unit: "redirects",
    min: 0,
    max: null,
    label: "Redirects to follow",
    help: "How many redirects Reverie follows when fetching a cover. 0 follows none.",
  },
  {
    key: "writeback_enabled",
    area: "writeback",
    control: "switch",
    destructive: false,
    label: "Write corrections into book files",
    help: "When on, Reverie rewrites the metadata and cover inside each EPUB to match the library record, and restores the file if the result fails validation.",
  },
  {
    key: "writeback_concurrency",
    area: "writeback",
    control: "int",
    unit: "files",
    min: 1,
    max: 10,
    label: "Concurrent writes",
    help: "How many files are rewritten at the same time. Between 1 and 10.",
  },
  {
    key: "writeback_poll_idle_secs",
    area: "writeback",
    control: "int",
    unit: "seconds",
    min: 1,
    max: null,
    label: "Idle check interval",
    help: "How long the writeback worker waits before checking an empty queue again.",
  },
  {
    key: "writeback_max_attempts",
    area: "writeback",
    control: "int",
    unit: "attempts",
    min: 1,
    max: null,
    label: "Attempts per file",
    help: "How many times Reverie retries a failed write before it gives up on that file.",
  },
  {
    key: "opds_enabled",
    area: "catalogue",
    control: "switch",
    destructive: false,
    label: "Serve the OPDS catalogue",
    help: "When off, reading apps can no longer reach the feed.",
  },
  {
    key: "opds_page_size",
    area: "catalogue",
    control: "int",
    unit: "entries",
    min: 1,
    max: 500,
    label: "Entries per page",
    help: "How many books each page of the feed lists. Between 1 and 500.",
  },
];

export function fieldsForArea(area: AreaId): readonly Field[] {
  return FIELDS.filter((field) => field.area === area);
}

export function fieldByKey(key: SettingsKey): Field {
  return FIELDS.find((field) => field.key === key) ?? FIELDS[0];
}

export function areaOfKey(key: SettingsKey): AreaId {
  return fieldByKey(key).area;
}

const DELETION_COPY = {
  imported: {
    title: "Remove source files after import?",
    body: "Once a book is in the library, Reverie will delete the original file from the ingestion folder. This cannot be undone. The setting takes effect when you save.",
    note: "after a successful import",
  },
  duplicates: {
    title: "Remove source files for duplicates?",
    body: "When the library already holds a book, Reverie will delete the duplicate original file from the ingestion folder. This cannot be undone. The setting takes effect when you save.",
    note: "when the library already holds the same book",
  },
} as const;

export type DeletionCopy = (typeof DELETION_COPY)[keyof typeof DELETION_COPY];

/** Confirmation copy for the two switches that delete source files; `null` for every other field. */
export function deletionCopy(key: SettingsKey): DeletionCopy | null {
  if (key === "cleanup_imported") return DELETION_COPY.imported;
  if (key === "cleanup_duplicates") return DELETION_COPY.duplicates;
  return null;
}

export const ALL_KEYS: readonly SettingsKey[] = FIELDS.map((field) => field.key);

/** A copy of `record` without `drop`, built from the closed key list so no key is deleted dynamically. */
export function omitKey<K extends string, V>(
  record: Partial<Record<K, V>>,
  keys: readonly K[],
  drop: K,
): Partial<Record<K, V>> {
  const next: Partial<Record<K, V>> = {};
  for (const key of keys) {
    const value = record[key];
    if (key !== drop && value !== undefined) next[key] = value;
  }
  return next;
}

function boundsMessage(field: IntField): string {
  if (field.max !== null)
    return `Enter a whole number from ${String(field.min)} to ${String(field.max)}.`;
  return field.min === 0 ? "Enter a whole number of 0 or more." : WHOLE_1_OR_MORE;
}

export type ParseResult<T> = { ok: true; value: T } | { ok: false; message: string };

const INTEGER_TEXT = /^-?\d+$/;
const DECIMAL_TEXT = /^\d*\.?\d+$/;
const NOT_A_NUMBER = "Enter a number.";
const SIZE_MESSAGE = "Enter a size greater than 0.";

const numericText = z
  .string()
  .trim()
  .refine((text) => text !== "" && Number.isFinite(Number(text)), NOT_A_NUMBER);

function toResult<T>(parsed: z.ZodSafeParseResult<T>): ParseResult<T> {
  if (parsed.success) return { ok: true, value: parsed.data };
  return { ok: false, message: parsed.error.issues[0]?.message ?? NOT_A_NUMBER };
}

function integerSchema(field: IntField): z.ZodType<number, string> {
  const upper = field.max ?? INT32_MAX;
  const upperMessage =
    field.max === null
      ? "Enter a whole number no larger than 2,147,483,647."
      : boundsMessage(field);
  return numericText
    .pipe(z.string().regex(INTEGER_TEXT, boundsMessage(field)))
    .pipe(z.string().transform(Number))
    .pipe(z.number().min(field.min, boundsMessage(field)).max(upper, upperMessage));
}

const mibSchema: z.ZodType<number, string> = numericText
  .pipe(z.string().regex(DECIMAL_TEXT, SIZE_MESSAGE))
  .pipe(z.string().transform(Number))
  .pipe(
    z
      .number()
      .gt(0, SIZE_MESSAGE)
      .max(MIB_MAX, `Enter a size no larger than ${MIB_MAX.toLocaleString("en-AU")} MiB.`),
  )
  .transform((mib) => Math.round(mib * BYTES_PER_MIB))
  .pipe(z.number().min(1, SIZE_MESSAGE));

export function parseInteger(field: IntField, text: string): ParseResult<number> {
  return toResult(integerSchema(field).safeParse(text));
}

export function parseMib(text: string): ParseResult<number> {
  return toResult(mibSchema.safeParse(text));
}

export function parseNumberField(field: IntField | MibField, text: string): ParseResult<number> {
  return field.control === "mib" ? parseMib(text) : parseInteger(field, text);
}

export function numberToText(key: NumberKey, value: number): string {
  return key === "cover_max_bytes" ? String(value / BYTES_PER_MIB) : String(value);
}

export function formatBytes(bytes: number): string {
  return `${bytes.toLocaleString("en-AU")} bytes`;
}

export function pluralise(count: number, one: string, many: string): string {
  return count === 1 ? one : many;
}

function formatMib(bytes: number): string {
  return `${String(Number((bytes / BYTES_PER_MIB).toFixed(2)))} MiB`;
}

/** Human value of one setting, as the conflict and notice copy quotes it. */
export function describeValue(key: SettingsKey, values: SettingsValues): string {
  const field = fieldByKey(key);
  if (field.control === "switch") return values[field.key] ? "On" : "Off";
  if (field.control === "formats") {
    return values.accepted_formats.length > 0
      ? values.accepted_formats.map((f) => f.toUpperCase()).join(", ")
      : "None";
  }
  if (field.control === "mib") return formatMib(values.cover_max_bytes);
  return `${String(values[field.key])} ${field.unit}`;
}

/** Same as {@link describeValue}, for a number already parsed from the draft. */
export function describeNumber(key: NumberKey, value: number): string {
  const field = fieldByKey(key);
  if (field.control === "mib") return formatMib(value);
  if (field.control === "int") return `${String(value)} ${field.unit}`;
  return String(value);
}

export function describeBoolean(value: boolean): string {
  return value ? "On" : "Off";
}

function offOn(enabled: boolean, text: string): string {
  return enabled ? `On. ${text}` : "Off";
}

/** One-line live summary shown under each area in the sub-navigation. */
export function areaSummary(area: AreaId, values: SettingsValues): string {
  switch (area) {
    case "acquisition": {
      const formats =
        values.accepted_formats.length > 0
          ? values.accepted_formats.map((f) => f.toUpperCase()).join(", ")
          : "No formats";
      if (!values.cleanup_imported && !values.cleanup_duplicates)
        return `${formats}. Source files kept`;
      if (values.cleanup_imported && !values.cleanup_duplicates) {
        return `${formats}. Source files removed after import`;
      }
      return `${formats}. Source files removed`;
    }
    case "enrichment":
      return offOn(
        values.enrichment_enabled,
        `${String(values.enrichment_concurrency)} ${pluralise(values.enrichment_concurrency, "lookup", "lookups")} at a time`,
      );
    case "covers":
      return `Up to ${formatMib(values.cover_max_bytes)}`;
    case "writeback":
      return offOn(
        values.writeback_enabled,
        `${String(values.writeback_concurrency)} ${pluralise(values.writeback_concurrency, "file", "files")} at a time`,
      );
    case "catalogue":
      return offOn(
        values.opds_enabled,
        `${String(values.opds_page_size)} ${pluralise(values.opds_page_size, "entry", "entries")} per page`,
      );
  }
}

/** "Largest cover file" or "Largest cover file and Download timeout" for restart copy. */
export function restartLabels(keys: readonly SettingsKey[]): string {
  return keys.map((key) => fieldByKey(key).label).join(" and ");
}

export function fieldCountText(n: number): string {
  return `${String(n)} ${pluralise(n, "field needs", "fields need")} attention`;
}
