import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vite-plus/test";

// Exercises scripts/assert-no-design-chunk.mjs (the build gate) end to end
// against fixture dist/assets trees, so a wrong readdir or a fail-open bug is
// caught here rather than silently passing a production build.
const SCRIPT = resolve(__dirname, "..", "..", "scripts", "assert-no-design-chunk.mjs");

interface RunResult {
  code: number;
  stderr: string;
}

function runIn(dir: string): RunResult {
  try {
    execFileSync("node", [SCRIPT], { cwd: dir, encoding: "utf8" });
    return { code: 0, stderr: "" };
  } catch (err) {
    const e = err as { status?: number; stderr?: string };
    return { code: e.status ?? 1, stderr: e.stderr ?? "" };
  }
}

function fixture(root: string, name: string, assets: string[]): string {
  const dir = join(root, name);
  mkdirSync(join(dir, "dist", "assets"), { recursive: true });
  for (const asset of assets) {
    writeFileSync(join(dir, "dist", "assets", asset), "");
  }
  return dir;
}

describe("assert-no-design-chunk build gate", () => {
  let tmp: string;
  beforeAll(() => {
    tmp = mkdtempSync(join(tmpdir(), "design-chunk-"));
  });
  afterAll(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it("exits 0 when dist/assets has no design chunk", () => {
    expect(runIn(fixture(tmp, "clean", ["index-abc.js", "library-xyz.js"])).code).toBe(0);
  });

  it("exits 1 and names the offender when a design chunk leaked", () => {
    const r = runIn(fixture(tmp, "leaked", ["index-abc.js", "design-9f3.js"]));
    expect(r.code).toBe(1);
    expect(r.stderr).toContain("design-9f3.js");
  });

  it("anchors on the design- prefix (designsystem- is not an offender)", () => {
    expect(runIn(fixture(tmp, "anchor", ["designsystem-abc.js"])).code).toBe(0);
  });

  it("exits 2 (loud) when dist/assets is missing", () => {
    const dir = join(tmp, "nobuild");
    mkdirSync(dir, { recursive: true });
    expect(runIn(dir).code).toBe(2);
  });
});
