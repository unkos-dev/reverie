import { readFileSync } from "node:fs";

import { describe, expect, it } from "vite-plus/test";

import { ARTIFACT_PATH, buildPrimitivesCss } from "../emit-primitives.ts";

describe("primitives.generated.css drift gate", () => {
  it("matches the committed artifact byte for byte", () => {
    expect(buildPrimitivesCss()).toBe(readFileSync(ARTIFACT_PATH, "utf8"));
  });

  it("is deterministic across invocations", () => {
    expect(buildPrimitivesCss()).toBe(buildPrimitivesCss());
  });

  it("applies the gold-contrast ink override in both themes", () => {
    const css = buildPrimitivesCss();
    expect(css.match(/--gold-contrast: #0e0d0a;/g)).toHaveLength(2);
    expect(css).not.toContain("--gold-contrast: #fff");
  });

  it("keeps the generator's danger contrast unoverridden", () => {
    const css = buildPrimitivesCss();
    expect(css.match(/--danger-contrast: #fff;/g)).toHaveLength(2);
  });
});
