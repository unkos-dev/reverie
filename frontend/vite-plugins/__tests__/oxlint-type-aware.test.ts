import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vite-plus/test";

/**
 * Guards that the type-aware lint rule class is actually firing under `vp lint`.
 *
 * The `strictTypeChecked` rules (`no-floating-promises`, the `no-unsafe-*`
 * family) run through tsgolint, which vp drives from the lint config in
 * `vite.config.ts`. If tsgolint is ever absent or drifts against the bundled
 * oxlint, `vp lint` can still exit zero while those rules silently stop firing.
 * The native rules cannot regress this way: their loss is a diff-visible edit to
 * the lint config, whereas a missing type-aware engine is invisible.
 *
 * A floating promise is the canonical type-aware violation: detecting it
 * requires type information, so a flagged fixture proves the engine ran.
 *
 * The fixtures live under `src` so tsgolint resolves `tsconfig.app.json` and
 * sees the types the rule needs. They are written and removed inside the test;
 * the probe directory must not be git- or lint-ignored.
 */

const PROBE_DIR = join(process.cwd(), "src", "__tests__", "__type-aware-probe__");

interface LintResult {
  code: number;
  output: string;
}

function lint(file: string): LintResult {
  try {
    const stdout = execFileSync("vp", ["lint", file], {
      cwd: process.cwd(),
      encoding: "utf8",
    });
    return { code: 0, output: stdout };
  } catch (err) {
    const e = err as { status?: number; stdout?: string; stderr?: string };
    return {
      code: e.status ?? 1,
      output: `${e.stdout ?? ""}${e.stderr ?? ""}`,
    };
  }
}

function writeFixture(name: string, source: string): string {
  const file = join(PROBE_DIR, name);
  writeFileSync(file, source);
  return file;
}

describe("type-aware lint enforcement (vp lint)", () => {
  beforeAll(() => {
    rmSync(PROBE_DIR, { recursive: true, force: true });
    mkdirSync(PROBE_DIR, { recursive: true });
  });

  afterAll(() => {
    rmSync(PROBE_DIR, { recursive: true, force: true });
  });

  it("vp is resolvable", () => {
    expect(() => execFileSync("vp", ["--version"], { encoding: "utf8" })).not.toThrow();
  });

  it("flags a floating promise (tsgolint present and active)", () => {
    const file = writeFixture("floating.ts", "export async function p() {}\np();\n");
    const { code, output } = lint(file);
    expect(output).toContain("no-floating-promises");
    expect(code).not.toBe(0);
  });

  it("passes a correctly awaited promise", () => {
    const file = writeFixture(
      "awaited.ts",
      "export async function p() {}\nexport async function run() {\n  await p();\n}\n",
    );
    const { code, output } = lint(file);
    expect(output).not.toContain("no-floating-promises");
    expect(code).toBe(0);
  });
});
