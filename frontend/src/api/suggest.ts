/**
 * Client for the vocabulary typeahead + author batch-get surface.
 *
 * Two endpoint families share one row shape ([`Suggestion`] in
 * `backend/src/routes/suggest.rs`), so both parse through
 * {@link SuggestionSchema}:
 *
 * - `GET /api/v1/{kind}/suggest?q=` returns ranked typeahead rows in a
 *   `{ suggestions }` envelope. Feeds {@link TypeaheadMultiSelect} while
 *   the user types.
 * - `GET /api/v1/authors?id=…&id=…` resolves author ids to display
 *   labels in an `{ items }` envelope. Restores author chips from a
 *   bookmarked `?author_any=<uuid>` URL, where the id is all the client
 *   has and the list payload carries author names only.
 *
 * Snake_case is unused here (the wire fields are `id` / `value`), and
 * every body is parsed through Zod at the boundary per
 * `frontend/CLAUDE.md` so a schema-violating response surfaces as a
 * `ZodError` rather than silently corrupting a chip label.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

/** The vocabularies the typeahead can query. Publishers are intentionally
 *  excluded: publisher is neither a filterable column nor a chip this tranche. */
const SUGGEST_KINDS = ["genres", "moods", "tags", "authors", "series"] as const;

/** A vocabulary the suggest endpoint serves. */
export type SuggestKind = (typeof SUGGEST_KINDS)[number];

const SuggestionSchema = z.object({
  // Null only for publisher rows, which this client never requests; kept
  // nullable so the shared schema still matches the wire contract exactly.
  id: z.uuid().nullable(),
  value: z.string(),
});
/** One typeahead row: a vocabulary id and the label to show and resubmit. */
export type Suggestion = z.infer<typeof SuggestionSchema>;

const SuggestResponseSchema = z.object({
  suggestions: z.array(SuggestionSchema),
});

const AuthorsResponseSchema = z.object({
  items: z.array(SuggestionSchema),
});

/** Upper bound on `?id=` repetitions per resolve call; matches the backend
 *  `MAX_RESOLVE_IDS` cap, over which the endpoint returns 422. */
export const MAX_RESOLVE_IDS = 20;

/**
 * Fetch typeahead suggestions for one vocabulary. The backend trims `q`,
 * caps it at 100 characters, and ranks the rows; callers gate on a
 * minimum length before firing (the endpoint rejects an empty `q` with
 * 422). Returns at most `limit` rows (clamped server-side to 1..=20).
 *
 * @param signal - Optional `AbortSignal` to cancel a stale keystroke.
 */
export async function suggestVocab(
  kind: SuggestKind,
  q: string,
  signal?: AbortSignal,
): Promise<Suggestion[]> {
  const url = new URL(`/api/v1/${kind}/suggest`, window.location.origin);
  url.searchParams.set("q", q);
  const body = await apiFetch(url, signal ? { method: "GET", signal } : { method: "GET" });
  return SuggestResponseSchema.parse(body).suggestions;
}

/**
 * Resolve author ids to display labels for chip hydration. The `id`
 * filter is required by the backend (an empty call is a 422), so an
 * empty input short-circuits to `[]` without a request. Ids are
 * de-duplicated and capped at {@link MAX_RESOLVE_IDS}; unknown or
 * out-of-scope ids are silently omitted by the server, so a caller
 * treats a missing id as "unknown author", not an error.
 *
 * @param signal - Optional `AbortSignal` to cancel an in-flight lookup.
 */
export async function resolveAuthors(
  ids: readonly string[],
  signal?: AbortSignal,
): Promise<Suggestion[]> {
  const unique = [...new Set(ids)].slice(0, MAX_RESOLVE_IDS);
  if (unique.length === 0) return [];
  const url = new URL("/api/v1/authors", window.location.origin);
  for (const id of unique) url.searchParams.append("id", id);
  const body = await apiFetch(url, signal ? { method: "GET", signal } : { method: "GET" });
  return AuthorsResponseSchema.parse(body).items;
}
