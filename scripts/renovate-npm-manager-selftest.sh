#!/usr/bin/env bash
# The custom manager that tracks devEngines.packageManager.version is the only
# thing that makes that copy visible to Renovate: the built-in npm manager
# reads the top-level packageManager field and never devEngines. If its regex
# stops matching, Renovate silently drops the dep, raises the npm bump against
# mise.toml alone, and reinstates the mismatch npm-pin-drift exists to catch.
#
# A custom manager that matches nothing produces no deps and no error, so
# nothing else in this repository can notice that failure. This asserts the
# regex against the committed package.json, and that it rejects the shapes
# npm-pin-drift also rejects, so the guard and the manager cannot drift into
# disagreeing about which shape is canonical.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

# The quoted heredoc delimiter keeps the shell out of the JS: the template
# literals below are JavaScript interpolation, not shell expansion.
node --input-type=module - <<'JS'
import { readFileSync } from "node:fs";

const config = JSON.parse(readFileSync(".github/renovate.json", "utf8"));
const manager = config.customManagers?.find((m) =>
  m.description?.includes("devEngines"),
);
if (!manager) {
  console.error("FAIL: no custom manager describes devEngines; the npm pin is invisible to Renovate");
  process.exit(1);
}
if (manager.depNameTemplate !== "npm" || manager.datasourceTemplate !== "npm") {
  console.error(`FAIL: manager must resolve depName/datasource npm to join the npm-pin group, got ${manager.depNameTemplate}/${manager.datasourceTemplate}`);
  process.exit(1);
}

// Renovate applies matchStrings with the "s" flag by default.
const re = new RegExp(manager.matchStrings[0], "s");
const pkg = readFileSync("package.json", "utf8");
const declared = JSON.parse(pkg).devEngines.packageManager.version;

let failed = 0;
const check = (name, ok, detail) => {
  if (ok) {
    console.log(`ok   ${name}`);
  } else {
    console.error(`FAIL ${name}: ${detail}`);
    failed = 1;
  }
};

const live = re.exec(pkg);
check(
  "regex extracts the committed devEngines version",
  live && live.groups.currentValue === declared,
  `expected ${declared}, got ${live ? live.groups.currentValue : "no match"}`,
);

// Key order is not guaranteed by the JSON spec and a reformat could move
// version ahead of name, so the pattern must not depend on their order.
const reordered = JSON.stringify(
  { devEngines: { packageManager: { version: declared, name: "npm" } } },
  null,
  2,
);
const hit = re.exec(reordered);
check(
  "regex survives reordered keys",
  hit && hit.groups.currentValue === declared,
  hit ? `matched ${hit.groups.currentValue}` : "no match",
);

// The shapes npm-pin-drift rejects must not match here either. If one did,
// the guard would reject a file Renovate was happily bumping.
for (const [name, src] of [
  ["array form", JSON.stringify({ devEngines: { packageManager: [{ name: "npm", version: declared }] } })],
  ["string form", JSON.stringify({ devEngines: { packageManager: `npm@${declared}` } })],
]) {
  check(`regex ignores the ${name} that npm-pin-drift rejects`, !re.exec(src), "unexpectedly matched");
}

process.exit(failed);
JS
