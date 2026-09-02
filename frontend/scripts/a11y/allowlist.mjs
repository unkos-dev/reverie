// @ts-check
//
// Accessibility-gate allowlist + verdict (pure, side-effect-free).
//
// The a11y CI gate (scripts/a11y/a11y.spec.ts) runs axe-core against the
// routes in DEFAULT_TARGETS below and must fail on any WCAG 2.2 AA violation.
// The allowlist is empty, which is the state to keep it in.
//
// A carve-out belongs here only when axe MEASURES the surface wrongly. A real
// contrast shortfall is a design defect and must be fixed in the surface, not
// suppressed here. Entries match on `node.html` rather than `node.target`
// because a target can be an incidental class: the button loading state's
// target is `.animate-[loading-pulse…]` with no `data-size`, while its html
// still carries the identifying `data-slot` / `data-size` attributes.
//
// The brand's gold restriction is satisfied without an exception here.
// `--fg-on-accent` resolves to `gold-contrast` (Ink), so the lg primary button
// measures ink-on-gold at 5.11:1 on Light and 8.64:1 on Dark, both clear of
// 4.5:1 at its 14px/500 type. frontend/DESIGN.md §2 "Light-Gold Restriction
// Rule" still governs where the gold-9 fill may be used: large CTAs and
// recovery actions only, with gold that must read as text or a hairline, the
// focus ring above all, taking the `accent-text` (gold-11) step instead.

/**
 * Documented, accepted WCAG carve-outs. Each entry matches a node iff the
 * violation rule id equals `ruleId` AND every string in `htmlIncludesAll` is a
 * substring of the node's `html`. Empty is the right state; every entry is an
 * accessibility exception a reviewer must be able to justify.
 *
 * @type {Array<{ ruleId: string, htmlIncludesAll: string[], rationale: string, issue: string | null }>}
 */
export const ALLOWLIST = [];

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
 * Scan targets when `A11Y_TARGETS` is unset, which is how CI runs.
 *
 * `/forgot-password` is the only pre-auth route that renders its real markup
 * against the Vite-only server this gate boots: it reaches the API from submit
 * handlers alone, while `/login` and `/setup` each call `fetchSetupStatus` in a
 * mount-time query and would render an error branch instead. Playwright's
 * readiness URL is derived from the first entry, which keeps a second copy of a
 * path out of playwright.config.ts; that probe measures server readiness only,
 * since Vite's SPA fallback answers 200 for any path.
 *
 * Authenticated Home, Library, and Detail are the remaining gap; they need a
 * storage-state fixture and a backend before they can join.
 */
export const DEFAULT_TARGETS = ["/forgot-password"];

/**
 * Parse the `A11Y_TARGETS` env contract into a non-empty list of root-relative
 * paths. Comma-separated, trimmed, empties dropped (same contract as the
 * retired runner); an unset var falls back to `DEFAULT_TARGETS`.
 *
 * Fails closed by THROWING rather than returning a degenerate list:
 *   - a zero-length result (var set to "" or all-whitespace) would register no
 *     tests and let the suite pass without scanning anything;
 *   - an absolute or protocol-relative target (`http://host`, `//host`) makes
 *     `page.goto` leave the configured baseURL and scan an unrelated origin.
 * A top-level throw at spec-collection time aborts the run with a non-zero exit.
 *
 * @param {string | undefined} raw value of `process.env.A11Y_TARGETS`
 * @returns {string[]} one or more paths, each beginning with a single "/"
 */
export function parseTargets(raw) {
  const targets = (raw ?? DEFAULT_TARGETS.join(","))
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
  if (targets.length === 0) {
    throw new Error(
      "A11Y_TARGETS resolved to zero targets: refusing to pass a gate that scans nothing.",
    );
  }
  for (const t of targets) {
    if (!t.startsWith("/") || t.startsWith("//") || t.includes("://")) {
      throw new Error(
        `A11Y_TARGETS entry "${t}" is not a root-relative path: targets must begin with a single "/" so the scan stays on the configured baseURL.`,
      );
    }
  }
  return targets;
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
