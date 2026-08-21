// Consistency check for the frontend CycloneDX document produced in the image
// build's `frontend-sbom` stage.
//
// Records a verdict on stdout and always exits 0. The SBOM is a published
// deliverable, not a security control — Snyk and trivy scan the source and the
// image directly — so a defect here must never block a release. The scheduled
// published-image audit reads the recorded verdict and opens an issue, which is
// what stops a broken SBOM going unnoticed.
//
// `pnpm deploy` writes its own lockfile into the tree it materialises, and that
// lockfile is the production closure as pnpm itself resolved it. It is an
// independent reference for what syft should have found, generated for free by
// a step the stage already runs, so nothing here is hand-maintained and nothing
// needs rebaselining when a dependency moves.
//
// Containment rather than equality: syft additionally reports the workspace
// root and any nested manifest it walks, neither of which is a defect.
// Under-reporting is the direction that matters, because that is what would
// leave shipped code undescribed.

import { readFileSync } from "node:fs";

const [tree, cdxPath] = process.argv.slice(2);
if (!tree || !cdxPath) {
  console.log("FAIL usage: verify-frontend-sbom.mjs <deploy-tree> <cyclonedx-json>");
  process.exit(0);
}

const verdict = (tag, message) => {
  console.log(`${tag} ${message}`);
  process.exit(0);
};

// A `--filter` that silently resolved to a different package would leave the
// closure and the SBOM agreeing with each other and both describing the wrong
// thing, which no comparison between them could detect.
let name;
try {
  name = JSON.parse(readFileSync(`${tree}/package.json`, "utf8")).name;
} catch (error) {
  verdict("FAIL", `deploy tree has no readable package.json: ${error.message}`);
}
if (name !== "frontend") {
  verdict("FAIL", `deploy tree holds "${name}", expected "frontend"`);
}

let lock;
try {
  lock = readFileSync(`${tree}/pnpm-lock.yaml`, "utf8");
} catch (error) {
  verdict("FAIL", `deploy tree has no readable lockfile: ${error.message}`);
}

// `packages:` is the resolved closure. `importers:` is not usable here: it
// still records devDependencies even under --prod.
const section = (lock.split(/^packages:$/m)[1] ?? "").split(/^snapshots:$/m)[0];
const expected = [...section.matchAll(/^ {2}(?! )'?(.+?)'?:$/gm)].map((m) => m[1]);
// Guards the check against passing while matching nothing, which is the way a
// comparison like this normally rots.
if (expected.length === 0) {
  verdict("FAIL", "deploy lockfile lists no packages, so the comparison would be vacuous");
}

let cdx;
try {
  cdx = JSON.parse(readFileSync(cdxPath, "utf8"));
} catch (error) {
  verdict("FAIL", `SBOM is not readable JSON: ${error.message}`);
}

// purl percent-encodes the scope, so `@dnd-kit/core` arrives as `%40dnd-kit/core`.
const found = new Set(
  (cdx.components ?? [])
    .filter((c) => typeof c.purl === "string" && c.purl.startsWith("pkg:npm/"))
    .map((c) => decodeURIComponent(c.purl.slice("pkg:npm/".length).split("?")[0])),
);

const missing = expected.filter((e) => !found.has(e));
if (missing.length > 0) {
  const sample = missing.slice(0, 5).join(", ");
  verdict(
    "FAIL",
    `${missing.length} of ${expected.length} closure packages absent from the SBOM, e.g. ${sample}`,
  );
}

verdict("OK", `all ${expected.length} closure packages present`);
