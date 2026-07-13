/**
 * Cookie persistence for the library view choice (grid / table). A stale
 * cookie holding the retired "list" value fails `isLibraryView` and reads
 * as no preference.
 *
 * The URL `?view=` param stays the canonical, shareable state; this cookie
 * only supplies the default when the param is absent, so the chosen view
 * survives navigation away and back. Mirrors the theme cookie shape in
 * `lib/theme/cookie.ts` (the codebase's one client-persistence precedent);
 * a cookie rather than localStorage keeps the two mechanisms uniform.
 */
import { isLibraryView, type LibraryView } from "@/routes/library-params";

export const VIEW_COOKIE_NAME = "reverie_library_view";

const ONE_YEAR_SECONDS = 60 * 60 * 24 * 365;

/** Read the persisted view choice, or `null` when unset or malformed. */
export function readViewCookie(): LibraryView | null {
  const match = document.cookie
    .split("; ")
    .find((entry) => entry.startsWith(`${VIEW_COOKIE_NAME}=`));
  if (match === undefined) return null;
  const value = match.slice(VIEW_COOKIE_NAME.length + 1);
  return isLibraryView(value) ? value : null;
}

/**
 * Persist the view choice for a year. Browsers drop cookie writes that
 * violate an attribute constraint (Secure on plain HTTP, for one) without
 * throwing, so this write is unobservable by design; the read path treats
 * an absent cookie as "no preference" and nothing depends on the write
 * having landed.
 */
export function writeViewCookie(value: LibraryView): void {
  document.cookie = `${VIEW_COOKIE_NAME}=${value}; Path=/; Max-Age=${String(ONE_YEAR_SECONDS)}; SameSite=Lax; Secure`;
}
