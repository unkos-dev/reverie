/**
 * Cloth-bound spine — the cover fallback primitive (V5 cover generator).
 *
 * Real `cover_url` art is canonical; when a book has none the grid renders
 * a generated cloth-bound cover: a warm-dark binding cloth (one of six
 * family-related tones) under a real binding texture (linen, buckram,
 * marbled endpaper, plain cloth, leather), framed by a double gilt rule
 * with gilt-foil title and mono author. Assignment hashes the book id, so
 * the same book wears the same binding across renders, themes, and reloads.
 *
 * Covers are atmosphere (Tier 3): physical objects, theme-fixed. The cloth
 * (`--atm-cloth-*`) and gilt (`--atm-gilt-*`) tones do NOT switch when the
 * reader toggles Light↔Dark — a binding does not change material when the
 * room lights do. A subtle pedestal shadow keeps the dark cloth reading as
 * an object on the Light parchment canvas.
 */
import type { ReactElement } from "react";
import { useId } from "react";

import { cn } from "@/lib/utils";

/** Binding cloth tone (resolves to `--atm-cloth-<tone>-c/-e`). */
type ClothTone = "bordeaux" | "oxblood" | "midnight" | "charcoal" | "sepia" | "terracotta";

/** Binding-cloth weave drawn over the cloth ground. */
type Texture = "linen" | "buckram" | "marbled" | "plain" | "leather";

interface Binding {
  tone: ClothTone;
  texture: Texture;
}

/** Six warm-dark bindings, each cloth tone paired with a weave. */
const BINDINGS: readonly Binding[] = [
  { tone: "bordeaux", texture: "linen" },
  { tone: "oxblood", texture: "buckram" },
  { tone: "midnight", texture: "marbled" },
  { tone: "charcoal", texture: "plain" },
  { tone: "sepia", texture: "leather" },
  { tone: "terracotta", texture: "linen" },
];

/** Cheap stable string hash (not cryptographic — distribution only). */
function hashId(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (h * 31 + id.charCodeAt(i)) >>> 0;
  }
  return h;
}

/**
 * Greedy word-wrap for the gilt title — at most three lines of ~13
 * characters. Overflow words are dropped (titles are decorative here; the
 * link carries the full accessible name).
 */
function wrapTitle(title: string): string[] {
  const MAX_CHARS = 13;
  const MAX_LINES = 3;
  const words = title.trim().split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current === "" ? word : `${current} ${word}`;
    if (current !== "" && candidate.length > MAX_CHARS) {
      lines.push(current);
      current = word;
      if (lines.length === MAX_LINES) break;
    } else {
      current = candidate;
    }
  }
  if (lines.length < MAX_LINES && current !== "") lines.push(current);
  return lines.length > 0 ? lines : [title.slice(0, MAX_CHARS)];
}

/** Title face shrinks as the longest line grows (V5 sizing). */
function titleFontSize(lines: string[]): number {
  const longest = Math.max(...lines.map((l) => l.length));
  if (longest > 11) return 14;
  if (longest > 7) return 17;
  return 22;
}

/** SVG `<pattern>` for one binding weave. Ids are caller-unique. */
function texturePattern(id: string, kind: Texture): ReactElement {
  switch (kind) {
    case "linen":
      return (
        <pattern id={id} width="5" height="5" patternUnits="userSpaceOnUse">
          <line x1="0" y1="0" x2="0" y2="5" stroke="rgba(255,255,255,0.10)" strokeWidth="1" />
          <line x1="0" y1="0" x2="5" y2="0" stroke="rgba(0,0,0,0.22)" strokeWidth="1" />
        </pattern>
      );
    case "buckram":
      return (
        <pattern
          id={id}
          width="7"
          height="7"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <line x1="0" y1="0" x2="0" y2="7" stroke="rgba(0,0,0,0.30)" strokeWidth="1" />
          <line x1="3" y1="0" x2="3" y2="7" stroke="rgba(255,255,255,0.06)" strokeWidth="0.6" />
        </pattern>
      );
    case "marbled":
      return (
        <pattern id={id} width="34" height="64" patternUnits="userSpaceOnUse">
          <path
            d="M0 14 Q8 8 16 16 T34 12"
            stroke="rgba(255,255,255,0.12)"
            fill="none"
            strokeWidth="0.9"
          />
          <path
            d="M0 30 Q10 24 18 32 T34 28"
            stroke="rgba(0,0,0,0.22)"
            fill="none"
            strokeWidth="0.9"
          />
          <path
            d="M0 46 Q9 40 17 48 T34 44"
            stroke="rgba(255,255,255,0.08)"
            fill="none"
            strokeWidth="0.8"
          />
          <path
            d="M0 58 Q11 52 19 60 T34 56"
            stroke="rgba(0,0,0,0.16)"
            fill="none"
            strokeWidth="0.7"
          />
        </pattern>
      );
    case "leather":
      return (
        <pattern id={id} width="14" height="14" patternUnits="userSpaceOnUse">
          <ellipse cx="3" cy="2" rx="0.9" ry="0.6" fill="rgba(255,255,255,0.14)" />
          <ellipse cx="9" cy="5" rx="0.7" ry="0.5" fill="rgba(0,0,0,0.28)" />
          <ellipse cx="2" cy="10" rx="0.6" ry="0.8" fill="rgba(0,0,0,0.22)" />
          <ellipse cx="11" cy="11" rx="0.9" ry="0.6" fill="rgba(255,255,255,0.10)" />
          <ellipse cx="7" cy="8" rx="0.7" ry="0.7" fill="rgba(255,255,255,0.08)" />
          <ellipse cx="13" cy="3" rx="0.6" ry="0.5" fill="rgba(0,0,0,0.20)" />
        </pattern>
      );
    case "plain":
    default:
      return (
        <pattern id={id} width="4" height="4" patternUnits="userSpaceOnUse">
          <rect width="1" height="1" fill="rgba(255,255,255,0.08)" />
          <rect x="2" y="2" width="1" height="1" fill="rgba(0,0,0,0.18)" />
        </pattern>
      );
  }
}

