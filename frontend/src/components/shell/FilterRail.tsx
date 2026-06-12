/**
 * Contextual filter rail for browse surfaces (spec §2, S11).
 *
 * This epic ships structural + API-backed facets only:
 *
 * - **Series** — live single-select radios writing `?series=<uuid>`
 *   (the list payload's `SeriesRef` carries the id the backend
 *   deserializes). Re-selecting the active value, or Clear, removes
 *   the param.
 * - **Author** — disabled placeholder rows carrying real names
 *   (spec S3 treatment). The backend expects `?author=<uuid>` but the
 *   list payload exposes author *names* only — sending a name 400s.
 *   Goes live in UNK-387 with multi-select, counts, and the remaining
 *   facets. No fake-interactive UI (spec §12).
 *
 * Facet options derive from the loaded pages — the owning page passes
 * them in; the rail holds no fetch of its own.
 */
import type { ReactElement } from "react";
import { useSearchParams } from "react-router";

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
                type="radio"
                name="series-facet"
                value={series.id}
                checked={activeSeries === series.id}
                onChange={() => {
                  setSeries(series.id);
                }}
                onClick={() => {
                  // Native radios don't deselect — toggle off when the
                  // active value is clicked again (spec §10 active-
                  // filter clearing).
                  if (activeSeries === series.id) setSeries(null);
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
        <summary className="text-fg-faint cursor-pointer select-none font-mono text-xs uppercase tracking-[0.14em]">
          Author
        </summary>
        <div className="mt-2 flex flex-col gap-0.5">
          {authorNames.map((name) => (
            <span
              key={name}
              aria-disabled="true"
              aria-describedby="author-facet-planned"
              className="text-fg-faint group relative flex cursor-default items-center rounded-md px-2 py-1.5"
            >
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
        <span id="author-facet-planned" className="sr-only">
          {PLANNED_TOOLTIP}
        </span>
      </details>
    </aside>
  );
}
