import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

// UNK-114 issue 4 regression guard. Brand `--color-accent` (Reverie Gold) is
// the signature for primary affordances and must never appear as a shadcn
// primitive's hover/focus treatment — the dropped `--color-accent-foreground`
// alias means stock shadcn primitives that ship with `focus:bg-accent
// focus:text-accent-foreground` would render with broken text colour. A
// future `npx shadcn@latest add <component>` must rewrite those utilities to
// `focus:bg-hover focus:text-fg` (matching dropdown-menu.tsx and select.tsx)
// before merging. This guard fails the build if a primitive reintroduces the
// pattern.
//
// Lives under `vite-plugins/__tests__/` (not co-located with the primitives)
// because the scan needs node env + node types and the app's tsconfig is
// browser-only — placing it under `src/` broke `tsc -b` even though vitest
// itself ran fine.
const UI_DIR = resolve(__dirname, "..", "..", "src", "components", "ui");

describe("shadcn primitives must not use brand bg-accent for hover/focus", () => {
  const files = readdirSync(UI_DIR)
    .filter((f: string) => f.endsWith(".tsx"))
    .map((f: string) => join(UI_DIR, f));

  it.each(files.map((f: string) => [f]))("%s does not use bg-accent or text-accent-foreground", (file: string) => {
    const src = readFileSync(file, "utf8");
    // Match Tailwind classnames literally; we want any occurrence inside
    // className strings to fail. The patterns are deliberately broad — any
    // future shadcn primitive that ships with these utilities must be
    // rewritten to bg-hover/text-fg before landing.
    const forbidden = [
      /(?<![\w-])bg-accent(?![\w-])/,
      /(?<![\w-])text-accent-foreground(?![\w-])/,
      /focus:bg-accent(?![\w-])/,
      /focus:text-accent-foreground(?![\w-])/,
      /data-open:bg-accent(?![\w-])/,
      /data-open:text-accent-foreground(?![\w-])/,
    ];
    for (const pat of forbidden) {
      expect(src, `${file} matched ${pat} — rewrite to bg-hover/text-fg per UNK-114 issue 4`).not.toMatch(pat);
    }
  });

  it("scans at least one file (catches a misconfigured glob)", () => {
    expect(files.length).toBeGreaterThan(10);
  });
});
