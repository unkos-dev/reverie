import { z } from "zod";

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

/** Upper bound on `?id=` repetitions per resolve call; matches the backend
 *  `MAX_RESOLVE_IDS` cap, over which the endpoint returns 422. Chip hydration
 *  resolves the union of the `author`, `author_any`, and `author_none` filter
 *  sets (each capped at 20), so 60 covers the largest set in one request. */
export const MAX_RESOLVE_IDS = 60;
