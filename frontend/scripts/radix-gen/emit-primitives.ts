import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { generateRadixColors } from "./generate-radix-colors.ts";

const ANCHORS = {
  dark: {
    background: "#0a0d16",
    shell: "#141d45",
    text: "#eef1fa",
    accent: "#8fa8ff",
    contrast: "#070b1a",
    hairline: "rgb(160 180 255 / 13%)",
  },
  light: {
    background: "#fcfcff",
    shell: "#d9e0fb",
    text: "#121831",
    accent: "#3d55d8",
    contrast: "#ffffff",
    hairline: "rgb(30 45 120 / 13%)",
  },
  monochrome: {
    background: "#0c0c0e",
    shell: "#1a1a1d",
    text: "#f3f3f5",
    accent: "#f3f3f5",
    contrast: "#0c0c0e",
    hairline: "rgb(243 243 245 / 13%)",
  },
  danger: "#b91c1c",
  dangerDark: "#ff8a93",
};
const palette = (appearance: "dark" | "light", accent: string, anchors = ANCHORS[appearance]) =>
  generateRadixColors({
    appearance,
    accent,
    gray: anchors.text,
    background: anchors.background,
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

const withStep = (scale: string[], step: number, value: string) =>
  scale.map((v, i) => (i === step - 1 ? value : v));

const HEADER = `/* Generated colour primitives. Regenerate: vp run primitives:gen. */\n`;

export function buildPrimitivesCss(): string {
  const gD = palette("dark", ANCHORS.dark.accent);
  const gL = palette("light", ANCHORS.light.accent);
  const dD = palette("dark", ANCHORS.dangerDark);
  const dL = palette("light", ANCHORS.danger);
  const gM = palette("dark", ANCHORS.monochrome.accent, ANCHORS.monochrome);
  const dM = palette("dark", ANCHORS.dangerDark, ANCHORS.monochrome);

  const lightCss = [
    `/* ---- LIGHT ---- */`,
    `[data-theme="light"] {`,
    `  --bg: ${ANCHORS.light.background};`,
    `  --shell: ${ANCHORS.light.shell};`,
    `  --hairline-color: ${ANCHORS.light.hairline};`,
    `  --bar-fill: rgb(255 255 255 / 4.5%);`,
    block(
      "neutral",
      withStep(gL.grayScale, 12, ANCHORS.light.text),
      gL.grayScaleAlpha,
      gL.graySurface,
      ANCHORS.light.text,
    ),
    block("accent", gL.accentScale, gL.accentScaleAlpha, gL.accentSurface, ANCHORS.light.contrast),
    block(
      "danger",
      withStep(dL.accentScale, 11, ANCHORS.danger),
      dL.accentScaleAlpha,
      dL.accentSurface,
      dL.accentContrast,
    ),
    `}`,
  ].join("\n");

  const darkCss = [
    `/* ---- DARK (default) ---- */`,
    `:root,\n[data-theme="dark"] {`,
    `  --bg: ${ANCHORS.dark.background};`,
    `  --shell: ${ANCHORS.dark.shell};`,
    `  --hairline-color: ${ANCHORS.dark.hairline};`,
    `  --bar-fill: rgb(255 255 255 / 4.5%);`,
    block(
      "neutral",
      withStep(gD.grayScale, 12, ANCHORS.dark.text),
      gD.grayScaleAlpha,
      gD.graySurface,
      ANCHORS.dark.text,
    ),
    block("accent", gD.accentScale, gD.accentScaleAlpha, gD.accentSurface, ANCHORS.dark.contrast),
    block("danger", dD.accentScale, dD.accentScaleAlpha, dD.accentSurface, ANCHORS.dark.contrast),
    `}`,
  ].join("\n");

  const monochromeCss = [
    `[data-theme="dark"][data-palette="monochrome"] {`,
    `  --bg: ${ANCHORS.monochrome.background};`,
    `  --shell: ${ANCHORS.monochrome.shell};`,
    `  --hairline-color: ${ANCHORS.monochrome.hairline};`,
    block(
      "neutral",
      withStep(gM.grayScale, 12, ANCHORS.monochrome.text),
      gM.grayScaleAlpha,
      gM.graySurface,
      ANCHORS.monochrome.text,
    ),
    block(
      "accent",
      gM.accentScale,
      gM.accentScaleAlpha,
      gM.accentSurface,
      ANCHORS.monochrome.contrast,
    ),
    block(
      "danger",
      dM.accentScale,
      dM.accentScaleAlpha,
      dM.accentSurface,
      ANCHORS.monochrome.contrast,
    ),
    `}`,
  ].join("\n");

  const p3 = [
    `/* ---- P3 wide-gamut (progressive enhancement; sRGB above is the floor) ---- */`,
    `@supports (color: color(display-p3 1 1 1)) {`,
    `  @media (color-gamut: p3) {`,
    `    :root,\n    [data-theme="dark"] {`,
    p3block(
      "neutral",
      withStep(gD.grayScaleWideGamut, 12, ANCHORS.dark.text),
      gD.grayScaleAlphaWideGamut,
    ),
    p3block("accent", gD.accentScaleWideGamut, gD.accentScaleAlphaWideGamut),
    p3block("danger", dD.accentScaleWideGamut, dD.accentScaleAlphaWideGamut),
    `    }`,
    `    [data-theme="light"] {`,
    p3block(
      "neutral",
      withStep(gL.grayScaleWideGamut, 12, ANCHORS.light.text),
      gL.grayScaleAlphaWideGamut,
    ),
    p3block("accent", gL.accentScaleWideGamut, gL.accentScaleAlphaWideGamut),
    p3block(
      "danger",
      withStep(dL.accentScaleWideGamut, 11, ANCHORS.danger),
      dL.accentScaleAlphaWideGamut,
    ),
    `    }`,
    `    [data-theme="dark"][data-palette="monochrome"] {`,
    p3block(
      "neutral",
      withStep(gM.grayScaleWideGamut, 12, ANCHORS.monochrome.text),
      gM.grayScaleAlphaWideGamut,
    ),
    p3block("accent", gM.accentScaleWideGamut, gM.accentScaleAlphaWideGamut),
    p3block("danger", dM.accentScaleWideGamut, dM.accentScaleAlphaWideGamut),
    `    }`,
    `  }`,
    `}`,
  ].join("\n");

  return `${HEADER}\n${darkCss}\n\n${lightCss}\n\n${monochromeCss}\n\n${p3}\n`;
}

export const ARTIFACT_PATH = join(
  import.meta.dirname,
  "../../src/styles/themes/primitives.generated.css",
);

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  writeFileSync(ARTIFACT_PATH, buildPrimitivesCss());
  console.log(`wrote ${ARTIFACT_PATH}`);
}
