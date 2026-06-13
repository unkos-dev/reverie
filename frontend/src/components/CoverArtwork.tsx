/**
 * Typographic spine — the cover fallback primitive (spec §5, S8).
 *
 * Real `cover_url` art is canonical; when a book has none the grid
 * renders one of five spine compositions (standard, monogram,
 * vertical, framed, band) in one of four colorways (ink, cream,
 * parchment, gold). Assignment hashes the book id, so the same book
 * wears the same spine across renders, themes, and reloads.
 *
 * Covers are brand-fixed and theme-independent — a publisher's spine
 * does not shift with the reader's theme. The `--cover-*` tokens are
 * declared once on `:root` with no `[data-theme]` override. Because
 * `--cover-parchment` equals the Light canvas, every cover carries the
 * pedestal treatment (hairline + shadow) so books stay objects on
 * Parchment; Dark neutralizes it (spec §5 cover pedestal).
 *
 * Promoted from the D4 fixture (`pages/design/components/CoverArtwork`,
 * left in place for the dev-only design routes).
 */
import type { ReactElement } from "react";

import { cn } from "@/lib/utils";

const LAYOUTS = ["standard", "monogram", "vertical", "framed", "band"] as const;
type SpineLayout = (typeof LAYOUTS)[number];

const COLORWAYS = ["ink", "cream", "parchment", "gold"] as const;
type SpineColorway = (typeof COLORWAYS)[number];

interface ColorwayClasses {
  ground: string;
  display: string;
  body: string;
  rule: string;
  ruleFill: string;
  groundFill: string;
}

const COLORWAY_CLASSES: Record<SpineColorway, ColorwayClasses> = {
  ink: {
    ground: "bg-cover-ink",
    display: "text-cover-cream",
    body: "text-cover-cream",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-ink",
  },
  cream: {
    ground: "bg-cover-cream",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-cream",
  },
  parchment: {
    ground: "bg-cover-parchment",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-gold",
    ruleFill: "fill-cover-gold",
    groundFill: "fill-cover-parchment",
  },
  gold: {
    ground: "bg-cover-gold",
    display: "text-cover-ink",
    body: "text-cover-ink",
    rule: "bg-cover-ink",
    ruleFill: "fill-cover-ink",
    groundFill: "fill-cover-gold",
  },
};

/** Cheap stable string hash (not cryptographic — distribution only). */
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
 * Deterministic typographic cover. Fills its container (`size-full`);
 * the caller owns the aspect box. Marked `aria-hidden` — surrounding
 * link text already carries the accessible label.
 */
export function CoverArtwork({
  bookId,
  title,
  authors,
  className,
}: CoverArtworkProps): ReactElement {
  const h = hashId(bookId);
  const layout: SpineLayout = LAYOUTS[h % LAYOUTS.length] ?? "standard";
  const colorway: SpineColorway = COLORWAYS[Math.floor(h / 5) % COLORWAYS.length] ?? "ink";
  const palette = COLORWAY_CLASSES[colorway];
  const author = authors[0] ?? "";

  return (
    <div
      data-layout={layout}
      data-colorway={colorway}
      aria-hidden="true"
      className={cn(
        "relative size-full overflow-hidden",
        // Pedestal: hairline + shadow keep parchment covers readable as
        // objects on the Light canvas; Dark needs no lift.
        "border-border-strong border shadow-sm dark:border-transparent dark:shadow-none",
        palette.ground,
        className,
      )}
    >
      {layout === "standard" ? (
        <StandardSpine palette={palette} title={title} author={author} />
      ) : null}
      {layout === "monogram" ? (
        <MonogramSpine palette={palette} title={title} author={author} />
      ) : null}
      {layout === "vertical" ? (
        <VerticalSpine palette={palette} title={title} author={author} />
      ) : null}
      {layout === "framed" ? <FramedSpine palette={palette} title={title} author={author} /> : null}
      {layout === "band" ? <BandSpine palette={palette} title={title} author={author} /> : null}
    </div>
  );
}

interface SpineProps {
  palette: ColorwayClasses;
  title: string;
  author: string;
}

