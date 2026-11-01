import { readFileSync } from "node:fs";

import { describe, expect, it } from "vite-plus/test";

import { ARTIFACT_PATH, buildPrimitivesCss } from "../emit-primitives.ts";

describe("primitives.generated.css drift gate", () => {
  it("matches the committed artifact byte for byte", () => {
    expect(buildPrimitivesCss()).toBe(readFileSync(ARTIFACT_PATH, "utf8"));
  });
});
