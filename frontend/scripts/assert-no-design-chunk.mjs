import { readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// The dev-only design showcase is routed into a "design" manualChunk that
// main.tsx imports behind `import.meta.env.DEV`. In a production build the
// branch is dead code and the chunk must not be emitted. A leaked
// `design-*.js` would ship the showcase (and its dev-only surface) to users,
// so this gate fails the build when one is present. Run after `vp build`.
const DESIGN_CHUNK = /^design-.*\.js$/;

/**
 * @param {string[]} filenames - basenames in dist/assets
 * @returns {boolean} true if any production design chunk is present
 */
export function hasDesignChunk(filenames) {
  return filenames.some((name) => DESIGN_CHUNK.test(name));
}

function main() {
  const assetsDir = join(process.cwd(), "dist", "assets");
  let entries;
  try {
    entries = readdirSync(assetsDir);
  } catch {
    console.error(
      `assert-no-design-chunk: ${assetsDir} not found. Run the production build first.`,
    );
    process.exit(2);
  }
  if (entries.length === 0) {
    console.error(`assert-no-design-chunk: ${assetsDir} is empty. The build emitted no assets.`);
    process.exit(2);
  }
  const offenders = entries.filter((name) => DESIGN_CHUNK.test(name));
  if (offenders.length > 0) {
    console.error(
      `assert-no-design-chunk: dev-only design chunk leaked into the production build: ${offenders.join(", ")}`,
    );
    process.exit(1);
  }
  console.log("assert-no-design-chunk: ok (no design-*.js in dist/assets)");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
