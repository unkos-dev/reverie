import type { CSSProperties, ReactElement } from "react";

type LockupProps = {
  size?: number;
  theme?: "dark" | "light";
  className?: string;
};

const INK = "#0E0D0A";
const CREAM = "#E8E0D0";

/**
 * Brand lockup (Slot glyph plus the Reverie wordmark). Inline styles keep the
 * wordmark correct before the theme tree resolves, while the glyph always
 * loads from the canonical shipped asset.
 *
 * @param props.size - Wordmark font size in pixels. Glyph size and gap scale
 *   from Satoshi's cap height. Defaults to 28px to match the app-shell header.
 * @param props.theme - Selects the wordmark colour: `"dark"` uses the
 *   cream tint (for dark surfaces); `"light"` uses ink (for light surfaces).
 * @param props.className - Optional `className` forwarded to the outer
 *   `<span>` so callers can layout the lockup within their own grid.
 * @returns A semantic `role="img"` span with `aria-label="Reverie"`. The
 *   inner glyph carries `aria-hidden` so screen readers only announce
 *   the brand name once.
 */
export function Lockup({ size = 28, theme = "dark", className }: LockupProps): ReactElement {
  const glyphSource = size < 24 ? "/brand/glyph/slot-favicon.svg" : "/brand/glyph/slot.svg";
  const wordColor = theme === "dark" ? CREAM : INK;

  const containerStyle: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: "0.5cap",
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif',
    fontWeight: 700,
    fontSize: `${String(size)}px`,
  };

  const glyphStyle: CSSProperties = {
    width: "0.95cap",
    height: "0.95cap",
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
