/**
 * Reactive `window.matchMedia` subscription.
 *
 * Built on `useSyncExternalStore` so the match state stays correct under
 * concurrent rendering (no tearing between the subscription and the read).
 */
import { useCallback, useSyncExternalStore } from "react";

/** Live boolean result of `query`, updating as the media query's match state
 *  changes. Returns `false` when `matchMedia` isn't available. */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback(
    (onStoreChange: () => void): (() => void) => {
      if (typeof matchMedia !== "function") return () => {};
      const mql = matchMedia(query);
      mql.addEventListener("change", onStoreChange);
      return () => {
        mql.removeEventListener("change", onStoreChange);
      };
    },
    [query],
  );
  const getSnapshot = useCallback((): boolean => {
    if (typeof matchMedia !== "function") return false;
    return matchMedia(query).matches;
  }, [query]);
  return useSyncExternalStore(subscribe, getSnapshot);
}
