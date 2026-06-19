/**
 * Cinematic mode for the Library reading room (V5 spec §5).
 *
 * Press `F` (outside any field) to dissolve the surrounding chrome; `F`
 * again or `Escape` exits. After ~2s of pointer stillness the cursor
 * hides too, returning on the next move. State is reflected on the
 * document element as `data-cinematic` / `data-cursor-hidden` so chrome
 * anywhere in the shell can respond purely through CSS (see
 * `styles/motion.css`); the hook itself touches no component tree.
 */
import { useEffect, useState } from "react";

/** Pointer-idle delay before the cursor hides in cinematic mode. */
const CURSOR_IDLE_MS = 2000;

/** True when the caller is typing into an editable element. */
function inEditableField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

/** Drives cinematic mode; returns whether it is currently active. */
export function useCinematicMode(): boolean {
  const [active, setActive] = useState(false);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (
        (event.key === "f" || event.key === "F") &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !inEditableField(event.target)
      ) {
        event.preventDefault();
        setActive((current) => !current);
      } else if (event.key === "Escape") {
        setActive(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

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

  return active;
}
