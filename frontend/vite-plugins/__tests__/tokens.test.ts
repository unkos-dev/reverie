import { describe, expect, it } from "vite-plus/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Editorial design-system token-contract guard. Verifies the generated Radix
// primitives meet the role-pair contrast floors, the brand anchors land
// verbatim, and every semantic token in index.css resolves to a real primitive
// (no dangling var()). Guards frontend/src/styles/themes/*.css against
// regeneration / hand-edit drift.
// Contract: docs/design/color-tokens.md.
//
// Lives under `vite-plugins/__tests__/` (not co-located with the CSS) for the
// same reason as no-shadcn-accent.guard.test.ts: it reads files from disk, so it
// needs node env + node types, and the app tsconfig is browser-only — placing it
// under `src/` breaks `tsc -b` (and the jsdom project stubs CSS imports empty).
const THEMES_DIR = resolve(__dirname, "..", "..", "src", "styles", "themes");
const primitivesCss = readFileSync(resolve(THEMES_DIR, "primitives.generated.css"), "utf8");
const indexCss = readFileSync(resolve(THEMES_DIR, "index.css"), "utf8");
const BODY_RULE = /^body\s*\{[^}]*\}/m;

function bodyRule(css: string): string {
  const match = css.match(BODY_RULE);
  if (!match) throw new Error("selector not found: body");
  return match[0];
}

describe("global body typography", () => {
  it("does not mistake a class suffix for the body selector", () => {
    const css = ".card-body { font-family: var(--font-body); }\nbody { font-family: inherit; }";

    expect(bodyRule(css)).toBe("body { font-family: inherit; }");
  });

  it("applies the canonical Satoshi body token to document text", () => {
    expect(bodyRule(indexCss)).toMatch(/font-family:\s*var\(--font-body\)/);
  });

  it("does not duplicate a fallback stack outside the canonical body token", () => {
    expect(bodyRule(indexCss)).not.toMatch(/system-ui|Satoshi Variable/);
  });
});

// --- WCAG 2.1 relative-luminance contrast (pure; no color lib) ---
function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}
function luminance(hex: string): number {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.replace(/(.)/g, "$1$1"); // expand #abc → #aabbcc
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}
function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  const hi = Math.max(la, lb);
  const lo = Math.min(la, lb);
  return (hi + 0.05) / (lo + 0.05);
}

// --- parse a flat custom-property block following a selector header ---
function parseBlock(css: string, selectorMarker: string): Record<string, string> {
  const start = css.indexOf(selectorMarker);
  if (start === -1) throw new Error(`selector not found: ${selectorMarker}`);
  const open = css.indexOf("{", start);
  // Walk forward tracking brace depth so a future generated format with a nested
  // at-rule or a comment containing `}` inside the block can't truncate the parse
  // early (the current flat sRGB blocks have no nested braces, so this is a no-op
  // today — it future-proofs against regeneration drift).
  let depth = 1;
  let pos = open + 1;
  while (pos < css.length && depth > 0) {
    if (css[pos] === "{") depth++;
    else if (css[pos] === "}") depth--;
    pos++;
  }
  const close = pos - 1;
  const body = css.slice(open + 1, close);
  const map: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{3,8})\s*;/g)) {
    map[m[1]] = m[2];
  }
  return map;
}

const light = parseBlock(primitivesCss, ":root,");
const dark = parseBlock(primitivesCss, '[data-theme="dark"]');

// role pairs from spec §6 (AA-normal text 4.5; body ideal 7)
const pairs: ReadonlyArray<[label: string, fg: string, bg: string, min: number]> = [
  ["body sand-12 on canvas", "sand-12", "bg", 7],
  ["muted sand-11 on canvas", "sand-11", "bg", 4.5],
  ["gold-11 text on canvas", "gold-11", "bg", 4.5],
  ["ink on gold-9 button", "gold-contrast", "gold-9", 4.5],
  ["danger-11 text on canvas", "danger-11", "bg", 4.5],
  ["white on danger-9 button", "danger-contrast", "danger-9", 4.5],
];

