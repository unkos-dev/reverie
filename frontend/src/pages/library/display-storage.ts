/**
 * localStorage persistence for the library table's display preferences:
 * row density and hidden columns.
 *
 * Display preferences are client-side only (no server reader) and are
 * deliberately not URL state: a shared link should carry the query
 * (filters, search, sort), never one reader's table layout. Mirrors
 * `rail-storage.ts`: reads and writes degrade silently, and an absent or
 * malformed value reads as "no preference". Hidden-column keys are
 * persisted as written; the table view intersects them with the columns
 * it actually has, so stale keys from an older schema are inert.
 */
import { z } from "zod";

export const DISPLAY_STORAGE_KEY = "reverie_library_display";

export const DENSITIES = ["comfortable", "compact"] as const;
export type Density = (typeof DENSITIES)[number];

const StoredDisplaySchema = z.object({
  density: z.enum(DENSITIES).nullable().catch(null),
  hiddenColumns: z.array(z.string()).nullable().catch(null),
});

export type DisplayPreferences = z.infer<typeof StoredDisplaySchema>;

const NO_PREFERENCE: DisplayPreferences = { density: null, hiddenColumns: null };

/** Read persisted display preferences; each field is `null` when unset,
 *  unreadable, or malformed. */
export function readDisplayPreferences(): DisplayPreferences {
  try {
    const raw = localStorage.getItem(DISPLAY_STORAGE_KEY);
    if (raw === null) return NO_PREFERENCE;
    const parsed = StoredDisplaySchema.safeParse(JSON.parse(raw));
    return parsed.success ? parsed.data : NO_PREFERENCE;
  } catch {
    return NO_PREFERENCE;
  }
}

/**
 * Persist display preferences, merging over what is already stored so a
 * density write does not clobber a hidden-columns write. Failures are
 * swallowed; nothing may depend on the write having landed.
 */
export function writeDisplayPreferences(partial: Partial<DisplayPreferences>): void {
  try {
    const current = readDisplayPreferences();
    const next: DisplayPreferences = {
      density: partial.density === undefined ? current.density : partial.density,
      hiddenColumns:
        partial.hiddenColumns === undefined ? current.hiddenColumns : partial.hiddenColumns,
    };
    localStorage.setItem(DISPLAY_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage unavailable or full: the preference simply won't persist.
  }
}
