/**
 * Debounced commit of a locally drafted value.
 *
 * Complements `useDebounced` (which echoes a value) for the write path:
 * the caller keeps a fast local draft and this hook calls `commit` once
 * the draft has been stable for `delayMs`. The commit callback is read
 * through an effect event, so its identity is irrelevant: an unrelated
 * parent re-render handing down a fresh closure neither resets nor
 * extends the pending window. Only a draft change restarts the timer,
 * and a draft already equal to the committed value never fires.
 *
 * `isStale` runs at fire time, before the commit. The effect cleanup
 * cancels the timer only once the invalidating re-render commits, and a
 * router-transition render can lose that race to an already-due timer;
 * the check closes the gap by consulting live (ref-backed) state that an
 * invalidating gesture updates synchronously.
 */
import { useEffect, useEffectEvent } from "react";

export function useDebouncedCommit<T>(
  draft: T,
  committed: T,
  commit: (next: T) => void,
  delayMs: number,
  isStale?: () => boolean,
): void {
  const fire = useEffectEvent((next: T) => {
    if (isStale?.() === true) return;
    commit(next);
  });
  useEffect(() => {
    if (Object.is(draft, committed)) return;
    const handle = window.setTimeout(() => {
      fire(draft);
    }, delayMs);
    return () => {
      window.clearTimeout(handle);
    };
  }, [draft, committed, delayMs]);
}
