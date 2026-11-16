import { z } from "zod";

const SearchHitKindSchema = z.enum(["book"]);
/** Tag identifying which entity a {@link SearchHit} points at. */
export type SearchHitKind = z.infer<typeof SearchHitKindSchema>;

const SearchHitSchema = z.object({
  kind: SearchHitKindSchema,
  id: z.string(),
  work_id: z.string().nullable(),
  title: z.string(),
  authors: z.array(z.string()),
  snippet: z.string().nullable(),
  cover_url: z.string().nullable(),
});
/** One result row from `GET /api/v1/search`. Discriminate by `kind`. */
export type SearchHit = z.infer<typeof SearchHitSchema>;

const SearchResponseSchema = z.object({
  items: z.array(SearchHitSchema),
});
/** Envelope returned by `GET /api/v1/search`. */
export type SearchResponse = z.infer<typeof SearchResponseSchema>;