/** Slot mark + top rule, title block lower third, author at the foot. */
function StandardSpine({ palette, title, author }: SpineProps): ReactElement {
  return (
    <>
      <svg
        viewBox="0 0 300 400"
        className="absolute inset-0 h-full w-full"
        preserveAspectRatio="xMidYMid slice"
      >
        <rect x="20" y="20" width="20" height="20" className={palette.ruleFill} />
        <rect x="22.5" y="29" width="15" height="2" className={palette.groundFill} />
        <rect x="20" y="56" width="60" height="1.5" className={palette.ruleFill} />
        <rect x="20" y="332" width="80" height="1" opacity="0.7" className={palette.ruleFill} />
      </svg>
      <div className="absolute inset-0 flex flex-col justify-between p-[7%]">
        <div className="h-[16%]" />
        <div
          className={cn(
            "font-display line-clamp-4 text-base font-semibold leading-[1.05] tracking-tight",
            palette.display,
          )}
        >
          {title}
        </div>
        <AuthorLine palette={palette} author={author} />
      </div>
    </>
  );
}

/** Oversized display-face initial behind the title block. */
function MonogramSpine({ palette, title, author }: SpineProps): ReactElement {
  return (
    <div className="absolute inset-0 flex flex-col justify-between p-[7%]">
      <div
        className={cn(
          "font-display text-[5.5rem] font-semibold leading-none opacity-90",
          palette.display,
        )}
      >
        {(title.charAt(0) || "·").toUpperCase()}
      </div>
      <div className="flex flex-col gap-2">
        <div className={cn("h-[2px] w-8", palette.rule)} />
        <div
          className={cn(
            "font-display line-clamp-3 text-sm font-semibold leading-tight",
            palette.display,
          )}
        >
          {title}
        </div>
        <AuthorLine palette={palette} author={author} />
      </div>
    </div>
  );
}

/** Title runs top-to-bottom along the right edge like a shelf spine. */
function VerticalSpine({ palette, title, author }: SpineProps): ReactElement {
  return (
    <div className="absolute inset-0 flex flex-row-reverse items-stretch justify-between p-[7%]">
      {/* line-clamp (-webkit-box) breaks under vertical writing modes —
          nowrap + ellipsis truncates correctly along the block axis. */}
      <div
        className={cn(
          "font-display max-h-full overflow-hidden text-ellipsis whitespace-nowrap text-lg font-semibold tracking-tight [writing-mode:vertical-rl]",
          palette.display,
        )}
      >
        {title}
      </div>
      <div className="flex flex-col justify-end gap-2">
        <div className={cn("h-[2px] w-6", palette.rule)} />
        <AuthorLine palette={palette} author={author} />
      </div>
    </div>
  );
}

/** Inset hairline frame, centred title. */
function FramedSpine({ palette, title, author }: SpineProps): ReactElement {
  return (
    <div className="absolute inset-0 p-[7%]">
      <div
        className={cn(
          "flex h-full flex-col items-center justify-center gap-3 border p-[8%] text-center",
          palette.display,
          "border-current/40",
        )}
      >
        <div
          className={cn(
            "font-display line-clamp-4 text-base font-semibold leading-[1.1] tracking-tight",
            palette.display,
          )}
        >
          {title}
        </div>
        <div className={cn("h-[1.5px] w-8", palette.rule)} />
        <AuthorLine palette={palette} author={author} />
      </div>
    </div>
  );
}

/**
 * Gold jacket band across the middle carrying the title.
 *
 * The band itself is hardcoded `cover-gold`/`cover-ink` regardless of
 * the assigned colorway — the gold band IS this composition's identity.
 * Only the decorative rule above it takes the palette; don't "fix" the
 * band to thread `palette` through.
 */
function BandSpine({ palette, title, author }: SpineProps): ReactElement {
  return (
    <div className="absolute inset-0 flex flex-col justify-center">
      <div className="bg-cover-gold flex flex-col gap-1.5 px-[7%] py-[8%]">
        <div className="font-display text-cover-ink line-clamp-3 text-base font-semibold leading-[1.05] tracking-tight">
          {title}
        </div>
        <div className="text-cover-ink font-body text-[0.62rem] uppercase tracking-[0.18em] opacity-85">
          {author}
        </div>
      </div>
      <div className={cn("absolute inset-x-[7%] top-[8%] h-[1.5px] opacity-70", palette.rule)} />
    </div>
  );
}

/** Shared author footer line. */
function AuthorLine({
  palette,
  author,
}: {
  palette: ColorwayClasses;
  author: string;
}): ReactElement {
  return (
    <div
      className={cn(
        "font-body line-clamp-1 text-[0.62rem] uppercase tracking-[0.18em] opacity-85",
        palette.body,
      )}
    >
      {author}
    </div>
  );
}
