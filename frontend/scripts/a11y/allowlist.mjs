// @ts-check

/**
 * Scan targets when `A11Y_TARGETS` is unset, which is how CI runs.
 *
 * The setup-status fixture enables the recovery form on the Vite-only server.
 * Fragment-free verification and reset routes render their invalid-link states.
 * Playwright derives its server-readiness URL from the first entry; that probe
 * measures server readiness only, since Vite's SPA fallback answers 200 for any path.
 *
 * Authenticated Home, Library, and Detail are the remaining gap; they need a
 * storage-state fixture and a backend before they can join.
 */
export const DEFAULT_TARGETS = ["/forgot-password", "/verify-email", "/reset-password"];

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
