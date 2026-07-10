/**
 * localStorage persistence for the desktop filter-rail collapsed state.
 *
 * Purely a client-side UI preference with no server reader, so localStorage
 * rather than a cookie: nothing needs the value to ride along with requests.
 * Storage access can throw (privacy mode, disabled storage, quota) and reads
 * can return values written by something else entirely; both read and write
 * degrade silently, and an absent or unparsable value reads as "no
 * preference" rather than a default collapsed/expanded state.
 */
export const RAIL_STORAGE_KEY = "reverie_library_rail";

/** Read the persisted rail-collapsed preference, or `null` when unset,
 *  unreadable, or malformed. */
export function readRailCollapsed(): boolean | null {
  try {
    const raw = localStorage.getItem(RAIL_STORAGE_KEY);
    if (raw === "true") return true;
    if (raw === "false") return false;
    return null;
  } catch {
    return null;
  }
}

/**
 * Persist the rail-collapsed preference. Failures (quota, disabled storage)
 * are swallowed; nothing may depend on the write having landed.
 */
export function writeRailCollapsed(value: boolean): void {
  try {
    localStorage.setItem(RAIL_STORAGE_KEY, String(value));
  } catch {
    // Storage unavailable or full: the preference simply won't persist.
  }
}
