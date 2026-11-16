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
 * No value here is ever written to the browser URL. A Library visit's own
 * view override wins over the stored view while the visit holds one. Sort
 * has no URL form at all: it is
 * a one-layer per-user preference, resolved here and sent explicitly on the
 * list request (see `docs/adr/0047-library-sort-is-a-per-user-preference-never-url-state.md`
 * for why the two-layer URL-over-preference model was retired).
 */
import { fetchQuery, useFragment, useMutation, useRelayEnvironment } from "react-relay";
import { createOperationDescriptor, getRequest } from "relay-runtime";
import { z } from "zod";
import { activeUserEpoch } from "@/lib/active-user";
import { readerLibraryPreferences, readerLibraryPreferencesQuery } from "@/lib/relay/reader";
import { mutationsUpdateLibraryPreferencesMutation } from "@/lib/relay/mutations";
import type { readerLibraryPreferencesQuery as PreferencesQuery } from "@/lib/relay/__generated__/readerLibraryPreferencesQuery.graphql";
import type { readerLibraryPreferences$key } from "@/lib/relay/__generated__/readerLibraryPreferences.graphql";
import type {
  mutationsUpdateLibraryPreferencesMutation as PreferencesMutation,
  LibraryPreferencesPatch,
} from "@/lib/relay/__generated__/mutationsUpdateLibraryPreferencesMutation.graphql";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  DENSITIES,
  LIBRARY_VIEWS,
  parseSortParam,
  serializeSortParam,
  type Density,
  type LibraryView,
  type SortLevelParam,
} from "@/api";

import { readDisplayPreferences, writeDisplayPreferences } from "./display-storage";

/** How long a preference change waits for a follow-up before it is sent. */
export const PREFERENCE_WRITE_DEBOUNCE_MS = 400;

/**
 * What to paint before the server or the mirror has answered. Deliberately
 * minimal, and never consulted once a response has arrived.
 */
const BOOTSTRAP: { density: Density; hiddenColumns: readonly string[]; view: LibraryView } = {
  density: "comfortable",
  hiddenColumns: ["subtitle"],
  view: "grid",
};

export type LibraryPreferences = {
  /** Effective row density for the table view. */
  density: Density;
  /** Effective hidden-column keys. Stale keys are inert: the table
   *  intersects them with the columns it actually has. */
  hiddenColumns: ReadonlySet<string>;
  /** Effective view choice, which a Library visit's view override still overrides. */
  view: LibraryView;
  /**
   * The reader's sort override in the `?sort=` wire grammar, or `""` when
   * they inherit the installation order. This is the value the list request
   * carries: an override is sent explicitly, and an inheriting reader sends
   * no `sort` at all. The installation default is never serialized into a
   * request it would not change, so an inheriting reader's query key stays
   * equal to the route loader's URL-derived seed key.
   */
  sortOverride: string;
  /**
   * The effective sort stack, resolved as the override else the response's
   * installation default. This is the one reading every sort surface renders
   * and edits: the table headers, the rail's stack editor, and the live
   * region cannot disagree, because none of them resolves independently.
   * Empty only in the window before the first preferences response on a
   * device with no override mirrored.
   */
  sortLevels: readonly SortLevelParam[];
  /** The installation default sort stack from the response's `defaults`;
   *  empty before the first response. */
  defaultSortLevels: readonly SortLevelParam[];
  /** True when the effective sort is the inherited installation stack, which
   *  is what disables the sort reset control. */
  sortInherited: boolean;
  /**
   * Whether a columns override exists to drop. An override whose value
   * equals the installation default still counts: dropping it changes
   * nothing visible today, but re-subscribes the reader to future changes
   * of that default, which is the reset control's actual contract.
   */
  canResetColumns: boolean;
  setDensity: (next: Density) => void;
  setHiddenColumns: (next: ReadonlySet<string>) => void;
  /** Drop the columns override so the group inherits the default again. */
  resetColumns: () => void;
  setView: (next: LibraryView) => void;
  /**
   * The one intent handler for every sort gesture: header clicks and the
   * rail's stack editor both serialize through here. A non-empty stack is
   * written as the reader's override; an empty stack (removing the last
   * level, or the reset control) writes `null`, and the display visibly
   * transitions to the installation stack it now inherits. There is no
   * unsorted state to request: the library always has a total order.
   */
  setSortLevels: (levels: readonly SortLevelParam[]) => void;
};

