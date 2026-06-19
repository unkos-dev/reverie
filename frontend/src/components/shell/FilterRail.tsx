/**
 * Contextual filter rail for browse surfaces (spec §2, S11).
 *
 * This epic ships structural + API-backed facets only:
 *
 * - **Series** — live single-select checkboxes writing `?series=<uuid>`
 *   (the list payload's `SeriesRef` carries the id the backend
 *   deserializes). Re-selecting the active value, or Clear, removes
 *   the param. The checkbox previews multi-select, which is blocked on
 *   backend OR-filtering.
 * - **Author** — disabled checkbox rows carrying real names. The backend
 *   expects `?author=<uuid>` but the list payload exposes author *names*
 *   only — sending a name 400s. The tick-box is shown by request but
 *   stays disabled + non-tabbable until the backend lands author UUIDs,
 *   counts, multi-select, and the remaining facets.
 *
 * Facet options derive from the loaded pages — the owning page passes
 * them in; the rail holds no fetch of its own.
 */
import { useId, type ReactElement } from "react";
import { useSearchParams } from "react-router";

import { openCommandPalette, searchHintLabel } from "@/lib/command-palette";

import { PLANNED_TOOLTIP } from "./nav-items";

/** One selectable series in the facet (id is the URL/param value). */
export interface SeriesFacetOption {
  id: string;
  name: string;
}

interface FilterRailProps {
  /** Distinct series from the loaded pages. */
  seriesOptions: SeriesFacetOption[];
  /** Distinct author display names (placeholder rows only). */
  authorNames: string[];
}

/** Filter rail: Series facet (live) + Author placeholder group. */
export function FilterRail({ seriesOptions, authorNames }: FilterRailProps): ReactElement {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeSeries = searchParams.get("series");
  // Instance-scoped — BrowseLayout mounts the rail twice (desktop
  // column + open Refine sheet); a static id would duplicate.
  const plannedDescriptionId = useId();

  function setSeries(next: string | null): void {
    const updated = new URLSearchParams(searchParams);
    if (next === null) updated.delete("series");
    else updated.set("series", next);
    // A filter change invalidates keyset pagination position.
    updated.delete("cursor");
    setSearchParams(updated, { replace: true });
  }

  return (
    <aside aria-label="Filters" className="flex flex-col gap-4 text-sm">
      {/* Search lives in the rail now (the top utility strip was removed). */}
      <button
        type="button"
        onClick={() => {
          openCommandPalette();
        }}
        className="border-border bg-surface text-fg-muted hover:border-border-strong hover:text-fg focus-visible:ring-accent flex h-9 w-full items-center justify-between gap-3 rounded-md border px-3 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2"
      >
        <span className="truncate">Find a volume…</span>
        <kbd className="text-fg-muted font-mono text-[0.65rem] tracking-wide">
          {searchHintLabel()}
        </kbd>
      </button>
      <details open>
        <summary className="text-fg-muted cursor-pointer select-none font-mono text-xs uppercase tracking-[0.14em]">
          Series
        </summary>
        <div className="mt-2 flex flex-col gap-0.5">
          {seriesOptions.map((series) => (
            <label
              key={series.id}
              className="text-fg-muted hover:bg-surface hover:text-fg has-checked:bg-surface has-checked:text-fg flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 transition-colors"
            >
              <input
                type="checkbox"
                value={series.id}
                checked={activeSeries === series.id}
                onChange={() => {
                  // Single-select for now (the API takes one ?series=
                  // value). The checkbox previews multi-select, which is
                  // blocked on backend OR-filtering — see the docstring.
                  setSeries(activeSeries === series.id ? null : series.id);
                }}
                className="accent-accent focus-visible:ring-accent size-3.5 focus-visible:outline-none focus-visible:ring-2"
              />
              <span className="truncate">{series.name}</span>
            </label>
          ))}
          {seriesOptions.length === 0 ? (
            <p className="text-fg-faint px-2 py-1.5">No series in view.</p>
          ) : null}
        </div>
        {activeSeries !== null ? (
          <button
            type="button"
            onClick={() => {
              setSeries(null);
            }}
            className="text-fg-muted hover:text-fg focus-visible:ring-accent mt-2 rounded-sm px-2 font-mono text-xs uppercase tracking-wide transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            Clear
          </button>
        ) : null}
      </details>

      <details open>
        <summary className="text-fg-muted cursor-pointer select-none font-mono text-xs uppercase tracking-[0.14em]">
          Author
        </summary>
        <div className="mt-2 flex flex-col gap-0.5">
          {authorNames.map((name) => (
            <span
              key={name}
              aria-disabled="true"
              aria-describedby={plannedDescriptionId}
              className="text-fg-faint group relative flex cursor-default items-center gap-2 rounded-md px-2 py-1.5"
            >
              {/* Disabled, non-tabbable, SR-hidden — the tick-box affordance
                  is shown by request while the author facet stays inert
                  (multi-select + UUID filtering are backend-gated). */}
              <input
                type="checkbox"
                disabled
                tabIndex={-1}
                aria-hidden="true"
                className="accent-accent size-3.5"
              />
              <span className="truncate">{name}</span>
              <span
                role="tooltip"
                aria-hidden="true"
                className="border-border bg-surface text-fg-muted pointer-events-none absolute left-0 top-full z-10 mt-1 whitespace-nowrap rounded-sm border px-2 py-1 text-xs opacity-0 transition-opacity group-hover:opacity-100"
              >
                {PLANNED_TOOLTIP}
              </span>
            </span>
          ))}
        </div>
        <span id={plannedDescriptionId} className="sr-only">
          {PLANNED_TOOLTIP}
        </span>
      </details>
    </aside>
  );
}
