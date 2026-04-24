import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { Plugin, ResolvedConfig } from "vite";
import { cspHashPlugin } from "../csp-hash";

// Type guard — transformIndexHtml can be a function or an object with
// `{ order, handler }`. The plugin always returns the object form.
function getHandler(plugin: Plugin): (html: string) => string {
  const hook = plugin.transformIndexHtml;
  if (!hook || typeof hook === "function" || !("handler" in hook)) {
    throw new Error("plugin did not expose an object-form transformIndexHtml");
  }
  // The hook handler is declared as (html, ctx) => string | Promise<string>.
  // We invoke it without ctx here (plugin ignores ctx by design).
  return hook.handler as (html: string) => string;
}

function fakeResolvedConfig(root: string, command: "build" | "serve" = "build") {
  return {
    root,
    command,
    build: { outDir: "dist" },
  } as unknown as ResolvedConfig;
}

function projectWithFouc(body: string) {
  const root = mkdtempSync(join(tmpdir(), "csp-hash-"));
  mkdirSync(join(root, "src", "fouc"), { recursive: true });
  writeFileSync(join(root, "src", "fouc", "fouc.js"), body, "utf8");
  return root;
}

const VALID_HTML = `<!doctype html><html><head><!-- reverie:fouc-hash --></head><body></body></html>`;

describe("cspHashPlugin", () => {
  it("produces a pinned sha256 for the empty-IIFE fixture", () => {
    const body = "(function () {})();\n";
    const expected = createHash("sha256").update(body).digest("base64");

    const plugin = cspHashPlugin();
    const root = projectWithFouc(body);
    // Invoke the plugin's configResolved lifecycle hook to populate its
    // captured `resolvedConfig`. It's a regular function on the Plugin
    // object, but may be wrapped in the object-form `{ order, handler }`
    // shape by Vite's newer hook API — handle both.
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // Vite's typedef marks `configResolved` as async-capable; for our
      // synchronous assignment it's safe to invoke directly.
      // @ts-expect-error — the cb type admits Promise<void>; we don't await.
      configResolved(fakeResolvedConfig(root, "serve"));
    }

    const html = getHandler(plugin)(VALID_HTML);
    expect(html).toContain(`<script>${body}</script>`);
    // Sidecar must NOT exist in serve mode.
    expect(existsSync(join(root, "dist", "csp-hashes.json"))).toBe(false);
    // Hash must be standard base64 (no - or _).
    expect(expected).toMatch(/^[A-Za-z0-9+/]+={0,2}$/);
  });

  it("writes csp-hashes.json on build with matching hash", () => {
    const body = "(function () {\n  document.documentElement.dataset.theme = 'dark';\n})();\n";
    const expected = `sha256-${createHash("sha256").update(body).digest("base64")}`;

    const plugin = cspHashPlugin();
    const root = projectWithFouc(body);
    mkdirSync(join(root, "dist"), { recursive: true });
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      configResolved(fakeResolvedConfig(root, "build"));
    }

    getHandler(plugin)(VALID_HTML);
    const sidecarPath = join(root, "dist", "csp-hashes.json");
    const sidecar = JSON.parse(readFileSync(sidecarPath, "utf8"));
    expect(sidecar).toEqual({ "script-src-hashes": [expected] });
  });

  it("throws when marker is missing", () => {
    const plugin = cspHashPlugin();
    const root = projectWithFouc("(function () {})();\n");
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)("<!doctype html><head></head>")).toThrow(
      /found 0/,
    );
  });

  it("throws when marker appears twice", () => {
    const plugin = cspHashPlugin();
    const root = projectWithFouc("(function () {})();\n");
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      configResolved(fakeResolvedConfig(root, "serve"));
    }
    const doubled =
      `<!doctype html><head><!-- reverie:fouc-hash --><!-- reverie:fouc-hash --></head>`;
    expect(() => getHandler(plugin)(doubled)).toThrow(/found 2/);
  });

  it("throws when fouc.js contains </script>", () => {
    const plugin = cspHashPlugin();
    // Case-insensitive — </SCRIPT> must also trip the guard.
    const body = "var x = '</ScRiPt>';";
    const root = projectWithFouc(body);
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)(VALID_HTML)).toThrow(/<\/script>/i);
  });

  it("end-to-end: `npx vite build` produces sidecar whose hash matches the injected inline script body", () => {
    // Build a temp project that imports the plugin from the parent tree.
    const thisDir = resolve(__dirname);
    const pluginPath = resolve(thisDir, "..", "csp-hash.ts");

    const root = mkdtempSync(join(tmpdir(), "csp-hash-e2e-"));
    mkdirSync(join(root, "src", "fouc"), { recursive: true });
    const body = "(function () { /* e2e fixture */ })();\n";
    writeFileSync(join(root, "src", "fouc", "fouc.js"), body, "utf8");

    const html = `<!doctype html><html><head><!-- reverie:fouc-hash --><title>e2e</title></head><body><script type="module">console.log(1)</script></body></html>`;
    writeFileSync(join(root, "index.html"), html, "utf8");

    // Minimal vite config that imports the plugin by absolute path.
    const viteConfig = `
import { defineConfig } from "vite";
import { cspHashPlugin } from ${JSON.stringify(pluginPath)};
export default defineConfig({ plugins: [cspHashPlugin()], build: { minify: false } });
`;
    writeFileSync(join(root, "vite.config.ts"), viteConfig, "utf8");

    // Re-use the parent project's node_modules for vite + plugin types
    // by symlinking instead of a full `npm install` here. Direct fs call
    // rather than `ln -s` via shell so CodeQL doesn't flag a shell command
    // built from a non-constant path.
    const parentNodeModules = resolve(thisDir, "..", "..", "node_modules");
    symlinkSync(parentNodeModules, join(root, "node_modules"));

    execSync("npx vite build", { cwd: root, stdio: "pipe" });

    const sidecar = JSON.parse(
      readFileSync(join(root, "dist", "csp-hashes.json"), "utf8"),
    );
    const hashes = sidecar["script-src-hashes"] as string[];
    expect(hashes).toHaveLength(1);

    const builtHtml = readFileSync(join(root, "dist", "index.html"), "utf8");
    // Case-insensitive match — Vite emits lowercase but CodeQL flags
    // case-sensitive <script> extraction as a bad HTML filter.
    const match = builtHtml.match(/<script>([\s\S]*?)<\/script>/i);
    expect(match).not.toBeNull();
    const inlineBody = match![1];
    const expected = `sha256-${createHash("sha256").update(inlineBody).digest("base64")}`;

    expect(hashes[0]).toBe(expected);
  }, 30_000);
});
