import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { expect, test } from "vite-plus/test";

const FRONTEND_ROOT = resolve(__dirname, "..", "..");

test("the production build lists its emitted pre-paint script's hash in csp-hashes.json", () => {
  const outDir = mkdtempSync(join(tmpdir(), "csp-hash-build-"));
  try {
    execFileSync(
      resolve(FRONTEND_ROOT, "node_modules", ".bin", "vp"),
      ["build", "--outDir", outDir],
      {
        cwd: FRONTEND_ROOT,
        stdio: "pipe",
      },
    );

    const html = readFileSync(join(outDir, "index.html"), "utf8");
    const script = /<script>([\s\S]*?)<\/script>/i.exec(html)?.[1];
    expect(script).toContain("dataset.theme");
    const hash = `sha256-${createHash("sha256")
      .update(script ?? "", "utf8")
      .digest("base64")}`;

    const sidecar = JSON.parse(readFileSync(join(outDir, "csp-hashes.json"), "utf8")) as {
      "script-src-hashes": string[];
    };
    expect(sidecar["script-src-hashes"]).toContain(hash);
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }
}, 60_000);
