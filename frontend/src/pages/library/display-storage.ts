/**
 * First-paint mirror for the library's display preferences: row density,
 * hidden columns, view choice, and default sort.
 *
 * The server owns these preferences (`/auth/me/preferences`); this is a hint,
 * never authoritative. Preferences arrive asynchronously, so without a local
 * copy a cold load would paint installation defaults and then visibly reflow
 * when the response lands. What is mirrored is therefore the *effective*
 * value of each group, which is what a first paint needs; whether that value
 * is the caller's override or an inherited default is a question only the
 * server response answers, and only per-group reset asks.
 *
 * A first visit on a new device, or one with cleared storage, paints the
 * installation defaults before the preferences arrive. That is intended:
 * blocking first paint on the network would be worse.
 *
 * Reads and writes degrade silently, as `rail-storage.ts` does, and an absent
 * or malformed value reads as "no hint" per field, so a payload written
 * before this module covered view and sort still supplies the two groups it
 * does carry. Hidden-column keys are mirrored as written; the table view
 * intersects them with the columns it actually has, so stale keys from an
 * older schema are inert.
 */
import { z } from "zod";

import { DENSITIES, LIBRARY_VIEWS } from "@/api";

export const DISPLAY_STORAGE_KEY = "reverie_library_display";

const StoredDisplaySchema = z.object({
  density: z.enum(DENSITIES).nullable().catch(null),
  hiddenColumns: z.array(z.string()).nullable().catch(null),
  view: z.enum(LIBRARY_VIEWS).nullable().catch(null),
  sortStack: z.string().nullable().catch(null),
});

export type DisplayPreferences = z.infer<typeof StoredDisplaySchema>;

const NO_HINT: DisplayPreferences = {
  density: null,
  hiddenColumns: null,
  view: null,
  sortStack: null,
};

/** Read the mirrored preferences; each field is `null` when unset,
 *  unreadable, or malformed. */
export function readDisplayPreferences(): DisplayPreferences {
  try {
    const raw = localStorage.getItem(DISPLAY_STORAGE_KEY);
    if (raw === null) return NO_HINT;
    const parsed = StoredDisplaySchema.safeParse(JSON.parse(raw));
    return parsed.success ? parsed.data : NO_HINT;
  } catch {
    return NO_HINT;
  }
}

/**
 * Mirror preferences locally, merging over what is already stored so a
 * density write does not clobber a hidden-columns write. Failures are
 * swallowed; nothing may depend on the write having landed.
 */
export function writeDisplayPreferences(partial: Partial<DisplayPreferences>): void {
  try {
    const current = readDisplayPreferences();
    // Field by field rather than a spread: an absent key must leave the
    // stored group alone, where a spread of an explicit `undefined` would
    // erase it.
    const next: DisplayPreferences = {
      density: partial.density === undefined ? current.density : partial.density,
      hiddenColumns:
        partial.hiddenColumns === undefined ? current.hiddenColumns : partial.hiddenColumns,
      view: partial.view === undefined ? current.view : partial.view,
      sortStack: partial.sortStack === undefined ? current.sortStack : partial.sortStack,
    };
    localStorage.setItem(DISPLAY_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage unavailable or full: the hint simply won't persist.
  }
}
