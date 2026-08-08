/**
 * The library's per-user display preferences: hidden columns, row density,
 * view choice, and default sort.
 *
 * The server owns them (`/auth/me/preferences`), so they follow the reader
 * between devices. Three media answer for a group, in this order:
 *
 * 1. **What the reader just did.** A group acted on this session is pinned to
 *    that value. This is what keeps a response already in flight from
 *    overwriting a preference changed while it travelled: a response is a
 *    seed, and the seed is dropped per group once the reader has spoken.
 * 2. **The server response**, resolved as override-else-installation-default.
 *    The defaults arrive with every response rather than living here, so
 *    raising an installation default reaches every reader who has not
 *    overridden that group.
 * 3. **The first-paint mirror** in `./display-storage`, read once at mount, so
 *    a cold load paints the last known values instead of reflowing when the
 *    response lands.
 *
 * {@link BOOTSTRAP} covers only the frame before either of the last two has
 * anything to say (a first visit on a new device). It is not a second copy of
 * the installation defaults: the moment a response arrives, its `defaults`
 * win.
 *
 * Writes are debounced and merged into one patch, so unchecking four columns
 * sends one request rather than four. Last write wins server-side, with no
 * precondition header, which is why a burst needs no ordering protocol: the
 * merged patch is the reader's latest intent. Two tabs can drift until one
 * reloads; nothing is corrupted, and a reload converges.
 *
 * No value here is ever written to the browser URL. The URL carries the
 * *current* filters, sort, and view; these are the reader's defaults, and a
 * second writer on that surface is the bug class the typed per-key params
 * exist to prevent.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  getPreferences,
  parseSortParam,
  serializeSortParam,
  updatePreferences,
  type Density,
  type LibraryView,
  type UpdatePreferencesFields,
} from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { readDisplayPreferences, writeDisplayPreferences } from "./display-storage";

/** How long a preference change waits for a follow-up before it is sent. */
export const PREFERENCE_WRITE_DEBOUNCE_MS = 400;

/**
 * What to paint before the server or the mirror has answered. Deliberately
 * minimal, and never consulted once a response has arrived.
 */
const BOOTSTRAP: { density: Density; hiddenColumns: readonly string[]; view: LibraryView } = {
  density: "comfortable",
  hiddenColumns: [],
  view: "grid",
};

export type LibraryPreferences = {
  /** Effective row density for the table view. */
  density: Density;
  /** Effective hidden-column keys. Stale keys are inert: the table
   *  intersects them with the columns it actually has. */
  hiddenColumns: ReadonlySet<string>;
  /** Effective view choice, which a `?view=` param still overrides. */
  view: LibraryView;
  /**
   * The reader's own default sort in the `?sort=` wire grammar, or `""` when
   * they have none. Empty means "send no `sort`", not "sort by nothing": an
   * unsorted request already gets the installation default order from the
   * list endpoint, so sending it would only fragment the query cache away
   * from the route loader's prefetch.
   */
  defaultSort: string;
  /** Whether hiding nothing would actually change the table, which is what
   *  the columns reset control offers to do. */
  canResetColumns: boolean;
  setDensity: (next: Density) => void;
  setHiddenColumns: (next: ReadonlySet<string>) => void;
  /** Drop the columns override so the group inherits the default again. */
  resetColumns: () => void;
  setView: (next: LibraryView) => void;
  /** Persist the reader's default sort; `""` drops the override. */
  setDefaultSort: (next: string) => void;
};

