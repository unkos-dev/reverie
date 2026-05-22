import type { ReactElement } from "react";

import { cn } from "@/lib/utils";

/**
 * Typographic cover artwork.
 *
 * Production cover art comes from the ingested EPUB or an enrichment
 * provider. For D4 hero fixtures we render a brand-aligned typographic
 * stand-in so the library grid and book detail screens read as a real
 * library, not as empty placeholders.
 *
 * Cover artwork is brand-fixed and theme-independent — the cover does
 * not switch palette when the reader toggles Light↔Dark, by intent (a
 * publisher's spine does not shift). The `cover-*` Tailwind tokens
 * (`bg-cover-ink`, `fill-cover-gold`, etc.) map to the `--cover-*` CSS
 * variables declared once on `:root` in `styles/themes/index.css` with
 * no `[data-theme]` override, which gives us the constants without
 * inlining hex literals here (which would also trip the no-hex-in-tsx
 * ESLint rule).
 *
 * Variant is deterministic by book id so the same book renders the
 * same cover every render. The hash is intentionally cheap and stable,
 * not cryptographic.
 */

interface PaletteClasses {
  ground: string;
  display: string;
  body: string;
  rule: string;
  ruleFill: string;
  groundFill: string;
}

const PALETTES: [PaletteClasses, ...PaletteClasses[]] = [
  // 0: ink ground, cream type, gold rule
  {
    ground: "bg-cover-ink",
    display: "text-cover-cream",
    body: "text-cover-cream",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-ink",
  },
  // 1: cream ground, ink type, gold rule
  {
    ground: "bg-cover-cream",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-cream",
  },
  // 2: parchment ground, ink type, gold rule
  {
    ground: "bg-cover-parchment",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-parchment",
  },
  // 3: ink ground, gold display title, cream body, gold rule
  {
    ground: "bg-cover-ink",
    display: "text-cover-gold",
    body: "text-cover-cream",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-ink",
  },
  // 4: cream ground, ink type, ink rule (quieter variant)
  {
    ground: "bg-cover-cream",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-ink",
    ruleFill: "fill-cover-ink",
    groundFill: "fill-cover-cream",
  },
  // 5: gold ground, ink type, ink rule (rare, signature)
  {
    ground: "bg-cover-gold",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-ink",
    ruleFill: "fill-cover-ink",
    groundFill: "fill-cover-gold",
  },
];

function hashId(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (h * 31 + id.charCodeAt(i)) >>> 0;
  }
  return h;
}

interface CoverArtworkProps {
  bookId: string;
  title: string;
  authors: string[];
  className?: string;
}

/**
 * Typographic book-cover SVG keyed deterministically to a book id.
 *
 * Renders a 3:4 brand-palette cover from {@link CoverArtworkProps.title}
 * and the first entry of {@link CoverArtworkProps.authors}; the palette
 * is chosen by hashing {@link CoverArtworkProps.bookId} against the
 * six-variant `PALETTES` table so the same book always gets the same
 * cover across renders, themes, and reloads.
 *
 * Theme-independent by design — the cover acts as a publisher's spine
 * and is intentionally not overridden by the dark/light theme. Marked
 * `aria-hidden` because surrounding link text already provides the
 * accessible label; the SVG itself carries no semantic information.
 */
export function CoverArtwork({
  bookId,
  title,
  authors,
  className,
}: CoverArtworkProps): ReactElement {
  const palette = PALETTES[hashId(bookId) % PALETTES.length] ?? PALETTES[0];
  const author = authors[0] ?? "";

  return (
    <div
      className={cn("relative aspect-[3/4] w-full overflow-hidden", palette.ground, className)}
      aria-hidden="true"
    >
      <svg
        viewBox="0 0 300 400"
        className="absolute inset-0 h-full w-full"
        preserveAspectRatio="xMidYMid slice"
      >
        {/* Slot mark — quiet echo of the Reverie wordmark. */}
        <rect x="20" y="20" width="20" height="20" className={palette.ruleFill} />
        <rect x="22.5" y="29" width="15" height="2" className={palette.groundFill} />
        {/* Top rule under the slot mark. */}
        <rect x="20" y="56" width="60" height="1.5" className={palette.ruleFill} />
        {/* Bottom rule above the author block. */}
        <rect x="20" y="332" width="80" height="1" opacity="0.7" className={palette.ruleFill} />
      </svg>

      <div className="absolute inset-0 flex flex-col justify-between p-[6%]">
        <div className="h-[18%]" />
        <div
          className={cn(
            "font-display font-semibold leading-[1.05] tracking-tight text-base sm:text-lg lg:text-xl line-clamp-4",
            palette.display,
          )}
        >
          {title}
        </div>
        <div
          className={cn(
            "font-body text-[0.62rem] sm:text-[0.7rem] uppercase tracking-[0.18em] opacity-85",
            palette.body,
          )}
        >
          {author}
        </div>
      </div>
    </div>
  );
}
