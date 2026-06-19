/**
 * Bookmark ribbon — a slim gilt thumb pinned to the right edge that
 * tracks scroll position through the library (V5 spec §5, "the one
 * decisive gold flourish").
 *
 * Purely decorative (`aria-hidden`): the native scrollbar and keyboard
 * scrolling stay the mechanism, so there is no drag affordance and
 * nothing for WCAG 2.5.7 to provide an alternative to. The thumb position
 * is the one genuinely dynamic value, so it rides an inline style. Fades
 * with the rest of the chrome in cinematic mode via `data-chrome`.
 */
import { useEffect, useState, type CSSProperties, type ReactElement } from "react";

/** Gilt scroll-position thumb pinned to the right edge (decorative). */
export function BookmarkRibbon(): ReactElement {
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    let frame = 0;
    function update(): void {
      const max = document.documentElement.scrollHeight - window.innerHeight;
      setProgress(max > 0 ? Math.min(1, Math.max(0, window.scrollY / max)) : 0);
    }
    function onScroll(): void {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(update);
    }
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll, { passive: true });
    // Defer the initial read through rAF rather than setting state
    // synchronously inside the effect.
    onScroll();
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  }, []);

  // Keep the 4rem-tall thumb inside the full-height track at both ends.
  const thumbStyle: CSSProperties = { top: `calc(${String(progress)} * (100vh - 4rem))` };

  return (
    <div
      data-chrome=""
      aria-hidden="true"
      className="pointer-events-none fixed right-1.5 top-0 z-20 hidden h-screen w-1 lg:block"
    >
      <div
        className="bg-accent absolute right-0 h-16 w-full rounded-full opacity-80"
        style={thumbStyle}
      />
    </div>
  );
}