export function useLibraryPreferences(): LibraryPreferences {
  const queryClient = useQueryClient();
  // Read once: this is a first-paint hint, and re-reading it mid-session
  // would let the mirror answer for a group the server has since settled.
  const [mirror] = useState(readDisplayPreferences);

  const { data } = useQuery({
    queryKey: queryKeys.auth.preferences(),
    queryFn: ({ signal }) => getPreferences(signal),
  });

  const [localDensity, setLocalDensity] = useState<Density | null>(null);
  const [localColumns, setLocalColumns] = useState<readonly string[] | null>(null);
  const [localView, setLocalView] = useState<LibraryView | null>(null);
  const [localSort, setLocalSort] = useState<string | null>(null);

  const { mutate } = useMutation({
    mutationFn: (fields: UpdatePreferencesFields) => updatePreferences(fields),
    onSuccess: (next) => {
      queryClient.setQueryData(queryKeys.auth.preferences(), next);
    },
    onError: (error: unknown) => {
      // The local value stays applied for the session: a reader who chose
      // compact rows keeps them even when the write failed. Only the
      // breadcrumb is owed, since a retry cannot know the intent is current.
      console.error("[useLibraryPreferences] failed to persist a preference", error);
    },
  });

  const pending = useRef<UpdatePreferencesFields>({});
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flush = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const fields = pending.current;
    pending.current = {};
    if (Object.keys(fields).length > 0) mutate(fields);
  }, [mutate]);

  const queue = useCallback(
    (fields: UpdatePreferencesFields) => {
      // Merged, not queued: a later write to the same group replaces the
      // earlier one, and an explicit `null` survives the merge as the reset
      // it is, where a truthiness check would collapse it into "absent".
      pending.current = { ...pending.current, ...fields };
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = setTimeout(flush, PREFERENCE_WRITE_DEBOUNCE_MS);
    },
    [flush],
  );

  // Flush rather than cancel on teardown, so leaving the page inside the
  // debounce window does not silently drop the change. `flush` is stable, so
  // in practice this runs only on unmount; an early run would merely send the
  // pending patch sooner, which is always safe under last-write-wins.
  useEffect(() => {
    return () => {
      flush();
    };
  }, [flush]);

  const density =
    localDensity ?? data?.density ?? data?.defaults.density ?? mirror.density ?? BOOTSTRAP.density;
  const view = localView ?? data?.view ?? data?.defaults.view ?? mirror.view ?? BOOTSTRAP.view;
  const columnKeys =
    localColumns ??
    data?.hidden_columns ??
    data?.defaults.hidden_columns ??
    mirror.hiddenColumns ??
    BOOTSTRAP.hiddenColumns;
  // Sort cannot use the chain above: an inherited sort resolves to "no
  // override", not to a value, so a `??` chain would fall through to the
  // mirror and resurrect an override the server has already dropped.
  const storedSort = localSort ?? (data === undefined ? mirror.sortStack : data.sort_stack) ?? "";
  // Normalised through the same codec the URL uses, so a mirror written
  // against an older schema can never put an unknown field on the wire.
  const defaultSort = serializeSortParam(parseSortParam(storedSort));

  const defaultColumns = data?.defaults.hidden_columns ?? BOOTSTRAP.hiddenColumns;
  const hiddenColumns = useMemo(() => new Set(columnKeys), [columnKeys]);

  // Refresh the mirror from whatever is currently effective, whichever medium
  // supplied it, so the next cold load paints this and not the previous value.
  useEffect(() => {
    writeDisplayPreferences({
      density,
      hiddenColumns: [...hiddenColumns],
      view,
      sortStack: defaultSort === "" ? null : defaultSort,
    });
  }, [density, hiddenColumns, view, defaultSort]);

  // The setters are stable so a caller can depend on one from an effect
  // without re-running it every render.
  const setDensity = useCallback(
    (next: Density) => {
      setLocalDensity(next);
      queue({ density: next });
    },
    [queue],
  );

  const setHiddenColumns = useCallback(
    (next: ReadonlySet<string>) => {
      const keys = [...next];
      setLocalColumns(keys);
      queue({ hidden_columns: keys });
    },
    [queue],
  );

  const resetColumns = useCallback(() => {
    // Pin the local value to the default rather than clearing it: clearing
    // would fall back to the override still cached from the last response, so
    // the hidden columns would reappear until the reset landed.
    setLocalColumns(defaultColumns);
    queue({ hidden_columns: null });
  }, [defaultColumns, queue]);

  const setView = useCallback(
    (next: LibraryView) => {
      setLocalView(next);
      queue({ view: next });
    },
    [queue],
  );

  const setDefaultSort = useCallback(
    (next: string) => {
      setLocalSort(next);
      queue({ sort_stack: next === "" ? null : next });
    },
    [queue],
  );

  return {
    density,
    hiddenColumns,
    view,
    defaultSort,
    canResetColumns: !sameKeys(columnKeys, defaultColumns),
    setDensity,
    setHiddenColumns,
    resetColumns,
    setView,
    setDefaultSort,
  };
}

/** Order-insensitive comparison of two column-key lists. */
function sameKeys(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const seen = new Set(a);
  return b.every((key) => seen.has(key));
}
