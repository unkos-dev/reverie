import type { CSSProperties, ReactElement } from "react";

type LockupProps = {
  size?: number;
  theme?: "dark" | "light";
  className?: string;
};

const INK = "#0E0D0A";
const CREAM = "#E8E0D0";
const REVERIE_GOLD = "#C9A961";
const STANDARD_GLYPH_PATH = "M4 4h24v24H4V4zm4 13h16v2H8v-2z";
const THICK_GLYPH_PATH = "M4 4h24v24H4V4zm3 11h18v3H7v-3z";

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
 * The SVG viewBox removes the canonical artwork's transparent inset so its
 * visible block fills the specified glyph size.
 *
 * Inline styles and canonical SVG paths keep the whole lockup independent
 * of theme CSS and glyph requests. Exact asset parity is checked in tests.
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
  const glyphSize = size * GLYPH_RATIO;
  const glyphPath = glyphSize < THICK_SLOT_BELOW_PX ? THICK_GLYPH_PATH : STANDARD_GLYPH_PATH;
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
      <svg
        width={glyphSize}
        height={glyphSize}
        viewBox="4 4 24 24"
        aria-hidden="true"
        style={glyphStyle}
      >
        <path fillRule="evenodd" clipRule="evenodd" d={glyphPath} fill={REVERIE_GOLD} />
      </svg>
      <span style={wordStyle}>Reverie</span>
    </span>
  );
}
