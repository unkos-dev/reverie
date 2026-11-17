/**
 * Presentation types for the caller's per-user library display
 * preferences: hidden columns, row density, view choice, and default sort.
 *
 * Mirrors `PreferencesResponse` in `backend/src/routes/preferences/mod.rs`.
 * Every group is an override that is `null` when the caller has not chosen,
 * and the installation defaults travel in the same response, so the client
 * holds no second copy of them to drift. Resolving an override against its
 * default is the reader's job, not this module's.
 *
 * This module also owns the density and view vocabularies, because both are
 * Postgres enums on the other side of the wire. Declaring them here keeps
 * one list per concept: `routes/library-params` re-exports the view union
 * for the `?view=` codec, and the table view imports the density union.
 */
import { z } from "zod";

/** Row heights the library table offers. */
export const DENSITIES = ["comfortable", "compact"] as const;
export type Density = (typeof DENSITIES)[number];

/** The library's browse presentation modes. */
export const LIBRARY_VIEWS = ["grid", "table"] as const;
export type LibraryView = (typeof LIBRARY_VIEWS)[number];

/** Type guard narrowing an arbitrary string into the `LibraryView` union. */
export function isLibraryView(value: string): value is LibraryView {
  return LIBRARY_VIEWS.some((view) => view === value);
}

const PreferenceDefaultsSchema = z.object({
  hidden_columns: z.array(z.string()),
  density: z.enum(DENSITIES),
  view: z.enum(LIBRARY_VIEWS),
  sort_stack: z.string(),
});

/** The installation defaults in force for every group the caller has not set. */
export type PreferenceDefaults = z.infer<typeof PreferenceDefaultsSchema>;

const PreferencesSchema = z.object({
  hidden_columns: z.array(z.string()).nullable(),
  density: z.enum(DENSITIES).nullable(),
  view: z.enum(LIBRARY_VIEWS).nullable(),
  sort_stack: z.string().nullable(),
  defaults: PreferenceDefaultsSchema,
});

/**
 * The caller's overrides beside the defaults they shadow. A `null` group
 * means "inherit"; it is deliberately distinct from a group set to a value
 * that happens to equal the default, because that distinction is what per-
 * group reset acts on and what lets a changed installation default reach
 * everyone who has not opted out of it.
 */
export type Preferences = z.infer<typeof PreferencesSchema>;

/**
 * RFC 7396 JSON Merge Patch body for `PATCH /auth/me/preferences`. An
 * omitted key leaves that group unchanged; a key present with `null` resets
 * it to the installation default. `JSON.stringify` drops `undefined` values,
 * so an optional field left unset serialises as absent rather than as null.
 */
export type UpdatePreferencesFields = {
  hidden_columns?: string[] | null;
  density?: Density | null;
  view?: LibraryView | null;
  sort_stack?: string | null;
};
