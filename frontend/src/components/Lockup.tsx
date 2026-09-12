import type { CSSProperties, ReactElement } from "react";

type LockupProps = {
  size?: number;
  theme?: "dark" | "light";
  className?: string;
};

const INK = "#0E0D0A";
const CREAM = "#E8E0D0";

/** Glyph edge length as a multiple of the wordmark type size. */
const GLYPH_RATIO = 1.4;

/**
 * Rendered glyph sizes below this fill the standard slot from anti-aliasing,
 * so they take the thicker canonical variant instead.
 */
const THICK_SLOT_BELOW_PX = 24;

/**
 * Brand lockup: the Slot glyph followed by the Reverie wordmark.
 *
 * Metrics follow the canonical lockup construction. Glyph, gap and wordmark
 * pad are all multiples of the wordmark type size, which the outer span
 * carries so each length resolves against it.
 *
 * The wordmark is styled inline so it stays correct on surfaces that paint
 * before the theme tree resolves. The glyph is fetched from the shipped brand
 * assets, so a failed request leaves the wordmark alone rather than an
 * approximation of the mark that could drift from the canonical artwork.
 *
 * @param props.size - Wordmark type size in pixels. Defaults to 28px.
 * @param props.theme - Selects the wordmark colour: `"dark"` uses the
 *   cream tint (for dark surfaces); `"light"` uses ink (for light surfaces).
 * @param props.className - Optional `className` forwarded to the outer
 *   `<span>` so callers can layout the lockup within their own grid.
 * @returns A semantic `role="img"` span with `aria-label="Reverie"`. The
 *   inner glyph carries `aria-hidden` so screen readers only announce
 *   the brand name once.
 */
export function Lockup({ size = 28, theme = "dark", className }: LockupProps): ReactElement {
  const glyphSource =
    size * GLYPH_RATIO < THICK_SLOT_BELOW_PX
      ? "/brand/glyph/slot-favicon.svg"
      : "/brand/glyph/slot.svg";
  const wordColor = theme === "dark" ? CREAM : INK;

  const containerStyle: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: "0.48em",
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif',
    fontWeight: 700,
    fontSize: `${String(size)}px`,
  };

  const glyphStyle: CSSProperties = {
    width: `${String(GLYPH_RATIO)}em`,
    height: `${String(GLYPH_RATIO)}em`,
    flex: "none",
  };

  const wordStyle: CSSProperties = {
    letterSpacing: "0.32em",
    textTransform: "uppercase",
    paddingLeft: "0.32em",
    color: wordColor,
    lineHeight: 1,
  };

  return (
    <span className={className} style={containerStyle} role="img" aria-label="Reverie">
      <img src={glyphSource} alt="" aria-hidden="true" style={glyphStyle} />
      <span style={wordStyle}>Reverie</span>
    </span>
  );
}
