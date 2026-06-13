/**
 * Navigation model for the app shell's left rail.
 *
 * Single source of truth for primary-nav entries (spec §3, UNK-385):
 * live destinations render as links, disabled entries render the
 * product's future shape (spec S3 — visible, non-interactive, tooltip
 * "Planned — not in this release"), and admin-only entries render only
 * for `role === "admin"` (spec S6 — structural absence for non-admins,
 * enforced server-side; rail gating is cosmetic).
 *
 * Settings intentionally lives in the user menu, not here.
 */

/** One primary-nav entry in the left rail. */
export interface NavItem {
  /** Visible label (text-only nav — no icons, spec §12). */
  label: string;
  /** Route path for live entries; absent on disabled placeholders. */
  to?: string;
  /** Spec S3 placeholder: rendered but non-interactive, out of tab order. */
  disabled?: boolean;
}

/** Tooltip copy for disabled placeholder entries (spec S3). */
export const PLANNED_TOOLTIP = "Planned — not in this release";

/** Primary navigation (top of rail, in render order). */
export const PRIMARY_NAV: NavItem[] = [
  { label: "Home", disabled: true },
  { label: "Library", to: "/library" },
  { label: "Shelves", to: "/shelves" },
  { label: "Stats", disabled: true },
];

/** Admin cluster (below divider; rendered only for `role === "admin"`). */
export const ADMIN_NAV: NavItem[] = [
  { label: "Users", to: "/admin/users" },
  { label: "Ingestion", to: "/admin/dashboard" },
];