describe("primitive role-pair contrast (spec §6)", () => {
  for (const theme of [
    { name: "dark", m: dark },
    { name: "light", m: light },
  ] as const) {
    for (const [label, fg, bg, min] of pairs) {
      it(`${theme.name}: ${label} >= ${String(min)}:1`, () => {
        expect(theme.m[fg], `missing --${fg}`).toBeDefined();
        expect(theme.m[bg], `missing --${bg}`).toBeDefined();
        expect(contrast(theme.m[fg], theme.m[bg])).toBeGreaterThanOrEqual(min);
      });
    }
  }
});

/* Brand-anchor assertions intentionally compare against literal hex
   (#c9a961 gold, #b91c1c danger, #0e0d0a ink): verifying the generated
   primitives match the locked anchors is this block's purpose. */
describe("brand anchors land verbatim", () => {
  it("gold-9 dark == #c9a961", () => {
    expect(dark["gold-9"].toLowerCase()).toBe("#c9a961");
  });
  it("danger-9 == #b91c1c both themes", () => {
    expect(dark["danger-9"].toLowerCase()).toBe("#b91c1c");
    expect(light["danger-9"].toLowerCase()).toBe("#b91c1c");
  });
  it("gold-contrast is ink, not white, in both themes (spec §4a)", () => {
    expect(dark["gold-contrast"].toLowerCase()).toBe("#0e0d0a");
    expect(light["gold-contrast"].toLowerCase()).toBe("#0e0d0a");
  });
});

describe("semantic tokens reference existing primitives", () => {
  // every `--x: var(--y)` semantic definition whose target is a primitive
  // family must resolve to a key in the parsed primitive map. The @theme inline
  // `--color-*` alias layer is excluded — it references semantic tokens (e.g.
  // `--color-danger: var(--danger)`), not primitives.
  const refs = [
    ...indexCss.matchAll(/--(?!color-)[\w-]+:\s*var\(--((?:sand|gold|danger|bg)[\w-]*)\)/g),
  ].map((m) => m[1]);
  it("finds semantic→primitive references", () => {
    expect(refs.length).toBeGreaterThan(5);
  });
  for (const target of refs) {
    it(`--${target} exists in primitives`, () => {
      expect(target === "bg" || target in dark, `dangling var(--${target})`).toBeTruthy();
    });
  }
});

describe("danger + destructive wiring", () => {
  it("--danger semantic maps to danger-9", () => {
    expect(indexCss).toMatch(/--danger:\s*var\(--danger-9\)/);
  });
  it("shadcn --color-destructive routes to --danger (not --fg)", () => {
    expect(indexCss).toMatch(/--color-destructive:\s*var\(--danger\)/);
    expect(indexCss).not.toMatch(/--color-destructive:\s*var\(--fg\)/);
  });
});

