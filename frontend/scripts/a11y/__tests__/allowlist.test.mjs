import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vite-plus/test";

import { ALLOWLIST, filterAllowed, urlMatches, verdict } from "../allowlist.mjs";

// Real axe output captured against /design/system (full WCAG 2.2 AA tag set).
// Ground truth for the allowlist: 1 color-contrast violation, 4 nodes —
// 3 lg buttons (permitted "large CTA" gold per DESIGN.md §2) + 1 default
// badge (NOT a permitted gold surface → must remain a failure).
const fixture = JSON.parse(
  readFileSync(fileURLToPath(new URL("../fixtures/violations.json", import.meta.url)), "utf8"),
);

const REAL_VIOLATIONS = fixture.violations;

const node = (html) => ({ target: ["x"], html, any: [{ id: "color-contrast", data: {} }] });
const violation = (id, nodes) => ({ id, impact: "serious", nodes });

const buttonNodes = REAL_VIOLATIONS[0].nodes.filter((n) => n.html.includes('data-slot="button"'));
const badgeNode = REAL_VIOLATIONS[0].nodes.find((n) => n.html.includes('data-slot="badge"'));

describe("a11y allowlist — discriminator", () => {
  it("allowlists every lg button (matched on html role, not bgColor/target)", () => {
    const remaining = filterAllowed([violation("color-contrast", buttonNodes)]);
    // All three button nodes are permitted → the whole violation drops.
    expect(remaining).toHaveLength(0);
  });

  it("does NOT allowlist the default badge — it shares the buttons' #8e6f38 bg", () => {
    const remaining = filterAllowed([violation("color-contrast", [badgeNode])]);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(1);
    expect(remaining[0].nodes[0].html).toContain('data-slot="badge"');
  });

  it("keeps the badge node and drops the button nodes from the real mixed violation", () => {
    const remaining = filterAllowed(REAL_VIOLATIONS);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(1);
    expect(remaining[0].nodes[0].html).toContain('data-slot="badge"');
  });

  it("anti-bgColor regression: a non-button node on gold is NOT allowlisted", () => {
    // Same gold bg as the buttons, but not a large-CTA element → must remain.
    const goldNonButton = node('<span data-slot="badge" data-variant="default" class="x">');
    const remaining = filterAllowed([violation("color-contrast", [goldNonButton])]);
    expect(remaining).toHaveLength(1);
  });

  it("does not allowlist a new color-contrast node on a non-gold surface", () => {
    const remaining = filterAllowed([violation("color-contrast", [node('<a data-slot="link">')])]);
    expect(remaining).toHaveLength(1);
  });

  it("does not allowlist a different rule even on an lg button", () => {
    const btn = node('<button data-slot="button" data-size="lg" class="x">');
    const remaining = filterAllowed([violation("image-alt", [btn])]);
    expect(remaining).toHaveLength(1);
  });

  it("fails closed on a node with missing/empty html", () => {
    const remaining = filterAllowed([
      violation("color-contrast", [{ target: ["x"], html: "", any: [] }, { target: ["x"] }]),
    ]);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(2);
  });

  it("handles a violation with no nodes", () => {
    const remaining = filterAllowed([violation("color-contrast", [])]);
    // No nodes left → violation drops.
    expect(remaining).toHaveLength(0);
  });

  it("exposes a documented allowlist with a rationale per entry", () => {
    expect(ALLOWLIST.length).toBeGreaterThan(0);
    for (const entry of ALLOWLIST) {
      expect(entry.rationale).toMatch(/DESIGN\.md/);
    }
  });
});

describe("a11y allowlist — cover-spine carve-out", () => {
  it("allowlists a spine text node carrying a text-cover-* class", () => {
    const spineText = node(
      '<div class="font-display text-cover-cream line-clamp-4 text-base font-semibold">',
    );
    const remaining = filterAllowed([violation("color-contrast", [spineText])]);
    expect(remaining).toHaveLength(0);
  });

  it("does NOT allowlist a non-cover color-contrast node", () => {
    const plainText = node('<p class="text-fg-faint">');
    const remaining = filterAllowed([violation("color-contrast", [plainText])]);
    expect(remaining).toHaveLength(1);
  });

  it("does NOT allowlist a text-cover-* node under a different rule id", () => {
    const spineText = node('<div class="text-cover-ink">');
    const remaining = filterAllowed([violation("image-alt", [spineText])]);
    expect(remaining).toHaveLength(1);
  });

  it("keeps the spine carve-out independent of the lg-button carve-out", () => {
    // A mixed violation: one spine node + one non-exempt badge node —
    // only the badge survives.
    const spineText = node('<div class="text-cover-parchment">');
    const remaining = filterAllowed([violation("color-contrast", [spineText, badgeNode])]);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(1);
    expect(remaining[0].nodes[0].html).toContain('data-slot="badge"');
  });
});

describe("a11y verdict — liveness gate", () => {
  it("passes only when the scan ran AND nothing remains", () => {
    expect(verdict({ violations: [], scanOk: true }).pass).toBe(true);
  });

  it("fails when an allowed-only result is reported but the scan did not run", () => {
    // Finding S1: an empty/blank result from a crashed scan or wrong page
    // must NEVER pass — empty must not be mistaken for "0 violations".
    expect(verdict({ violations: [], scanOk: false }).pass).toBe(false);
  });

  it("fails when a real (non-allowlisted) violation remains", () => {
    expect(verdict({ violations: REAL_VIOLATIONS, scanOk: true }).pass).toBe(false);
  });

  it("passes when only allowlisted button nodes are present", () => {
    expect(
      verdict({ violations: [violation("color-contrast", buttonNodes)], scanOk: true }).pass,
    ).toBe(true);
  });

  it("returns the remaining (non-allowlisted) violations", () => {
    const result = verdict({ violations: REAL_VIOLATIONS, scanOk: true });
    expect(result.remaining).toHaveLength(1);
    expect(result.remaining[0].nodes[0].html).toContain('data-slot="badge"');
  });
});

describe("a11y urlMatches — liveness URL check", () => {
  it("matches an exact path", () => {
    expect(urlMatches("/design/system", "/design/system")).toBe(true);
  });

  it("ignores a trailing slash the router may add to the scanned url", () => {
    expect(urlMatches("/design/system/", "/design/system")).toBe(true);
  });

  it("ignores a trailing slash on the target", () => {
    expect(urlMatches("/design/system", "/design/system/")).toBe(true);
  });

  it("rejects a suffix (partial) match", () => {
    expect(urlMatches("/design/system", "/system")).toBe(false);
  });

  it("rejects a different path", () => {
    expect(urlMatches("/design/hero", "/design/system")).toBe(false);
  });

  it("rejects a non-string url", () => {
    expect(urlMatches(undefined, "/design/system")).toBe(false);
  });
});
