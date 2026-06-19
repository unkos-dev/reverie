/**
 * Library page masthead — the photographic hero band, the curatorial
 * kicker, and the gilt-foil title. The hero photo is a fixed editorial
 * asset (decorative, served webp-first); the gilt `<h1>` is the
 * once-per-page hero treatment (atmosphere §8) styled by `.lib-h1-gilt`
 * in `styles/motion.css`.
 *
 * Count-bearing prose (the "… N shelves, M authors" subtitle and the
 * "N volumes · M authors · updated" meta line from the V5 spec) is
 * intentionally absent until a user-scoped count source is wired —
 * rendering fabricated totals would be worse than omitting them.
 */
import type { ReactElement } from "react";

import { cn } from "@/lib/utils";

interface LibraryMastheadProps {
  /** Curatorial tagline above the title (display italic). */
  kicker?: string;
  /** Gilt hero title. */
  title?: string;
  /**
   * Bleed utilities for the full-width hero band. Defaults to the
   * Library page's content padding so the photo reaches the column
   * edges; other consumers (e.g. the dev hero page) pass their own.
   */
  heroBleedClassName?: string;
}

/** The Library hero: photographic band, curatorial kicker, and gilt-foil title. */
export function LibraryMasthead({
  kicker = "Your library, catalogued.",
  title = "Library",
  heroBleedClassName = "-mx-6 -mt-10 sm:-mx-10",
}: LibraryMastheadProps): ReactElement {
  return (
    <header className="mb-10">
      <div className={cn("lib-hero mb-8", heroBleedClassName)}>
        <picture>
          <source srcSet="/editorial/library-hero.webp" type="image/webp" />
          <img src="/editorial/library-hero.jpg" alt="" decoding="async" fetchPriority="high" />
        </picture>
        <div className="lib-hero-fade" aria-hidden="true" />
      </div>
      <p className="font-display text-fg-muted mb-3 text-base italic sm:text-lg">{kicker}</p>
      <h1 className="lib-h1-gilt font-display text-[clamp(4rem,9.5vw,9rem)] font-medium leading-[1.02] tracking-[-0.025em] [padding-bottom:0.06em]">
        {title}
      </h1>
    </header>
  );
}
