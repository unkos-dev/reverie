import type { CSSProperties, ReactElement } from "react";

interface LockupProps {
  size?: number;
  theme?: "dark" | "light";
  className?: string;
}

const REVERIE_GOLD = "#C9A961";
const INK = "#0E0D0A";
const CREAM = "#E8E0D0";

export function Lockup({
  size = 28,
  theme = "dark",
  className,
}: LockupProps): ReactElement {
  const glyphSize = size * 0.95;
  const gap = size * 0.5;
  const wordColor = theme === "dark" ? CREAM : INK;

  const containerStyle: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: `${gap}px`,
  };

  const wordStyle: CSSProperties = {
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif',
    fontWeight: 700,
    fontSize: `${size}px`,
    letterSpacing: "0.32em",
    textTransform: "uppercase",
    paddingLeft: "0.32em",
    color: wordColor,
    lineHeight: 1,
  };

  return (
    <span
      className={className}
      style={containerStyle}
      role="img"
      aria-label="Reverie"
    >
      <svg
        width={glyphSize}
        height={glyphSize}
        viewBox="0 0 32 32"
        aria-hidden="true"
      >
        <rect x="4" y="4" width="24" height="24" fill={REVERIE_GOLD} />
        <rect x="8" y="17" width="16" height="2" fill={INK} />
      </svg>
      <span style={wordStyle}>Reverie</span>
    </span>
  );
}
