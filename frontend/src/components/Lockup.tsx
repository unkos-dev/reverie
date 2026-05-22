import type { CSSProperties, ReactElement } from "react";

interface LockupProps {
  size?: number;
  theme?: "dark" | "light";
  className?: string;
}

const REVERIE_GOLD = "#C9A961";
const INK = "#0E0D0A";
const CREAM = "#E8E0D0";

/**
 * Brand lockup (glyph + wordmark "Reverie"). Documented exemption from the
 * `no-restricted-syntax` hex-literal ban — the philosophy spec § 11C
 * invariant requires the lockup to render correctly even when the theme
 * tree has not resolved yet (e.g. on the OIDC error page where the
 * canonical token CSS may not yet be on the wire). Hex constants are
 * carved out for exactly this component plus its test in
 * `frontend/eslint.config.js`.
 *
 * Self-contained inline `style` blocks are used for the same reason: the
 * lockup must paint even when no stylesheet has resolved. Two-token glyph
 * (gold square + ink slot) and the Satoshi wordmark are baked in.
 *
 * @param props.size - Wordmark font-size in pixels. Glyph scales to
 *   `0.95 * size` and the column gap to `0.5 * size`. Defaults to 28px to
 *   match the canonical app-shell header height.
 * @param props.theme - Selects the wordmark colour: `"dark"` uses the
 *   cream tint (for dark surfaces); `"light"` uses ink (for light
 *   surfaces). Glyph colours are constant — the gold square + ink slot
 *   stay readable on both surface families.
 * @param props.className - Optional `className` forwarded to the outer
 *   `<span>` so callers can layout the lockup within their own grid.
 * @returns A semantic `role="img"` span with `aria-label="Reverie"`. The
 *   inner glyph carries `aria-hidden` so screen readers only announce
 *   the brand name once.
 */
export function Lockup({ size = 28, theme = "dark", className }: LockupProps): ReactElement {
  const glyphSize = size * 0.95;
  const gap = size * 0.5;
  const wordColor = theme === "dark" ? CREAM : INK;

  const containerStyle: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: `${String(gap)}px`,
  };

  const wordStyle: CSSProperties = {
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif',
    fontWeight: 700,
    fontSize: `${String(size)}px`,
    letterSpacing: "0.32em",
    textTransform: "uppercase",
    paddingLeft: "0.32em",
    color: wordColor,
    lineHeight: 1,
  };

  return (
    <span className={className} style={containerStyle} role="img" aria-label="Reverie">
      <svg width={glyphSize} height={glyphSize} viewBox="0 0 32 32" aria-hidden="true">
        <rect x="4" y="4" width="24" height="24" fill={REVERIE_GOLD} />
        <rect x="8" y="17" width="16" height="2" fill={INK} />
      </svg>
      <span style={wordStyle}>Reverie</span>
    </span>
  );
}
