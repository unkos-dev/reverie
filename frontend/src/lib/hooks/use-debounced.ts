/**
 * Shared debounce hook.
 *
 * Extracted from the command palette so the palette and the library
 * filter surfaces (quick search, typeahead) share one implementation:
 * the same trailing-edge behaviour and the same cleanup-on-change
 * timer, rather than two copies drifting apart.
 */
import { useEffect, useState } from "react";

/** Debounced echo of `value` that updates `delay` ms after the last change. */
export function useDebounced<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const handle = window.setTimeout(() => {
      setDebounced(value);
    }, delay);
    return () => {
      window.clearTimeout(handle);
    };
  }, [value, delay]);
  return debounced;
}
