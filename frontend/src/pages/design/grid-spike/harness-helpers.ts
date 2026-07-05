/**
 * Pure helper behind the keystroke bench, split out so the sample-integrity
 * rule is unit-testable without a live grid (the grid does not emit focus
 * events in jsdom, which is why the bench driver itself cannot be exercised
 * there).
 */

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
