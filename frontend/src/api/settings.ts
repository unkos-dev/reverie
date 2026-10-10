/**
 * Client for `/api/v1/settings`, the instance-wide runtime settings (admin only).
 *
 * Mirrors `SettingsResponse` and `PutSettingsResponse` in
 * `backend/src/routes/settings/mod.rs`. The `PUT` is an RFC 7396 merge patch
 * guarded by `If-Match`: `etags.ts` keys this path, so the tag a `GET` or
 * `PUT` returns is echoed on the next `PUT` by `apiFetch`, and a `412` or
 * `428` surfaces as an {@link ApiError} the caller reconciles.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const MANIFESTATION_FORMATS = ["epub", "pdf", "mobi", "azw3", "cbz", "cbr"] as const;

const int32 = z.number().int().min(-2147483648).max(2147483647);
const int64 = z.number().int().refine(Number.isSafeInteger, "exceeds the exact integer range");

const SettingsValuesSchema = z.object({
  accepted_formats: z.array(z.enum(MANIFESTATION_FORMATS)),
  cleanup_imported: z.boolean(),
  cleanup_duplicates: z.boolean(),
  enrichment_enabled: z.boolean(),
  enrichment_concurrency: int32,
  enrichment_poll_idle_secs: int32,
  enrichment_fetch_budget_secs: int32,
  cover_max_bytes: int64,
  cover_download_timeout_secs: int32,
  cover_min_long_edge_px: int32,
  cover_redirect_limit: int32,
  writeback_enabled: z.boolean(),
  writeback_concurrency: int32,
  writeback_poll_idle_secs: int32,
  writeback_max_attempts: int32,
  opds_enabled: z.boolean(),
  opds_page_size: int32,
});

/** The editable settings, keyed by their API field names. */
export type SettingsValues = z.infer<typeof SettingsValuesSchema>;
export type SettingsKey = keyof SettingsValues;

/** A merge patch: only the fields present are changed. */
export type SettingsPatch = Partial<SettingsValues>;

const GetSettingsSchema = SettingsValuesSchema.extend({
  revision: z.number().int(),
  updated_at: z.iso.datetime(),
  restart_required_fields: z.array(z.string()),
});

const ReloadStatusSchema = z.object({
  last_successful_reload_at: z.iso.datetime().nullable(),
});

const PutSettingsSchema = SettingsValuesSchema.extend({
  revision: z.number().int(),
  updated_at: z.iso.datetime(),
  restart_required: z.boolean(),
});

/** One consistent read of the settings row. */
export type SettingsSnapshot = {
  values: SettingsValues;
  revision: number;
  updatedAt: string;
  restartRequiredFields: readonly string[];
};

/** The settings row after a successful write. */
export type SettingsWriteResult = {
  values: SettingsValues;
  revision: number;
  updatedAt: string;
  restartRequired: boolean;
};

/** `GET /api/v1/settings` — the current settings. */
export async function getSettings(signal?: AbortSignal): Promise<SettingsSnapshot> {
  const raw = await apiFetch("/api/v1/settings", { signal });
  const { revision, updated_at, restart_required_fields, ...values } = GetSettingsSchema.parse(raw);
  return {
    values,
    revision,
    updatedAt: updated_at,
    restartRequiredFields: restart_required_fields,
  };
}

/**
 * `GET /api/v1/settings/reload-status` — when this server process last
 * refreshed its settings cache, or `null` until the first refresh. Carries no
 * `ETag`, so it can be refetched freely without disturbing a draft.
 */
export async function getReloadStatus(signal?: AbortSignal): Promise<string | null> {
  const raw = await apiFetch("/api/v1/settings/reload-status", { signal });
  return ReloadStatusSchema.parse(raw).last_successful_reload_at;
}

/**
 * `PUT /api/v1/settings` — apply a merge patch. `If-Match` rides on the tag
 * the last `GET` or `PUT` returned; callers never set it.
 */
export async function updateSettings(
  patch: SettingsPatch,
  signal?: AbortSignal,
): Promise<SettingsWriteResult> {
  const raw = await apiFetch("/api/v1/settings", {
    method: "PUT",
    body: JSON.stringify(patch),
    signal,
  });
  const { revision, updated_at, restart_required, ...values } = PutSettingsSchema.parse(raw);
  return { values, revision, updatedAt: updated_at, restartRequired: restart_required };
}