export function useLibraryPreferences(): LibraryPreferences {
  const environment = useRelayEnvironment();
  const [epoch] = useState(activeUserEpoch);
  // Read once: this is a first-paint hint, and re-reading it mid-session
  // would let the mirror answer for a group the server has since settled.
  const [mirror] = useState(readDisplayPreferences);

  const [reference, setReference] = useState<readerLibraryPreferences$key | null>(null);
  useEffect(() => {
    const retention = environment.retain(
      createOperationDescriptor(getRequest(readerLibraryPreferencesQuery), {}),
    );
    const subscription = fetchQuery<PreferencesQuery>(
      environment,
      readerLibraryPreferencesQuery,
      {},
      { fetchPolicy: "store-or-network" },
    ).subscribe({
      next: (response) => {
        setReference(response.libraryPreferences);
      },
      error: (error: Error) => {
        console.error("[useLibraryPreferences] preferences read failed", error);
      },
    });
    return () => {
      subscription.unsubscribe();
      retention.dispose();
    };
  }, [environment]);
  const data = useFragment<readerLibraryPreferences$key>(readerLibraryPreferences, reference);
  const [localDensity, setLocalDensity] = useState<Density | null>(null);
  // Tri-state, like the server column: `undefined` means the reader has not
  // acted this session, `null` means they reset to inherit, an array is
  // their override. Two-state `value | null` would collapse "reset" into
  // "untouched" and let the cached response resurrect a dropped override.
  const [localColumns, setLocalColumns] = useState<readonly string[] | null | undefined>(undefined);
  const [localView, setLocalView] = useState<LibraryView | null>(null);
  const [localSort, setLocalSort] = useState<string | null>(null);

  const [mutate] = useMutation<PreferencesMutation>(mutationsUpdateLibraryPreferencesMutation);
  const pending = useRef<LibraryPreferencesPatch>({});
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flush = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const fields = pending.current;
    pending.current = {};
    if (Object.keys(fields).length > 0 && activeUserEpoch() === epoch)
      mutate({
        variables: { patch: fields },
        updater: (store) => {
          const target = store.getRoot().getLinkedRecord("libraryPreferences");
          const updated = store.getRootField("updateLibraryPreferences");
          target.copyFieldsFrom(updated);
        },
        onError: (error) => {
          console.error("[useLibraryPreferences] failed to persist a preference", error);
        },
      });
  }, [mutate, epoch]);

  const queue = useCallback(
    (fields: LibraryPreferencesPatch) => {
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
    localDensity ??
    z
      .enum(DENSITIES)
      .parse(data?.density ?? data?.defaults.density ?? mirror.density ?? BOOTSTRAP.density);
  const view =
    localView ??
    z.enum(LIBRARY_VIEWS).parse(data?.view ?? data?.defaults.view ?? mirror.view ?? BOOTSTRAP.view);
  const defaultColumns = data?.defaults.hiddenColumns ?? BOOTSTRAP.hiddenColumns;
  // Columns resolve like sort, not like density: `null` is a meaningful
  // layer value (reset to inherit), so a bare `??` chain would fall through
  // it and resurrect the override still cached beneath.
  let columnKeys: readonly string[];
  if (localColumns !== undefined) {
    columnKeys = localColumns ?? defaultColumns;
  } else if (data != null) {
    columnKeys = data.hiddenColumns ?? data.defaults.hiddenColumns;
  } else {
    columnKeys = mirror.hiddenColumns ?? BOOTSTRAP.hiddenColumns;
  }
  // Whether a columns override EXISTS, which is what the reset control acts
  // on: an override whose value equals the installation default is still an
  // override, and dropping it is a real change, because it re-subscribes
  // the reader to future changes of that default. Only the pre-response
  // window falls back to comparing values, since the mirror stores effective
  // columns and cannot know override-ness.
  let columnsOverridden: boolean;
  if (localColumns !== undefined) {
    columnsOverridden = localColumns !== null;
  } else if (data != null) {
    columnsOverridden = data.hiddenColumns != null;
  } else {
    columnsOverridden = !sameKeys(columnKeys, defaultColumns);
  }
  // Sort cannot use the chain above: an inherited sort resolves to "no
  // override", not to a value, so a `??` chain would fall through to the
  // mirror and resurrect an override the server has already dropped.
  const storedSort = localSort ?? (data == null ? mirror.sortStack : data.sortStack) ?? "";
  // Normalised through the same codec the request uses, so a mirror written
  // against an older schema can never put an unknown field on the wire.
  const sortOverride = serializeSortParam(parseSortParam(storedSort));
  // The one resolution every sort surface reads: the override when the
  // reader has one, else the installation stack the response carries. The
  // request deliberately does NOT follow this fallback (see sortOverride):
  // display resolves to a value because the library always has an order,
  // while the wire omits what the server would apply anyway.
  const effectiveSort = sortOverride !== "" ? sortOverride : (data?.defaults.sortStack ?? "");
  const sortLevels = useMemo(() => parseSortParam(effectiveSort), [effectiveSort]);
  const defaultSortLevels = useMemo(
    () => parseSortParam(data?.defaults.sortStack ?? ""),
    [data?.defaults.sortStack],
  );

  const hiddenColumns = useMemo(() => new Set(columnKeys), [columnKeys]);

  // Refresh the mirror from whatever is currently effective, whichever medium
  // supplied it, so the next cold load paints this and not the previous value.
  // Sort mirrors the OVERRIDE, not the effective value: the route loader
  // seeds the list query key from this field, and an inheriting reader's key
  // must stay the bare one the loader derives from the URL alone.
  useEffect(() => {
    writeDisplayPreferences({
      density,
      hiddenColumns: [...hiddenColumns],
      view,
      sortStack: sortOverride === "" ? null : sortOverride,
    });
  }, [density, hiddenColumns, view, sortOverride]);

  // The setters are stable so a caller can depend on one from an effect
  // without re-running it every render.
  const setDensity = useCallback(
    (next: Density) => {
      setLocalDensity(next);
      queue({ density: next === "compact" ? "COMPACT" : "COMFORTABLE" });
    },
    [queue],
  );

  const setHiddenColumns = useCallback(
    (next: ReadonlySet<string>) => {
      const keys = [...next];
      setLocalColumns(keys);
      queue({ hiddenColumns: keys });
    },
    [queue],
  );

  const resetColumns = useCallback(() => {
    // `null` is "locally reset", a layer value in its own right: the
    // resolution shows the defaults immediately (never the override still
    // cached beneath) and the reset control greys out at once.
    setLocalColumns(null);
    queue({ hiddenColumns: null });
  }, [queue]);

  const setView = useCallback(
    (next: LibraryView) => {
      setLocalView(next);
      queue({ view: next === "table" ? "TABLE" : "GRID" });
    },
    [queue],
  );

  const setSortLevels = useCallback(
    (levels: readonly SortLevelParam[]) => {
      const serialized = serializeSortParam(levels);
      setLocalSort(serialized);
      queue({ sortStack: serialized === "" ? null : serialized });
    },
    [queue],
  );

  return {
    density,
    hiddenColumns,
    view,
    sortOverride,
    sortLevels,
    defaultSortLevels,
    sortInherited: sortOverride === "",
    canResetColumns: columnsOverridden,
    setDensity,
    setHiddenColumns,
    resetColumns,
    setView,
    setSortLevels,
  };
}

/** Order-insensitive comparison of two column-key lists. */
function sameKeys(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const seen = new Set(a);
  return b.every((key) => seen.has(key));
}
