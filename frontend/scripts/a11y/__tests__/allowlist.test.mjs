import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vite-plus/test";

import { ALLOWLIST, filterAllowed, parseTargets, urlMatches, verdict } from "../allowlist.mjs";

// A realistic multi-node axe payload captured with the full WCAG 2.2 AA tag
// set: 1 color-contrast violation over 4 nodes, 3 lg buttons plus 1 default
// badge.
//
// No live surface produces these violations. `--fg-on-accent` resolves to
// `gold-contrast` (Ink), so those components render ink-on-gold and pass. The
// fixture is retained anyway because it pins the discriminator's behaviour on
// gold surfaces, so a regression that reintroduces low-contrast gold is caught
// as a failure rather than silently exempted.
const fixture = JSON.parse(
  readFileSync(fileURLToPath(new URL("../fixtures/violations.json", import.meta.url)), "utf8"),
);

const REAL_VIOLATIONS = fixture.violations;

const node = (html) => ({ target: ["x"], html, any: [{ id: "color-contrast", data: {} }] });
const violation = (id, nodes) => ({ id, impact: "serious", nodes });

const buttonNodes = REAL_VIOLATIONS[0].nodes.filter((n) => n.html.includes('data-slot="button"'));
const badgeNode = REAL_VIOLATIONS[0].nodes.find((n) => n.html.includes('data-slot="badge"'));

describe("a11y allowlist — discriminator", () => {
  it("does NOT exempt lg buttons: low-contrast gold is a defect, not a carve-out", () => {
    const remaining = filterAllowed([violation("color-contrast", buttonNodes)]);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(buttonNodes.length);
  });

  it("does NOT allowlist the default badge", () => {
    const remaining = filterAllowed([violation("color-contrast", [badgeNode])]);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(1);
    expect(remaining[0].nodes[0].html).toContain('data-slot="badge"');
  });

  it("keeps every node of the real mixed violation", () => {
    const remaining = filterAllowed(REAL_VIOLATIONS);
    expect(remaining).toHaveLength(1);
    expect(remaining[0].nodes).toHaveLength(REAL_VIOLATIONS[0].nodes.length);
  });

  it("anti-bgColor regression: no node is exempted for sitting on gold", () => {
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

  it("drops only the spine node from a mixed violation", () => {
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

  it("fails when only button nodes are present — they carry no exemption", () => {
    expect(
      verdict({ violations: [violation("color-contrast", buttonNodes)], scanOk: true }).pass,
    ).toBe(false);
  });

  it("passes when only allowlisted spine nodes are present", () => {
    const spineText = node('<div class="text-cover-cream">');
    expect(
      verdict({ violations: [violation("color-contrast", [spineText])], scanOk: true }).pass,
    ).toBe(true);
  });

  it("returns the remaining (non-allowlisted) violations", () => {
    const result = verdict({ violations: REAL_VIOLATIONS, scanOk: true });
    expect(result.remaining).toHaveLength(1);
    expect(result.remaining[0].nodes).toHaveLength(REAL_VIOLATIONS[0].nodes.length);
  });
});

describe("a11y parseTargets — env contract + fail-closed target parsing", () => {
  it("defaults to a route that ships, so the gate cannot certify unreachable markup", () => {
    expect(parseTargets(undefined)).toEqual(["/forgot-password"]);
  });

  it("splits comma-separated paths, trims whitespace, drops empties", () => {
    expect(parseTargets("/design/system, /design/hero ,")).toEqual([
      "/design/system",
      "/design/hero",
    ]);
  });

  it("throws when the var is set but empty (would register zero tests)", () => {
    expect(() => parseTargets("")).toThrow(/zero targets/);
  });

  it("throws when the var is all whitespace/commas (would register zero tests)", () => {
    expect(() => parseTargets("  , ,  ")).toThrow(/zero targets/);
  });

  it("throws on an absolute http URL (escapes the configured baseURL)", () => {
    expect(() => parseTargets("http://example.com/design/system")).toThrow(/root-relative/);
  });

  it("throws on an absolute https URL", () => {
    expect(() => parseTargets("https://evil.test/")).toThrow(/root-relative/);
  });

  it("throws on a protocol-relative target (//host escapes the origin)", () => {
    expect(() => parseTargets("//evil.test/design")).toThrow(/root-relative/);
  });

  it("throws on a non-root path missing the leading slash", () => {
    expect(() => parseTargets("design/system")).toThrow(/root-relative/);
  });

  it("rejects the whole list when any one target is off-origin", () => {
    expect(() => parseTargets("/design/system,http://evil.test")).toThrow(/root-relative/);
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
