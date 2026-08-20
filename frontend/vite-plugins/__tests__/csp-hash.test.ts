import { describe, expect, it } from "vite-plus/test";
import { execFileSync } from "node:child_process";
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
import type { Plugin, ResolvedConfig } from "vite-plus";
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
      void configResolved(fakeResolvedConfig(root, "serve"));
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
      void configResolved(fakeResolvedConfig(root, "build"));
    }

    getHandler(plugin)(VALID_HTML);
    const sidecarPath = join(root, "dist", "csp-hashes.json");
    const sidecar = JSON.parse(readFileSync(sidecarPath, "utf8")) as unknown as Record<
      string,
      string[]
    >;
    expect(sidecar).toEqual({ "script-src-hashes": [expected] });
  });

  it("throws when marker is missing", () => {
    const plugin = cspHashPlugin();
    const root = projectWithFouc("(function () {})();\n");
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      void configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)("<!doctype html><head></head>")).toThrow(/found 0/);
  });

  it("throws when marker appears twice", () => {
    const plugin = cspHashPlugin();
    const root = projectWithFouc("(function () {})();\n");
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      void configResolved(fakeResolvedConfig(root, "serve"));
    }
    const doubled = `<!doctype html><head><!-- reverie:fouc-hash --><!-- reverie:fouc-hash --></head>`;
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
      void configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)(VALID_HTML)).toThrow(/closing-script-tag literal/i);
  });

  // The HTML parser terminates an inline <script> at `</script` followed by
  // any of: whitespace (\s), `/`, or `>`. The guard must catch all four
  // termination signatures, not just `</script>`. UNK-114 issue 5.
  it.each([
    ["space", "var x = '</script ';"],
    ["tab", "var x = '</script\t';"],
    ["newline", "var x = '</script\n';"],
    ["slash", "var x = '</script/';"],
  ])("throws when fouc.js contains </script followed by %s", (_label, body) => {
    const plugin = cspHashPlugin();
    const root = projectWithFouc(body);
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      void configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)(VALID_HTML)).toThrow(/closing-script-tag literal/i);
  });

  it("does NOT throw when </script appears as a non-terminating substring (e.g. </scripty)", () => {
    const plugin = cspHashPlugin();
    // `</scripty>` is not a script terminator — the next char after `</script`
    // is a name character, not whitespace/slash/`>`. Must not false-positive.
    const body = "var s = '</scripty>';";
    const root = projectWithFouc(body);
    const configResolved = plugin.configResolved;
    if (typeof configResolved === "function") {
      // @ts-expect-error — see note above.
      void configResolved(fakeResolvedConfig(root, "serve"));
    }
    expect(() => getHandler(plugin)(VALID_HTML)).not.toThrow();
  });

  it("end-to-end: a real build via the workspace vp CLI produces a sidecar whose hash matches the injected inline script body", () => {
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

    // vp treats its build root as a workspace package and rejects a
    // directory with no package.json of its own, so the fixture needs one.
    writeFileSync(
      join(root, "package.json"),
      JSON.stringify({ name: "csp-hash-e2e-fixture", private: true, type: "module" }),
      "utf8",
    );

    // Re-use the workspace-root node_modules by symlinking rather than
    // installing into the fixture. The root declares vite itself, so its link
    // is there, and pnpm's virtual store sits inside that same directory, so
    // the symlink carries the transitive graph with it. Direct fs call rather
    // than `ln -s` via shell so CodeQL doesn't flag a shell command built from
    // a non-constant path.
    const parentNodeModules = resolve(thisDir, "..", "..", "..", "node_modules");
    symlinkSync(parentNodeModules, join(root, "node_modules"));

    // Resolve the workspace's own `vp` binary rather than letting `npx`
    // search for a `vite` CLI: the workspace aliases "vite" to
    // vite-plus-core, which ships no `vite` bin, so `npx vite build` used to
    // fall back to fetching the real vite package from the registry at test
    // time. `vp` is what the workspace actually builds with (frontend's
    // `build` script runs `vp build`), so invoking its absolute path here
    // both matches production/CI build behavior and needs nothing beyond
    // what the lockfile already installed.
    const vpBin = resolve(parentNodeModules, ".bin", "vp");
    if (!existsSync(vpBin)) {
      throw new Error(
        `workspace vp binary not found at ${vpBin}; the vite-plus toolchain dependency may have moved or been removed`,
      );
    }

    execFileSync(vpBin, ["build"], {
      cwd: root,
      stdio: "pipe",
      env: {
        ...process.env,
        // Regression guard: if a future change reintroduces a
        // package-manager fallback, forcing the package manager offline
        // against an unreachable registry makes any such fetch fail loudly
        // instead of silently succeeding against the network again.
        //
        // The prefix is load-bearing. pnpm reads `pnpm_config_<key>` and
        // ignores the `npm_config_` form these once used, so keeping the npm
        // spelling would leave the guard passing while enforcing nothing.
        pnpm_config_offline: "true",
        pnpm_config_registry: "http://csp-hash-e2e-hermetic-guard.invalid/",
      },
    });

    const sidecar = JSON.parse(
      readFileSync(join(root, "dist", "csp-hashes.json"), "utf8"),
    ) as unknown as Record<string, string[]>;
    const hashes = sidecar["script-src-hashes"];
    expect(hashes).toHaveLength(1);

    const builtHtml = readFileSync(join(root, "dist", "index.html"), "utf8");
    // Case-insensitive match — Vite emits lowercase but CodeQL flags
    // case-sensitive <script> extraction as a bad HTML filter.
    const match = builtHtml.match(/<script>([\s\S]*?)<\/script>/i);
    expect(match).not.toBeNull();
    if (!match) throw new Error("regex match was null after expect");
    const inlineBody = match[1];
    const expected = `sha256-${createHash("sha256").update(inlineBody).digest("base64")}`;

    expect(hashes[0]).toBe(expected);
    // This case runs a real build through the workspace's own `vp` binary
    // resolved from node_modules/.bin, so it needs no network access and no
    // package-manager resolution step. Measured locally at 300-350ms across
    // repeated runs; this budget keeps generous headroom for a slower or
    // colder CI runner without chasing a network-fetch ceiling that no
    // longer applies.
  }, 15_000);
});
