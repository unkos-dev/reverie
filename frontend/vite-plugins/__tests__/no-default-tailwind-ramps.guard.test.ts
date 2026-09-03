import { describe, expect, it } from "vite-plus/test";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { compile } from "tailwindcss";

// identity.md §4 requires Tailwind's default color ramps to be absent, not
// merely unused. A source grep only proves current non-use, so this compiles
// a fixture through the real Tailwind v4 Node API against this repository's
// actual theme entry and asserts stock-ramp candidates produce no utility
// rule. `loadStylesheet` is hand-written because `@tailwindcss/node` is not a
// declared dependency and pnpm's strict resolution would refuse an undeclared
// import.
//
// Lives under `vite-plugins/__tests__/` (not co-located with the CSS) for the
// same reason as tokens.test.ts: it reads files from disk and needs node
// module resolution, and the app tsconfig is browser-only.
const SRC_DIR = resolve(__dirname, "..", "..", "src");
const require = createRequire(import.meta.url);
const entryPath = resolve(SRC_DIR, "index.css");

function loadStylesheet(
  id: string,
  base: string,
): Promise<{ path: string; base: string; content: string }> {
  const path = id === "tailwindcss" ? require.resolve("tailwindcss/index.css") : resolve(base, id);
  return Promise.resolve({ path, base: dirname(path), content: readFileSync(path, "utf8") });
}

async function compileEntry(): Promise<string> {
  const entryCss = readFileSync(entryPath, "utf8");
  const result = await compile(entryCss, { base: dirname(entryPath), loadStylesheet });
  const forbidden = [
    "bg-red-500",
    "text-blue-600",
    "border-green-300",
    "fill-sky-400",
    "ring-rose-200",
    "bg-white",
    "text-black",
  ];
  const control = ["bg-canvas", "bg-background"];
  return result.build([...forbidden, ...control]);
}

describe("default Tailwind color ramps are reset to absent", () => {
  it("produces no rule for any stock-ramp candidate", async () => {
    const css = await compileEntry();
    for (const candidate of [
      "bg-red-500",
      "text-blue-600",
      "border-green-300",
      "fill-sky-400",
      "ring-rose-200",
      "bg-white",
      "text-black",
    ]) {
      expect(css, `${candidate} produced a rule`).not.toContain(`.${candidate}`);
    }
  });

  it("still compiles a brand token and a shadcn alias (control, catches a vacuous pass)", async () => {
    const css = await compileEntry();
    expect(css).toContain(".bg-canvas");
    expect(css).toContain(".bg-background");
  });
});
