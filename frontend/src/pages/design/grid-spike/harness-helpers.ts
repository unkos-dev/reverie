/**
 * Pure helpers behind the harness benches, split out so the sample-integrity
 * and edit-overlay rules are unit-testable without a live grid (the grids do
 * not emit focus or scroll in jsdom, which is why the bench drivers themselves
 * cannot be exercised there).
 */
import type { SpikeBookRow } from "./types";

/** rowId -> committed title. Title is the only editable column (columns.ts). */
export type RowEdits = ReadonlyMap<string, string>;

/**
 * Overlay committed edits onto the fetched rows so a committed value survives
 * re-render and the measure-mount remount instead of snapping back to the
 * query result. Returns the input untouched when nothing is edited.
 */
export function applyEdits(
  rows: readonly SpikeBookRow[],
  edits: RowEdits,
): readonly SpikeBookRow[] {
  if (edits.size === 0) return rows;
  return rows.map((row) => {
    const title = edits.get(row.id);
    return title === undefined ? row : { ...row, title };
  });
}

/**
 * Record one keystroke-bench move. A resolved focus reports its keydown-to-move
 * delta; a timed-out move records the ceiling so the stall counts against the
 * budget rather than vanishing from the sample. Returns true when the move was
 * dropped.
 */
export function recordMove(
  samples: number[],
  focusDeltaMs: number | null,
  timeoutMs: number,
): boolean {
  if (focusDeltaMs === null) {
    samples.push(timeoutMs);
    return true;
  }
  samples.push(focusDeltaMs);
  return false;
}
