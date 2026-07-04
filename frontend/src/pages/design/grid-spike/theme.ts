/**
 * Theme bridge (design D5 comparative metric).
 *
 * The same Reverie token set is applied to both grids so the theming-bridge
 * effort is comparable. react-data-grid consumes CSS variables directly (see
 * `theme.css`, mapping `--rdg-*` onto Reverie tokens). AG Grid owns its look in
 * JS through the Theming API, so its bridge lives here: `themeQuartz.withParams`
 * fed `var(--reverie-token)` strings. The asymmetry (CSS map vs JS param object,
 * and AG's inability to derive shades from a `var()` accent) is itself a finding
 * for the spike report.
 */
import { themeQuartz, type Theme } from "ag-grid-community";

/** Shared geometry so both grids render at the same density. */
export const ROW_HEIGHT = 36;
export const HEADER_HEIGHT = 40;

/**
 * AG Grid theme wired to Reverie semantic tokens. Flat color params map
 * cleanly; `accentColor` is passed as a token too, accepting that AG cannot
 * compute derived shades from a `var()` value at build time.
 */
export function buildSpikeAgTheme(): Theme {
  return themeQuartz.withParams({
    accentColor: "var(--accent)",
    backgroundColor: "var(--canvas)",
    foregroundColor: "var(--fg)",
    cellTextColor: "var(--fg)",
    borderColor: "var(--border)",
    headerBackgroundColor: "var(--surface-2)",
    headerTextColor: "var(--fg-muted)",
    rowHoverColor: "var(--surface)",
    selectedRowBackgroundColor: "var(--accent-soft)",
    fontFamily: "inherit",
    fontSize: 14,
    headerHeight: HEADER_HEIGHT,
    rowHeight: ROW_HEIGHT,
    spacing: 6,
    browserColorScheme: "inherit",
  });
}
