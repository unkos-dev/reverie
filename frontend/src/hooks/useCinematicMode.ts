/**
 * Cinematic mode for the Library reading room.
 *
 * The visible toggle dissolves surrounding chrome. After ~2s of pointer stillness the cursor
 * hides too, returning on the next move. State is reflected on the
 * document element as `data-cinematic` / `data-cursor-hidden` so chrome
 * anywhere in the shell can respond purely through CSS (see
 * `styles/motion.css`); the hook itself touches no component tree.
 */
import { useEffect, useState } from "react";

/** Pointer-idle delay before the cursor hides in cinematic mode. */
const CURSOR_IDLE_MS = 2000;

export function useCinematicMode(): { active: boolean; setActive: (active: boolean) => void } {
  const [active, setActive] = useState(false);

  useEffect(() => {
    const root = document.documentElement;
    if (!active) {
      root.removeAttribute("data-cinematic");
      root.removeAttribute("data-cursor-hidden");
      return;
    }
    root.setAttribute("data-cinematic", "on");
    let timer = window.setTimeout(() => {
      root.setAttribute("data-cursor-hidden", "on");
    }, CURSOR_IDLE_MS);
    function onMove(): void {
      root.removeAttribute("data-cursor-hidden");
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        root.setAttribute("data-cursor-hidden", "on");
      }, CURSOR_IDLE_MS);
    }
    window.addEventListener("mousemove", onMove);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("mousemove", onMove);
      root.removeAttribute("data-cinematic");
      root.removeAttribute("data-cursor-hidden");
    };
  }, [active]);

  return { active, setActive };
}
