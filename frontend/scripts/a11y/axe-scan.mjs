// Accessibility gate runner. Drives `agent-browser` (CDP) to load the dev
// server's design showcase, injects axe-core, runs it with the FULL WCAG 2.2 AA
// tag set, then applies the documented allowlist and exits non-zero on any
// remaining violation. Writes a machine-readable artifact to a11y-results.json.
//
// Driver rationale: this workspace is ARM64; @axe-core/cli's bundled
// chromedriver + selenium-manager are x64-only ELF binaries (exec format error),
// and Chrome-for-Testing has no linux-arm64 build. agent-browser drives Brave
// locally (AGENT_BROWSER_EXECUTABLE=/usr/bin/brave-browser) and Chromium in CI
// over CDP — one mechanism on both arches, no chromedriver, no Playwright.
//
// Tag rationale: the WCAG 2.2 AA target needs all of wcag2a/wcag2aa/wcag21a/
// wcag21aa/wcag22aa. The `wcag22aa` tag ALONE selects only the rules NEW in 2.2
// (e.g. target-size) and returns ZERO color-contrast findings — it would make
// the gate pass trivially. Do not narrow it.
//
// Liveness: an empty violation set from a crashed browser, a blank page, or a
// wrong URL is indistinguishable from "0 violations". The runner asserts
// the scan genuinely ran (testEngine present, url matched, non-trivial
// passes/inapplicable, clean agent-browser exits) and fails the gate otherwise.

import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { verdict } from "./allowlist.mjs";

const BASE_URL = process.env.A11Y_BASE_URL ?? "http://localhost:5173";
const TARGETS = (process.env.A11Y_TARGETS ?? "/design/system")
  .split(",")
  .map((t) => t.trim())
  .filter(Boolean);
// Future targets per adr/2026-06-05-accessibility-review-process.md: /design/hero/*
// and, once an authenticated session is wired, the post-login Home/Library/Detail
// views. Add them to A11Y_TARGETS (or the default above) as they ship.

const AXE_SOURCE = readFileSync(
  fileURLToPath(new URL("../../node_modules/axe-core/axe.min.js", import.meta.url)),
  "utf8",
);

// Returned to the parent as a single JSON object (agent-browser JSON-encodes the
// resolved value once). `url` + `testEngine` + counts power the liveness check.
const AXE_TRAILER = `
axe.run(document, { runOnly: { type: "tag", values: ["wcag2a","wcag2aa","wcag21a","wcag21aa","wcag22aa"] } })
  .then((r) => ({
    url: window.location.pathname,
    testEngine: r.testEngine && r.testEngine.name,
    counts: { violations: r.violations.length, passes: r.passes.length, inapplicable: r.inapplicable.length },
    violations: r.violations.map((v) => ({
      id: v.id, impact: v.impact, help: v.help, helpUrl: v.helpUrl, tags: v.tags,
      nodes: v.nodes.map((n) => ({
        target: n.target, html: n.html, impact: n.impact,
        any: (n.any || []).map((c) => ({ id: c.id, data: c.data })),
      })),
    })),
  }));
`;

/**
 * Run an agent-browser subcommand, inheriting env (so AGENT_BROWSER_EXECUTABLE
 * propagates). Never throws — returns the spawn result for explicit handling.
 *
 * @param {string[]} args
 * @param {string} [input]
 * @returns {import("node:child_process").SpawnSyncReturns<string>}
 */
function ab(args, input) {
  return spawnSync("agent-browser", args, {
    input,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    env: process.env,
  });
}

/**
 * Scan a single target URL. Returns the parsed axe result plus a `scanOk` flag
 * and a `reason` when the scan could not be trusted.
 *
 * @param {string} target path beginning with "/"
 * @returns {{ target: string, scanOk: boolean, reason: string|null, result: unknown, violations: Array<unknown> }}
 */
