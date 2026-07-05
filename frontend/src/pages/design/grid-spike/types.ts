/**
 * Synthetic row schema for the grid perf harness (dev-only, `/design/grid-spike`).
 *
 * The harness drives the production `ReactDataGridBinding` (see
 * `@/lib/grid`) over a synthetic dataset shaped like the `BookListRow`
 * projection `/api/v1/books` returns, so perf numbers measured here transfer
 * to the real table view. This module is dev-only; it lives under
 * `pages/design/` so the Vite `codeSplitting` group strips it from
 * production builds; see `frontend/vite.config.ts` and
 * `scripts/assert-no-design-chunk.mjs`.
 */
import { z } from "zod";

/**
 * Per-user reading summary, mirroring the backend `ReadingStateSummary`
 * projection. Every field is nullable: a book may carry no reading state.
 */
export const ReadingStateSummarySchema = z.object({
  status: z.string().nullable(),
  rating: z.number().int().nullable(),
  progress_pct: z.number().nullable(),
});
export type ReadingStateSummary = z.infer<typeof ReadingStateSummarySchema>;

/**
 * One synthetic row, shaped like the `BookListRow` projection the production
 * `/api/v1/books` list returns. Deliberately self-contained: the harness
 * never touches the real endpoint, so it does not depend on the frontend
 * `BookListItem` schema.
 */
export const SpikeBookRowSchema = z.object({
  id: z.string(),
  title: z.string(),
  subtitle: z.string().nullable(),
  authors: z.array(z.string()),
  isbn_13: z.string().nullable(),
  pages: z.number().int().nullable(),
  reading_state: ReadingStateSummarySchema.nullable(),
});
export type SpikeBookRow = z.infer<typeof SpikeBookRowSchema>;
