import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { compile } from "tailwindcss";
import { describe, expect, test } from "vite-plus/test";

const THEME_CSS = readFileSync(
  fileURLToPath(new URL("../../../src/styles/themes/index.css", import.meta.url)),
  "utf8",
);

async function build(candidates: string[]): Promise<string> {
  const variants = THEME_CSS.split("\n").filter((line) => line.startsWith("@custom-variant data-"));
  const compiler = await compile(
    `@theme inline { --color-primary: red; --color-input: blue; }\n${variants.join("\n")}\n@tailwind utilities;`,
  );
  return compiler.build(candidates);
}

describe("Radix state variants", () => {
  test("data-checked and data-unchecked match the data-state Radix reports", async () => {
    const css = await build(["data-checked:bg-primary", "data-unchecked:bg-input"]);
    expect(css).toContain('[data-state="checked"]');
    expect(css).toContain('[data-state="unchecked"]');
    expect(css).not.toContain("[data-checked]");
    expect(css).not.toContain("[data-unchecked]");
  });

  test("the variants still compose with the group and dark variants the primitives use", async () => {
    const css = await build(["group-data-[size=default]/switch:data-checked:bg-primary"]);
    expect(css).toContain('[data-state="checked"]');
  });

  test("the theme declares both variants, so the test is not matching nothing", () => {
    const declared = THEME_CSS.split("\n").filter((line) =>
      line.startsWith("@custom-variant data-"),
    );
    expect(declared).toHaveLength(2);
  });
});
