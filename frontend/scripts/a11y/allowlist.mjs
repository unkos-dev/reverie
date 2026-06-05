// @ts-check
//
// Accessibility-gate allowlist + verdict (pure, side-effect-free).
//
// The a11y CI gate (scripts/a11y/axe-scan.mjs) runs axe-core against the
// dev-only design showcase and must fail on any WCAG 2.2 AA violation EXCEPT
// the one deliberate brand carve-out: Reverie Gold on large CTAs.
//
// Discriminator is element ROLE read from `node.html`, NOT background colour.
// The lg-button carve-out matches on role because background colour is not a
// reliable discriminator: the default Badge variant once rendered the same gold
// background `#8e6f38` as the permitted lg buttons (de-gilded in UNK-345), and
// future surfaces could collide again. Matching also uses `node.html` rather
// than `node.target` because the button loading state's target is the
// `.animate-[loading-pulse…]` class with no `data-size`, while its html carries
// `data-slot="button" data-size="lg"`. See frontend/DESIGN.md §2 "Light-Gold
// Restriction Rule": gold on light surfaces is permitted only on focus rings,
// large CTAs, and recovery actions — "axe-core contrast violations on small-text
// gold are the right signal: the surface is misusing the accent."

/**
 * Documented, accepted WCAG carve-outs. Each entry matches a node iff the
 * violation rule id equals `ruleId` AND every string in `htmlIncludesAll` is a
 * substring of the node's `html`. Keep this list tiny and rationale-bearing;
 * every entry is an accessibility exception a reviewer must be able to justify.
 */
export const ALLOWLIST = [
  {
    ruleId: "color-contrast",
    // Large CTA = the primary button affordance. data-size="lg" excludes the
    // small default badge, which shares the same gold bg but is not a CTA.
    htmlIncludesAll: ['data-slot="button"', 'data-size="lg"'],
    rationale:
      "DESIGN.md §2 Light-Gold Restriction: Reverie Gold is permitted on large CTAs (primary buttons). Cream-on-gold ~3.44:1 is the accepted brand carve-out for this surface only.",
    issue: null,
  },
];

/**
 * Whether a single axe node is covered by a documented allowlist entry.
 * Fails closed: a node with missing/empty `html` is never allowlisted.
 *
 * @param {string} ruleId axe violation rule id (e.g. "color-contrast")
 * @param {{ html?: string }} node axe result node
 * @returns {boolean}
 */
export function isNodeAllowed(ruleId, node) {
  const html = node?.html;
  if (typeof html !== "string" || html.length === 0) {
    return false;
  }
  return ALLOWLIST.some(
    (entry) => entry.ruleId === ruleId && entry.htmlIncludesAll.every((s) => html.includes(s)),
  );
}

/**
 * Strip allowlisted nodes from each violation; drop a violation entirely when
 * none of its nodes remain. Returns only the violations that still represent a
 * real, non-exempt failure.
 *
 * @param {ReadonlyArray<{ id: string, nodes?: ReadonlyArray<{ html?: string }> }>} violations
 * @returns {Array<{ id: string, nodes: Array<{ html?: string }> }>}
 */
export function filterAllowed(violations) {
  const remaining = [];
  for (const v of violations ?? []) {
    const nodes = (v.nodes ?? []).filter((n) => !isNodeAllowed(v.id, n));
    if (nodes.length > 0) {
      remaining.push({ ...v, nodes });
    }
  }
  return remaining;
}

/**
 * Whether a scanned pathname matches the intended target, ignoring a trailing
 * slash on either side. The runner's liveness check uses this: the client
 * router (or Vite) may normalise `/design/system` to `/design/system/`, and a
 * naive suffix match would wrongly accept `/system` for `/design/system` — so
 * compare slash-stripped and exact.
 *
 * @param {unknown} url scanned `window.location.pathname`
 * @param {string} target intended path (begins with "/")
 * @returns {boolean}
 */
export function urlMatches(url, target) {
  if (typeof url !== "string") {
    return false;
  }
  const strip = (/** @type {string} */ s) => s.replace(/\/+$/, "") || "/";
  return strip(url) === strip(target);
}

/**
 * Gate verdict. Passes only when the scan genuinely ran AND no non-allowlisted
 * violation remains. An empty result with `scanOk: false` (crashed browser,
 * blank/wrong page) must FAIL — empty is not the same as "0 violations".
 *
 * @param {{ violations?: ReadonlyArray<{ id: string, nodes?: ReadonlyArray<{ html?: string }> }>, scanOk?: boolean }} input
 * @returns {{ pass: boolean, remaining: Array<{ id: string, nodes: Array<{ html?: string }> }> }}
 */
export function verdict({ violations = [], scanOk = false } = {}) {
  const remaining = filterAllowed(violations);
  return { pass: scanOk === true && remaining.length === 0, remaining };
}