function scanTarget(target) {
  const url = `${BASE_URL}${target}`;
  const fail = (reason) => ({ target, scanOk: false, reason, result: null, violations: [] });

  const open = ab(["open", url]);
  if (open.status !== 0) {
    return fail(`agent-browser open failed (exit ${open.status}): ${open.stderr?.trim()}`);
  }
  const wait = ab(["wait", "--load", "networkidle"]);
  if (wait.status !== 0) {
    return fail(`agent-browser wait failed (exit ${wait.status}): ${wait.stderr?.trim()}`);
  }
  const evalRes = ab(["eval", "--stdin"], `${AXE_SOURCE}\n${AXE_TRAILER}`);
  if (evalRes.status !== 0) {
    return fail(`agent-browser eval failed (exit ${evalRes.status}): ${evalRes.stderr?.trim()}`);
  }

  let result;
  try {
    result = JSON.parse(evalRes.stdout.trim());
  } catch {
    return fail(`could not parse axe output as JSON: ${evalRes.stdout.slice(0, 200)}`);
  }

  // Liveness assertions: prove axe actually ran against the intended page.
  if (typeof result?.testEngine !== "string") {
    return fail("axe result missing testEngine — scan did not run");
  }
  if (typeof result?.url !== "string" || !result.url.endsWith(target)) {
    return fail(`scanned url "${result?.url}" does not match target "${target}"`);
  }
  const probed = (result.counts?.passes ?? 0) + (result.counts?.inapplicable ?? 0);
  if (probed <= 0) {
    return fail("axe reported 0 passes and 0 inapplicable rules — blank or error page");
  }

  return { target, scanOk: true, reason: null, result, violations: result.violations ?? [] };
}

function main() {
  const perTarget = [];
  try {
    for (const target of TARGETS) {
      console.log(`[a11y] scanning ${BASE_URL}${target} …`);
      perTarget.push(scanTarget(target));
    }
  } finally {
    ab(["close", "--all"]);
  }

  const scanOk = perTarget.length > 0 && perTarget.every((t) => t.scanOk);
  const violations = perTarget.flatMap((t) => t.violations);
  const { pass, remaining } = verdict({ violations, scanOk });

  writeFileSync(
    fileURLToPath(new URL("../../a11y-results.json", import.meta.url)),
    `${JSON.stringify(
      {
        scanOk,
        pass,
        targets: TARGETS,
        perTarget: perTarget.map(({ result, ...m }) => m),
        remaining,
        violations,
      },
      null,
      2,
    )}\n`,
  );

  console.log("");
  if (!scanOk) {
    for (const t of perTarget.filter((t) => !t.scanOk)) {
      console.error(`[a11y] SCAN FAILED on ${t.target}: ${t.reason}`);
    }
    console.error("[a11y] FAIL — scan did not run cleanly; refusing to report a green gate.");
    process.exit(1);
  }

  const allowlisted =
    violations.reduce((sum, v) => sum + (v.nodes?.length ?? 0), 0) -
    remaining.reduce((sum, v) => sum + (v.nodes?.length ?? 0), 0);
  console.log(
    `[a11y] scanned ${TARGETS.length} target(s); ${allowlisted} node(s) allowlisted (documented carve-out).`,
  );

  if (pass) {
    console.log("[a11y] PASS — no WCAG 2.2 AA violations outside the documented allowlist.");
    process.exit(0);
  }

  console.error("[a11y] FAIL — WCAG 2.2 AA violations outside the documented allowlist:");
  for (const v of remaining) {
    console.error(`  • ${v.id} (${v.impact}) — ${v.help}`);
    for (const n of v.nodes) {
      const data = n.any?.[0]?.data;
      const ratio = data
        ? ` [${data.contrastRatio}:1 vs ${data.expectedContrastRatio}, ${data.fgColor} on ${data.bgColor}]`
        : "";
      console.error(`      ${n.target?.[0]}${ratio}`);
    }
  }
  console.error(
    "\n[a11y] Fix the violation, or add a documented entry to scripts/a11y/allowlist.mjs (with rationale).",
  );
  process.exit(1);
}

main();
