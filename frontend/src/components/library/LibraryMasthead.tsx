/**
 * Library page masthead: the full-bleed photographic hero band with the
 * page title set inside it, bottom-left, in accent ink over a canvas
 * gradient wash (`.lib-hero-wash` in `styles/motion.css`).
 *
 * The hero photo is theme-specific (gilt spines, dark and light
 * treatments). The effective theme picks which asset renders so only
 * one image downloads; both are decorative (`alt=""`).
 *
 * Count-bearing prose ("N volumes · M authors") is intentionally absent
 * until a user-scoped count source exists; fabricated totals would be
 * worse than omitting them.
 */
import type { ReactElement } from "react";

import { useTheme } from "@/lib/theme/ThemeProvider";
import { cn } from "@/lib/utils";

type LibraryMastheadProps = {
  /** Hero title, set inside the photo band. */
  title?: string;
  /**
   * Bleed utilities for the full-width hero band. Defaults to the
   * Library page's content padding so the photo reaches the column
   * edges; other consumers pass their own.
   */
  heroBleedClassName?: string;
};

/** The Library hero: photographic band with the title set inside it. */
export function LibraryMasthead({
  title = "Library",
  heroBleedClassName = "-mx-6 -mt-10 sm:-mx-10",
}: LibraryMastheadProps): ReactElement {
  const { effective } = useTheme();
  const variant = effective === "light" ? "-light" : "";
  return (
    <header className="mb-8">
      <div className={cn("lib-hero", heroBleedClassName)}>
        <picture key={variant}>
          <source
            srcSet={`/editorial/library-hero-gilt-spines${variant}.webp`}
            type="image/webp"
          />
          <img
            src={`/editorial/library-hero-gilt-spines${variant}.jpg`}
            alt=""
            decoding="async"
            fetchPriority="high"
          />
        </picture>
        <div className="lib-hero-wash" aria-hidden="true" />
        <div className="lib-hero-inner">
          <h1 className="lib-hero-title font-display font-medium">{title}</h1>
        </div>
      </div>
    </header>
  );
}
