/**
 * Shared filter-edit coordination for the library's two filter-editing
 * surfaces: the toolbar quick search and the filter rail's sections.
 *
 * Both write through the page-owned `applyParams` authority, and both
 * participate in one draft-survival protocol: `lastEdit` records the
 * tokens of the state the latest own-edit wrote, so a draft-holding
 * input can tell "our own edit elsewhere" (keep the draft) from
 * everything else, navigation, clears, and the page's clear-all (reset
 * the draft). The ref must therefore have exactly one instance per page,
 * owned by the container and handed to every editing surface; a second
 * instance would split the protocol and drafts would die on sibling
 * commits.
 */
import type { RefObject } from "react";

import type { ApplyParams } from "@/lib/hooks/use-live-search-params";
import {
  parseFilterParams,
  serializeFilterParams,
  type FilterState,
} from "@/routes/library-params";

/** Minimum quick-search length; below it the input clears the `q` filter. */
export const MIN_Q_LEN = 2;
/** Debounce before a quick-search or typed-section edit becomes a URL change. */
export const FILTER_DEBOUNCE_MS = 250;

/** Tokens of the committed state the latest own-edit wrote; `null` after a
 *  clear affordance or before any edit. */
export type EditTokens = { full: string; reset: string };

/** A stable, order-independent serialization of every filter *except* `q`.
 *  Quick search watches this: it stays constant while the user types (and
 *  across the debounced `q` write), and changes only on an external filter
 *  edit, which is the signal to discard an uncommitted quick-search draft. */
export function filterResetToken(filters: FilterState): string {
  const params = new URLSearchParams();
  serializeFilterParams(filters, params);
  params.delete("q");
  params.sort();
  return params.toString();
}

/** Order-independent serialization of the whole committed filter state; the
 *  draft-holding sections resync whenever it changes. */
export function fullFilterToken(filters: FilterState): string {
  const params = new URLSearchParams();
  serializeFilterParams(filters, params);
  params.sort();
  return params.toString();
}

/**
 * Builds the user-edit commit path: applies a patch on top of the live
 * committed state, records the edit tokens for the draft-survival
 * protocol, and drops `cursor` because a changed filter invalidates the
 * keyset position.
 */
export function makeFilterCommit(
  applyParams: ApplyParams,
  lastEdit: RefObject<EditTokens | null>,
): (patch: (current: FilterState) => FilterState) => void {
  return (patch) => {
    applyParams((params) => {
      const next = patch(parseFilterParams(params));
      lastEdit.current = { full: fullFilterToken(next), reset: filterResetToken(next) };
      serializeFilterParams(next, params);
      params.delete("cursor");
      return params;
    });
  };
}

/**
 * Builds the clear-affordance write path: pending drafts everywhere must
 * die with a clear, or a queued debounce could resurrect a condition the
 * user just removed. `clears: true` bumps the shared generation the
 * drafts check at fire time.
 */
export function makeFilterClear(
  applyParams: ApplyParams,
  lastEdit: RefObject<EditTokens | null>,
): (patch: (current: FilterState) => FilterState) => void {
  return (patch) => {
    lastEdit.current = null;
    applyParams(
      (params) => {
        const next = patch(parseFilterParams(params));
        serializeFilterParams(next, params);
        params.delete("cursor");
        return params;
      },
      { clears: true },
    );
  };
}
