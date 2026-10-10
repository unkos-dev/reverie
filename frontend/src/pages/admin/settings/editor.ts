/**
 * Pure draft logic for the settings screen: what is dirty, what a save sends,
 * and how a stale write is reconciled against the server's newer values.
 */
import type { SettingsKey, SettingsPatch, SettingsValues } from "@/api/settings";

import {
  ALL_KEYS,
  BOOLEAN_KEYS,
  NUMBER_KEYS,
  omitKey,
  areaOfKey,
  areaById,
  describeBoolean,
  describeNumber,
  describeValue,
  fieldByKey,
  fieldsForArea,
  isBooleanKey,
  isNumberKey,
  numberToText,
  parseNumberField,
  type AreaId,
  type BooleanKey,
  type NumberKey,
} from "./fields";

export type Draft = {
  booleans: Partial<Record<BooleanKey, boolean>>;
  numbers: Partial<Record<NumberKey, string>>;
  epub: boolean | null;
};

export const EMPTY_DRAFT: Draft = { booleans: {}, numbers: {}, epub: null };

export function sameValue(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function baseEpub(base: SettingsValues): boolean {
  return base.accepted_formats.includes("epub");
}

export function shownBoolean(draft: Draft, base: SettingsValues, key: BooleanKey): boolean {
  return draft.booleans[key] ?? base[key];
}

export function shownText(draft: Draft, base: SettingsValues, key: NumberKey): string {
  return draft.numbers[key] ?? numberToText(key, base[key]);
}

export function shownEpub(draft: Draft, base: SettingsValues): boolean {
  return draft.epub ?? baseEpub(base);
}

export function setBoolean(
  draft: Draft,
  base: SettingsValues,
  key: BooleanKey,
  value: boolean,
): Draft {
  if (value === base[key])
    return { ...draft, booleans: omitKey(draft.booleans, BOOLEAN_KEYS, key) };
  return { ...draft, booleans: { ...draft.booleans, [key]: value } };
}

export function setText(draft: Draft, base: SettingsValues, key: NumberKey, text: string): Draft {
  if (text === numberToText(key, base[key]))
    return { ...draft, numbers: omitKey(draft.numbers, NUMBER_KEYS, key) };
  return { ...draft, numbers: { ...draft.numbers, [key]: text } };
}

export function setEpub(draft: Draft, base: SettingsValues, value: boolean): Draft {
  return { ...draft, epub: value === baseEpub(base) ? null : value };
}

export function dropKeys(draft: Draft, keys: readonly SettingsKey[]): Draft {
  let next = draft;
  for (const key of keys) {
    if (isBooleanKey(key)) next = { ...next, booleans: omitKey(next.booleans, BOOLEAN_KEYS, key) };
    else if (isNumberKey(key)) next = { ...next, numbers: omitKey(next.numbers, NUMBER_KEYS, key) };
    else next = { ...next, epub: null };
  }
  return next;
}

function isKeyDirty(draft: Draft, base: SettingsValues, key: SettingsKey): boolean {
  if (isBooleanKey(key))
    return draft.booleans[key] !== undefined && draft.booleans[key] !== base[key];
  if (isNumberKey(key)) {
    const text = draft.numbers[key];
    if (text === undefined) return false;
    const field = fieldByKey(key);
    if (field.control !== "int" && field.control !== "mib") return false;
    const parsed = parseNumberField(field, text);
    return !parsed.ok || parsed.value !== base[key];
  }
  return draft.epub !== null && draft.epub !== baseEpub(base);
}

/** Dirty fields of one area, in display order. */
export function dirtyKeys(area: AreaId, draft: Draft, base: SettingsValues): SettingsKey[] {
  return fieldsForArea(area)
    .map((field) => field.key)
    .filter((key) => isKeyDirty(draft, base, key));
}

export type BuiltPatch =
  | { ok: true; patch: SettingsPatch }
  | { ok: false; errors: Partial<Record<SettingsKey, string>> };

/** Parse the area's dirty fields into a merge patch, or report each field's error. */
export function buildPatch(area: AreaId, draft: Draft, base: SettingsValues): BuiltPatch {
  const patch: SettingsPatch = {};
  const errors: Partial<Record<SettingsKey, string>> = {};
  for (const key of dirtyKeys(area, draft, base)) {
    const field = fieldByKey(key);
    if (field.control === "switch") {
      patch[field.key] = shownBoolean(draft, base, field.key);
    } else if (field.control === "formats") {
      patch.accepted_formats = shownEpub(draft, base) ? ["epub"] : [];
    } else {
      const parsed = parseNumberField(field, shownText(draft, base, field.key));
      if (!parsed.ok) errors[key] = parsed.message;
      else patch[field.key] = parsed.value;
    }
  }
  return Object.keys(errors).length > 0 ? { ok: false, errors } : { ok: true, patch };
}

export function patchKeys(patch: SettingsPatch): SettingsKey[] {
  return ALL_KEYS.filter((key) => patch[key] !== undefined);
}

/** Value of a field as the draft currently holds it, as copy quotes it; unparsable text is quoted raw. */
export function describeDraftValue(key: SettingsKey, draft: Draft, base: SettingsValues): string {
  const field = fieldByKey(key);
  if (field.control === "switch") return describeBoolean(shownBoolean(draft, base, field.key));
  if (field.control === "formats") {
    const formats: SettingsValues["accepted_formats"] = shownEpub(draft, base) ? ["epub"] : [];
    return describeValue(key, { ...base, accepted_formats: formats });
  }
  const text = shownText(draft, base, field.key);
  const parsed = parseNumberField(field, text);
  return parsed.ok ? describeNumber(field.key, parsed.value) : text;
}

export function changedKeys(before: SettingsValues, after: SettingsValues): SettingsKey[] {
  return ALL_KEYS.filter((key) => !sameValue(before[key], after[key]));
}

export type Classification = {
  overlap: SettingsKey[];
  agreed: SettingsKey[];
  changedByOthers: SettingsKey[];
};

/**
 * Compare what the user edited with what changed server-side since the
 * snapshot the edits were based on. `overlap` fields were changed by both
 * sides to different values; `agreed` fields were changed to the user's own
 * value, which is not a conflict.
 */
export function classify(
  base: SettingsValues,
  fresh: SettingsValues,
  patch: SettingsPatch,
): Classification {
  const changedByOthers = changedKeys(base, fresh);
  const overlap: SettingsKey[] = [];
  const agreed: SettingsKey[] = [];
  for (const key of patchKeys(patch)) {
    if (!changedByOthers.includes(key)) continue;
    if (sameValue(patch[key], fresh[key])) agreed.push(key);
    else overlap.push(key);
  }
  return { overlap, agreed, changedByOthers };
}

export function withoutKeys(patch: SettingsPatch, keys: readonly SettingsKey[]): SettingsPatch {
  const next: SettingsPatch = {};
  for (const key of ALL_KEYS) {
    const value = patch[key];
    if (value !== undefined && !keys.includes(key)) Object.assign(next, { [key]: value });
  }
  return next;
}

/** Value of a patched field as copy quotes it. */
export function describePatched(
  key: SettingsKey,
  patch: SettingsPatch,
  values: SettingsValues,
): string {
  const value = patch[key];
  if (typeof value === "boolean") return describeBoolean(value);
  if (typeof value === "number" && isNumberKey(key)) return describeNumber(key, value);
  return describeValue(key, { ...values, ...patch });
}

const ELSEWHERE_LIMIT = 3;

/** "Writeback, Attempts per file, now 3 attempts" for each field on another page, capped. */
export function elsewhereSummary(
  area: AreaId,
  changed: readonly SettingsKey[],
  fresh: SettingsValues,
): string | null {
  const other = changed.filter((key) => areaOfKey(key) !== area);
  if (other.length === 0) return null;
  const items = other
    .slice(0, ELSEWHERE_LIMIT)
    .map(
      (key) =>
        `${areaById(areaOfKey(key)).title}, ${fieldByKey(key).label}, now ${describeValue(key, fresh)}`,
    );
  const more = other.length - ELSEWHERE_LIMIT;
  const tail = more > 0 ? `, and ${String(more)} more` : "";
  return `Also changed by someone else: ${items.join("; ")}${tail}.`;
}

/** Field-level 422: the server leads its detail with the offending field's key. */
export function fieldForServerDetail(area: AreaId, detail: string): SettingsKey | null {
  const match = fieldsForArea(area).find((field) => detail.startsWith(`${field.key} `));
  return match?.key ?? null;
}
