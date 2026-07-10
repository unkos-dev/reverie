/**
 * `useSearchParams` with a single write-through view of the URL params.
 *
 * React re-renders on a scheduler task, which runs after every already-due
 * timer; two writers landing in the same frame (a debounced filter commit
 * and a header-sort click, say) would each build on the same stale render
 * snapshot and the second would silently drop the first. `applyParams`
 * routes every write through a live ref instead: each mutator receives the
 * result of the previous write, whether or not React has rendered between
 * them. The page owns one instance and hands `applyParams` to every URL
 * writer on the route, so there is exactly one write authority.
 */
import { useEffect, useRef, type RefObject } from "react";
import { useSearchParams } from "react-router";

/**
 * Applies `mutate` to the live params and navigates to the result.
 * `clears: true` marks the write as a clear affordance: it bumps the
 * draft-invalidation generation synchronously, so any pending debounced
 * draft can notice at fire time that it has been invalidated even before
 * the cancelling re-render lands.
 */
export type ApplyParams = (
  mutate: (params: URLSearchParams) => URLSearchParams,
  options?: { clears?: boolean },
) => void;

export function useLiveSearchParams(): {
  searchParams: URLSearchParams;
  applyParams: ApplyParams;
  /** Monotonic count of clearing writes; see `ApplyParams`. */
  clearGen: RefObject<number>;
} {
  const [searchParams, setSearchParams] = useSearchParams();
  const liveParams = useRef(searchParams);
  const clearGen = useRef(0);
  // Renders re-sync the ref to the router's truth, picking up external
  // changes (back/forward navigation) between writes.
  useEffect(() => {
    liveParams.current = searchParams;
  }, [searchParams]);

  function applyParams(
    mutate: (params: URLSearchParams) => URLSearchParams,
    options?: { clears?: boolean },
  ): void {
    if (options?.clears === true) clearGen.current += 1;
    const next = mutate(new URLSearchParams(liveParams.current));
    liveParams.current = next;
    setSearchParams(next, { replace: true });
  }

  return { searchParams, applyParams, clearGen };
}
