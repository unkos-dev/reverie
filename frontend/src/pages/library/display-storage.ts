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
 * Entries are scoped per account (`lib/active-user` supplies the id), so a
 * shared browser holds one mirror per family member and none can seed
 * another's first paint or first request. Sign-out forgets the id but keeps
 * the mirrors, so a returning account gets its own warm paint back.
 *
 * A first visit on a new device, one with cleared storage, or the first
 * library visit after a sign-in that has not yet confirmed an identity,
 * paints the installation defaults before the preferences arrive. That is
 * intended: blocking first paint on the network would be worse.
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
import { activeUserId } from "@/lib/active-user";

/**
 * Key prefix for the per-account entries (`<prefix>:<user id>`). The bare
 * prefix is also the pre-scoping device-global key: nothing writes that
 * form any more, and a value left by an earlier build is deleted on sight
 * rather than migrated, because a device-global mirror cannot say whose
 * preferences it holds.
 */
const DISPLAY_STORAGE_PREFIX = "reverie_library_display";

/** The mirror entry for one account. Exported for tests and for nothing
 *  else: production callers never name a key directly. */
export function displayStorageKey(userId: string): string {
  return `${DISPLAY_STORAGE_PREFIX}:${userId}`;
}

/**
 * The active account's mirror key, from the last confirmed identity on
 * this browser, or `null` when none is confirmed. A `null` key reads as
 * "no hint" and swallows writes: an unscoped mirror is exactly the
 * cross-account seed this keying exists to prevent.
 */
function activeKey(): string | null {
  const userId = activeUserId();
  return userId === null ? null : displayStorageKey(userId);
}

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

/** Read the active account's mirror; each field is `null` when unset,
 *  unreadable, malformed, or when no account is confirmed on this browser. */
export function readDisplayPreferences(): DisplayPreferences {
  try {
    // The legacy device-global entry is deleted, never read: it cannot say
    // whose preferences it holds, so it must not seed anyone's paint.
    localStorage.removeItem(DISPLAY_STORAGE_PREFIX);
    const key = activeKey();
    if (key === null) return NO_HINT;
    const raw = localStorage.getItem(key);
    if (raw === null) return NO_HINT;
    const parsed = StoredDisplaySchema.safeParse(JSON.parse(raw));
    return parsed.success ? parsed.data : NO_HINT;
  } catch {
    return NO_HINT;
  }
}

/**
 * Mirror preferences under the active account's key, merging over what is
 * already stored so a density write does not clobber a hidden-columns
 * write. With no confirmed account the write is dropped: better a cold
 * paint next visit than a mirror that cannot say whose it is. Failures are
 * swallowed; nothing may depend on the write having landed.
 */
export function writeDisplayPreferences(partial: Partial<DisplayPreferences>): void {
  try {
    const key = activeKey();
    if (key === null) return;
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
    localStorage.setItem(key, JSON.stringify(next));
  } catch {
    // Storage unavailable or full: the hint simply won't persist.
  }
}