interface CoverArtworkProps {
  bookId: string;
  title: string;
  authors: string[];
  className?: string;
}

/**
 * Deterministic cloth-bound cover. Fills its container (`size-full`); the
 * caller owns the aspect box. Marked `aria-hidden` — the surrounding link
 * text already carries the accessible label; the `<title>` element is for
 * tooling/tests only.
 */
export function CoverArtwork({
  bookId,
  title,
  authors,
  className,
}: CoverArtworkProps): ReactElement {
  // Stable per-mount, url(#…)-safe id namespace for the SVG defs.
  const svgId = useId().replace(/:/g, "");
  const binding = BINDINGS[hashId(bookId) % BINDINGS.length] ?? {
    tone: "charcoal",
    texture: "plain",
  };
  const { tone, texture } = binding;
  const author = (authors[0] ?? "").toUpperCase();

  const lines = wrapTitle(title);
  const size = titleFontSize(lines);
  const lineHeight = size * 1.06;
  const titleY = 200 - ((lines.length - 1) * lineHeight) / 2;

  const clothId = `cloth_${svgId}`;
  const leafId = `leaf_${svgId}`;
  const ruleId = `rule_${svgId}`;
  const texId = `tex_${svgId}`;
  const vignId = `vign_${svgId}`;

  return (
    <div
      data-cloth={tone}
      data-texture={texture}
      aria-hidden="true"
      className={cn(
        "relative size-full overflow-hidden",
        // Pedestal keeps the dark cloth reading as an object on parchment;
        // Dark canvas needs no lift.
        "shadow-sm dark:shadow-none",
        className,
      )}
    >
      <svg viewBox="0 0 240 360" preserveAspectRatio="xMidYMid slice" className="size-full">
        <title>{title}</title>
        <defs>
          <linearGradient id={clothId} x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor={`var(--atm-cloth-${tone}-c)`} />
            <stop offset="1" stopColor={`var(--atm-cloth-${tone}-e)`} />
          </linearGradient>
          <linearGradient id={leafId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--atm-gilt-0)" />
            <stop offset="35%" stopColor="var(--atm-gilt-1)" />
            <stop offset="55%" stopColor="var(--atm-gilt-2)" />
            <stop offset="80%" stopColor="var(--atm-gilt-3)" />
            <stop offset="100%" stopColor="var(--atm-gilt-4)" />
          </linearGradient>
          <linearGradient id={ruleId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--atm-gilt-1)" />
            <stop offset="100%" stopColor="var(--atm-gilt-4)" />
          </linearGradient>
          {texturePattern(texId, texture)}
          <radialGradient id={vignId} cx="0.5" cy="0.5" r="0.7">
            <stop offset="0%" stopColor="rgba(0,0,0,0)" />
            <stop offset="100%" stopColor="rgba(0,0,0,0.55)" />
          </radialGradient>
        </defs>

        <rect width="240" height="360" fill={`url(#${clothId})`} />
        <rect width="240" height="360" fill={`url(#${texId})`} />
        <rect width="240" height="360" fill={`url(#${vignId})`} />
        {/* Spine edge-darkening down each side (atmosphere depth, not chrome). */}
        <rect x="0" y="0" width="6" height="360" fill="rgba(0,0,0,0.35)" />
        <rect x="234" y="0" width="6" height="360" fill="rgba(0,0,0,0.35)" />
        {/* Double gilt frame. */}
        <rect
          x="14"
          y="14"
          width="212"
          height="332"
          fill="none"
          stroke={`url(#${ruleId})`}
          strokeWidth="0.7"
          opacity="0.7"
        />
        <rect
          x="18"
          y="18"
          width="204"
          height="324"
          fill="none"
          stroke={`url(#${ruleId})`}
          strokeWidth="0.3"
          opacity="0.4"
        />
        {/* Top ornament. */}
        <g
          transform="translate(120 56)"
          stroke={`url(#${ruleId})`}
          strokeWidth="0.7"
          opacity="0.75"
          fill="none"
        >
          <path d="M-26 0h52" />
          <circle r="2.4" fill={`url(#${ruleId})`} stroke="none" />
          <path d="M-34 -6l8 6 -8 6 M34 -6l-8 6 8 6" />
        </g>
        {/* Gilt-foil title. */}
        <text
          x="120"
          y={titleY}
          fill={`url(#${leafId})`}
          textAnchor="middle"
          fontStyle="italic"
          fontWeight="600"
          fontSize={size}
          letterSpacing="0.005em"
          className="font-display"
        >
          {lines.map((line, i) => (
            <tspan key={line + String(i)} x="120" dy={i === 0 ? 0 : lineHeight}>
              {line}
            </tspan>
          ))}
        </text>
        {/* Author. */}
        {author !== "" ? (
          <text
            x="120"
            y="282"
            fill={`url(#${ruleId})`}
            textAnchor="middle"
            opacity="0.9"
            fontWeight="500"
            fontSize="8"
            letterSpacing="0.22em"
            className="font-body"
          >
            {author}
          </text>
        ) : null}
        {/* Bottom ornament. */}
        <g
          transform="translate(120 312)"
          stroke={`url(#${ruleId})`}
          strokeWidth="0.6"
          opacity="0.6"
          fill="none"
        >
          <path d="M-22 0h44" />
          <path d="M0 -4l3 4 -3 4 -3 -4z" fill={`url(#${ruleId})`} stroke="none" />
        </g>
      </svg>
    </div>
  );
}
