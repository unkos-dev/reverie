import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Editorial design-system token-contract guard. Verifies the generated Radix
// primitives meet the spec §6 role-pair contrast floors, the brand anchors land
// verbatim, and every semantic token in index.css resolves to a real primitive
// (no dangling var()). Guards frontend/src/styles/themes/*.css against
// regeneration / hand-edit drift.
// Spec: plans/2026-06-18-editorial-design-system-tokens-design.md §4/§6.
//
// Lives under `vite-plugins/__tests__/` (not co-located with the CSS) for the
// same reason as no-shadcn-accent.guard.test.ts: it reads files from disk, so it
// needs node env + node types, and the app tsconfig is browser-only — placing it
// under `src/` breaks `tsc -b` (and the jsdom project stubs CSS imports empty).
const THEMES_DIR = resolve(__dirname, "..", "..", "src", "styles", "themes");
const primitivesCss = readFileSync(resolve(THEMES_DIR, "primitives.generated.css"), "utf8");
const indexCss = readFileSync(resolve(THEMES_DIR, "index.css"), "utf8");

// --- WCAG 2.1 relative-luminance contrast (pure; no color lib) ---
function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}
function luminance(hex: string): number {
  let h = hex.replace("#", "");
  if (h.length === 3) h = [...h].map((c) => c + c).join(""); // expand #abc → #aabbcc
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// --- parse a flat custom-property block following a selector header ---
function parseBlock(css: string, selectorMarker: string): Record<string, string> {
  const start = css.indexOf(selectorMarker);
  if (start === -1) throw new Error(`selector not found: ${selectorMarker}`);
  const open = css.indexOf("{", start);
  const close = css.indexOf("}", open);
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
      it(`${theme.name}: ${label} >= ${min}:1`, () => {
        expect(theme.m[fg], `missing --${fg}`).toBeDefined();
        expect(theme.m[bg], `missing --${bg}`).toBeDefined();
        expect(contrast(theme.m[fg], theme.m[bg])).toBeGreaterThanOrEqual(min);
      });
    }
  }
});

describe("brand anchors land verbatim", () => {
  it("gold-9 dark == #c9a961", () => expect(dark["gold-9"].toLowerCase()).toBe("#c9a961"));
  it("danger-9 == #b91c1c both themes", () => {
    expect(dark["danger-9"].toLowerCase()).toBe("#b91c1c");
    expect(light["danger-9"].toLowerCase()).toBe("#b91c1c");
  });
  it("gold-contrast is ink, not white (spec §4a)", () =>
    expect(dark["gold-contrast"].toLowerCase()).toBe("#0e0d0a"));
});

describe("semantic tokens reference existing primitives", () => {
  // every `--x: var(--y)` semantic definition whose target is a primitive
  // family must resolve to a key in the parsed primitive map. The @theme inline
  // `--color-*` alias layer is excluded — it references semantic tokens (e.g.
  // `--color-danger: var(--danger)`), not primitives.
  const refs = [
    ...indexCss.matchAll(/--(?!color-)[\w-]+:\s*var\(--((?:sand|gold|danger|bg)[\w-]*)\)/g),
  ].map((m) => m[1]);
  it("finds semantic→primitive references", () => expect(refs.length).toBeGreaterThan(5));
  for (const target of refs) {
    it(`--${target} exists in primitives`, () => {
      expect(target === "bg" || target in dark, `dangling var(--${target})`).toBeTruthy();
    });
  }
});

describe("danger + destructive wiring", () => {
  it("--danger semantic maps to danger-9", () =>
    expect(indexCss).toMatch(/--danger:\s*var\(--danger-9\)/));
  it("shadcn --color-destructive routes to --danger (not --fg)", () => {
    expect(indexCss).toMatch(/--color-destructive:\s*var\(--danger\)/);
    expect(indexCss).not.toMatch(/--color-destructive:\s*var\(--fg\)/);
  });
});