describe("focus indicator WCAG 1.4.11 contract (spec §5b)", () => {
  // One 2px gold-11 ring, no halo: --accent-text (gold-11) carries the >=3:1
  // non-text boundary against the page on its own, so the sand-12 box-shadow
  // that padded gold-9's failing 2.80:1 is gone. These guard the numeric claim
  // in the index.css :focus-visible comment against primitive regeneration.
  for (const theme of [
    { name: "dark", m: dark },
    { name: "light", m: light },
  ] as const) {
    it(`${theme.name}: gold-11 focus ring on canvas >= 3:1 (non-text floor)`, () => {
      expect(contrast(theme.m["gold-11"], theme.m["bg"])).toBeGreaterThanOrEqual(3);
    });
  }
  // Set a floor under the published ring figures (the index.css comment): if a
  // primitive regeneration drops gold-11 below the stated ~7:1 / ~10:1, the test
  // fails loudly and the prose must be updated. The floor is a minimum, not an
  // exact pin — upward drift is benign (a regen that *raises* contrast still
  // satisfies the published figure) and intentionally does not fail.
  it("light ring (gold-11 on parchment) clears the published ~7:1", () => {
    expect(contrast(light["gold-11"], light["bg"])).toBeGreaterThanOrEqual(7);
  });
  it("dark ring (gold-11 on ink) clears the published ~10:1", () => {
    expect(contrast(dark["gold-11"], dark["bg"])).toBeGreaterThanOrEqual(10);
  });
  // Guard the semantic chain: the ring repoints to --accent-text (gold-11), and
  // the halo token plus its box-shadow bloom are gone for good.
  it("--focus-ring maps to --accent-text", () => {
    expect(indexCss).toMatch(/--focus-ring:\s*var\(--accent-text\)/);
  });
  it("--focus-halo is removed", () => {
    expect(indexCss).not.toMatch(/--focus-halo/);
  });
  it(":focus-visible pins box-shadow to none (no halo, suppresses component rings)", () => {
    const rule = indexCss.match(/:focus-visible\s*\{[^}]*\}/);
    expect(rule).not.toBeNull();
    expect(rule?.[0]).toMatch(/box-shadow:\s*none/);
  });
});

describe("accent-glow token (cover-lift shadow)", () => {
  // The toned library cover-lift glow lives behind a token so the arbitrary
  // `shadow-[…var(--accent-glow)]` utility compiles — Tailwind v4 drops an
  // inline color-mix() during shadow-color extraction. Guard the token and its
  // reference to --accent so a rename/repoint of the accent can't silently
  // leave the glow empty (it would render no shadow, not throw).
  it("--accent-glow is defined", () => {
    expect(indexCss).toMatch(/--accent-glow:/);
  });
  it("--accent-glow mixes from --accent via color-mix", () => {
    expect(indexCss).toMatch(/--accent-glow:\s*color-mix\([^;]*var\(--accent\)/);
  });
});

// Resolve a semantic token in index.css to the primitive family member it
// references (e.g. --fg -> sand-12, --canvas -> bg). Throws if it does not
// resolve directly to a primitive — a removed/renamed mapping fails loudly.
function resolveSemantic(token: string): string {
  const m = indexCss.match(
    new RegExp(`--${token}:\\s*var\\(--((?:sand|gold|danger|bg)[\\w-]*)\\)`),
  );
  if (!m) throw new Error(`--${token} does not resolve to a primitive`);
  return m[1];
}

// Role pairs resolved THROUGH the index.css semantic mapping (not by primitive
// name): a bad retarget of a semantic token — e.g. --fg-on-accent flipped off
// --gold-contrast — fails here even though the by-name primitive pair stays green.
const resolvedPairs: ReadonlyArray<[label: string, fg: string, bg: string, min: number]> = [
  ["--fg on --canvas", "fg", "canvas", 7],
  ["--fg-muted on --canvas", "fg-muted", "canvas", 4.5],
  ["--accent-text on --canvas", "accent-text", "canvas", 4.5],
  ["--danger-text on --canvas", "danger-text", "canvas", 4.5],
  ["--fg-on-accent on --accent", "fg-on-accent", "accent", 4.5],
  ["--fg-on-danger on --danger", "fg-on-danger", "danger", 4.5],
];

describe("semantic roles resolve to AA-adequate primitives (spec §6)", () => {
  for (const theme of [
    { name: "dark", m: dark },
    { name: "light", m: light },
  ] as const) {
    for (const [label, fg, bg, min] of resolvedPairs) {
      it(`${theme.name}: ${label} >= ${String(min)}:1`, () => {
        expect(
          contrast(theme.m[resolveSemantic(fg)], theme.m[resolveSemantic(bg)]),
        ).toBeGreaterThanOrEqual(min);
      });
    }
  }
});
