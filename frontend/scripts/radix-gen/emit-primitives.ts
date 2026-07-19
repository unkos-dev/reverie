/*
 * Emits src/styles/themes/primitives.generated.css from the vendored Radix
 * color engine plus the Reverie brand anchors. Run with `npm run
 * primitives:gen`; a drift test pins the committed artifact to this output,
 * so the file and this emitter change together, never independently.
 *
 * The one deliberate divergence from raw generator output: --gold-contrast is
 * forced to ink #0e0d0a because the generator returns #fff, which is
 * unreadable on the pale dark-theme gold-9 #c9a961 (white = 2.25:1).
 */
import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { generateRadixColors } from "./generate-radix-colors.ts";

const ANCHORS = {
  bgDark: "#0E0D0A",
  bgLight: "#E8DCC2",
  gold: "#C9A961",
  danger: "#B91C1C",
  graySeed: "#8A8170",
};

const GOLD_CONTRAST_OVERRIDE = "#0e0d0a";
const SAND_CONTRAST = { light: "#0e0d0a", dark: "#f0eee9" };

const dark = (accent: string) =>
  generateRadixColors({
    appearance: "dark",
    accent,
    gray: ANCHORS.graySeed,
    background: ANCHORS.bgDark,
  });
const light = (accent: string) =>
  generateRadixColors({
    appearance: "light",
    accent,
    gray: ANCHORS.graySeed,
    background: ANCHORS.bgLight,
  });

function block(
  prefix: string,
  scaleHex: string[],
  scaleAlpha: string[],
  surface: string,
  contrast: string,
) {
  const lines: string[] = [];
  scaleHex.forEach((h, i) => lines.push(`  --${prefix}-${String(i + 1)}: ${h};`));
  scaleAlpha.forEach((h, i) => lines.push(`  --${prefix}-a${String(i + 1)}: ${h};`));
  lines.push(`  --${prefix}-contrast: ${contrast};`);
  lines.push(`  --${prefix}-surface: ${surface};`);
  return lines.join("\n");
}

function p3block(prefix: string, wide: string[], wideAlpha: string[]) {
  const lines: string[] = [];
  wide.forEach((h, i) => lines.push(`      --${prefix}-${String(i + 1)}: ${h};`));
  wideAlpha.forEach((h, i) => lines.push(`      --${prefix}-a${String(i + 1)}: ${h};`));
  return lines.join("\n");
}

const HEADER = `/*
 * Reverie editorial design-system — Radix-generated color primitives.
 * GENERATED ARTIFACT — do not hand-edit. Regenerate: npm run primitives:gen.
 *
 * Source fn:  scripts/radix-gen/generate-radix-colors.ts (radix-ui/website @ 88a9f14)
 * Deps:       @radix-ui/colors@3.0.0  colorjs.io@0.6.1  bezier-easing@3.0.0
 * Anchors:    bg-dark #0E0D0A · bg-light(parchment) #E8DCC2
 *             accent(gold) #C9A961 · danger #B91C1C · neutral-seed #8A8170
 * OVERRIDE:   --gold-contrast forced to ink #0e0d0a (generator returned #fff,
 *             unreadable on the pale dark-theme gold-9 #c9a961: white = 2.25:1).
 * Families:   --sand-* (warm neutral) · --gold-* (accent) · --danger-* (destructive only)
 */
`;

export function buildPrimitivesCss(): string {
  const gD = dark(ANCHORS.gold);
  const gL = light(ANCHORS.gold);
  const dD = dark(ANCHORS.danger);
  const dL = light(ANCHORS.danger);

  const lightCss = [
    `/* ---- LIGHT (default) ---- */`,
    `:root,\n[data-theme="light"] {`,
    `  /* page background (parchment) */`,
    `  --bg: ${gL.background};`,
    block("sand", gL.grayScale, gL.grayScaleAlpha, gL.graySurface, SAND_CONTRAST.light),
    block("gold", gL.accentScale, gL.accentScaleAlpha, gL.accentSurface, GOLD_CONTRAST_OVERRIDE),
    block("danger", dL.accentScale, dL.accentScaleAlpha, dL.accentSurface, dL.accentContrast),
    `}`,
  ].join("\n");

  const darkCss = [
    `/* ---- DARK ---- */`,
    `[data-theme="dark"] {`,
    `  --bg: ${gD.background};`,
    block("sand", gD.grayScale, gD.grayScaleAlpha, gD.graySurface, SAND_CONTRAST.dark),
    block("gold", gD.accentScale, gD.accentScaleAlpha, gD.accentSurface, GOLD_CONTRAST_OVERRIDE),
    block("danger", dD.accentScale, dD.accentScaleAlpha, dD.accentSurface, dD.accentContrast),
    `}`,
  ].join("\n");

  const p3 = [
    `/* ---- P3 wide-gamut (progressive enhancement; sRGB above is the floor) ---- */`,
    `@supports (color: color(display-p3 1 1 1)) {`,
    `  @media (color-gamut: p3) {`,
    `    :root,\n    [data-theme="light"] {`,
    p3block("sand", gL.grayScaleWideGamut, gL.grayScaleAlphaWideGamut),
    p3block("gold", gL.accentScaleWideGamut, gL.accentScaleAlphaWideGamut),
    p3block("danger", dL.accentScaleWideGamut, dL.accentScaleAlphaWideGamut),
    `    }`,
    `    [data-theme="dark"] {`,
    p3block("sand", gD.grayScaleWideGamut, gD.grayScaleAlphaWideGamut),
    p3block("gold", gD.accentScaleWideGamut, gD.accentScaleAlphaWideGamut),
    p3block("danger", dD.accentScaleWideGamut, dD.accentScaleAlphaWideGamut),
    `    }`,
    `  }`,
    `}`,
  ].join("\n");

  return `${HEADER}\n${lightCss}\n\n${darkCss}\n\n${p3}\n`;
}

export const ARTIFACT_PATH = join(
  import.meta.dirname,
  "../../src/styles/themes/primitives.generated.css",
);

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  writeFileSync(ARTIFACT_PATH, buildPrimitivesCss());
  console.log(`wrote ${ARTIFACT_PATH}`);
}
