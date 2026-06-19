/**
 * Ambient atmosphere for the Library page — a fixed, slowly breathing
 * ember radial plus a film-grain overlay that sit behind the library
 * content. Decorative only (`aria-hidden`); the visual rules live in
 * `styles/motion.css` (`.lib-atm` / `.lib-grain`) and read the Tier-3
 * `--atm-*` tokens (the sanctioned art-directed use — the ambient field
 * IS the atmosphere, not chrome).
 *
 * Library-scoped by intent: mount inside the Library page, never in the
 * app shell, so utility/admin screens stay flat. Both layers are
 * `position: fixed`, so one instance covers the viewport; render it once
 * at the top of the page and give the page content a stacking context
 * above it (`relative z-[2]`).
 */
import type { ReactElement } from "react";

/** Fixed ember-radial + film-grain layers behind the Library page (decorative). */
export function Atmosphere(): ReactElement {
  return (
    <>
      <div className="lib-atm" aria-hidden="true" />
      <div className="lib-grain" aria-hidden="true" />
    </>
  );
}
