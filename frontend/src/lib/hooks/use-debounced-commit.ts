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
 *
 * `flushOnUnmount` decides a pending draft's fate when the host
 * unmounts: default is cancel (a draft must not write during unmount,
 * e.g. a navigation tearing down the toolbar search); `true` commits it
 * synchronously in the unmount cleanup unless `isStale` vetoes, so a
 * surface that unmounts on close (the filter drawer) applies what the
 * user typed instead of racing the close animation.
 */
import { useEffect, useEffectEvent, useRef } from "react";

export function useDebouncedCommit<T>(
  draft: T,
  committed: T,
  commit: (next: T) => void,
  delayMs: number,
  isStale?: () => boolean,
  flushOnUnmount = false,
): void {
  const fire = useEffectEvent((next: T) => {
    if (isStale?.() === true) return;
    commit(next);
  });
  // Latest pending draft, readable by the unmount-flush cleanup. `null`
  // when nothing is pending (fired, or draft equals committed).
  const pending = useRef<{ value: T } | null>(null);
  useEffect(() => {
    if (Object.is(draft, committed)) {
      pending.current = null;
      return;
    }
    pending.current = { value: draft };
    const handle = window.setTimeout(() => {
      pending.current = null;
      fire(draft);
    }, delayMs);
    return () => {
      window.clearTimeout(handle);
    };
  }, [draft, committed, delayMs]);
  useEffect(() => {
    if (!flushOnUnmount) return;
    return () => {
      const last = pending.current;
      if (last === null) return;
      pending.current = null;
      fire(last.value);
    };
  }, [flushOnUnmount]);
}
