import path from "node:path";
import { defineConfig, lazyPlugins, type PluginOption } from "vite-plus";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { parseAllowedHosts } from "./vite-plugins/allowed-hosts";
import { cspHashPlugin } from "./vite-plugins/csp-hash";
import { buildDevCsp } from "./vite-plugins/dev-csp";
import { parseHmrConfig } from "./vite-plugins/hmr-config";

// Dev-only CSP. The `connect-src` HMR websocket origins are derived from
// REVERIE_DEV_HOSTS so a non-loopback dev host fronted by a TLS edge is
// allowed to open its HMR socket; see vite-plugins/dev-csp.ts for the policy
// rationale and the Cloudflare RUM beacon allowances.
const DEV_CSP = buildDevCsp(process.env.REVERIE_DEV_HOSTS);

export default defineConfig({
  // Mirror the repo-wide oxfmt config (root .oxfmtrc.json) so vp's formatter
  // agrees with the committed style; ignorePatterns carry the frontend-scoped
  // carve-outs (shadcn primitives + the generated a11y report).
  fmt: {
    semi: true,
    singleQuote: false,
    trailingComma: "all",
    printWidth: 100,
    tabWidth: 2,
    proseWrap: "preserve",
    endOfLine: "lf",
    sortPackageJson: false,
    ignorePatterns: ["src/components/ui/", "a11y-results.json"],
  },
  lint: {
    plugins: ["typescript", "react"],
    categories: {
      correctness: "off",
    },
    options: {
      typeAware: true,
      typeCheck: true,
    },
    env: {
      builtin: true,
    },
    ignorePatterns: ["dist"],
    overrides: [
      {
        files: ["**/*.{ts,tsx}"],
        rules: {
          "constructor-super": "error",
          "for-direction": "error",
          "getter-return": "error",
          "no-async-promise-executor": "error",
          "no-case-declarations": "error",
          "no-class-assign": "error",
          "no-compare-neg-zero": "error",
          "no-cond-assign": "error",
          "no-const-assign": "error",
          "no-constant-binary-expression": "error",
          "no-constant-condition": "error",
          "no-control-regex": "error",
          "no-debugger": "error",
          "no-delete-var": "error",
          "no-dupe-class-members": "error",
          "no-dupe-else-if": "error",
          "no-dupe-keys": "error",
          "no-duplicate-case": "error",
          "no-empty": "error",
          "no-empty-character-class": "error",
          "no-empty-pattern": "error",
          "no-empty-static-block": "error",
          "no-ex-assign": "error",
          "no-extra-boolean-cast": "error",
          "no-fallthrough": "error",
          "no-func-assign": "error",
          "no-global-assign": "error",
          "no-import-assign": "error",
          "no-invalid-regexp": "error",
          "no-irregular-whitespace": "error",
          "no-loss-of-precision": "error",
          "no-misleading-character-class": "error",
          "no-new-native-nonconstructor": "error",
          "no-nonoctal-decimal-escape": "error",
          "no-obj-calls": "error",
          "no-prototype-builtins": "error",
          "no-redeclare": "error",
          "no-regex-spaces": "error",
          "no-self-assign": "error",
          "no-setter-return": "error",
          "no-shadow-restricted-names": "error",
          "no-sparse-arrays": "error",
          "no-this-before-super": "error",
          "no-unassigned-vars": "error",
          "no-undef": "off",
          "no-unexpected-multiline": "error",
          "no-unreachable": "error",
          "no-unsafe-finally": "error",
          "no-unsafe-negation": "error",
          "no-unsafe-optional-chaining": "error",
          "no-unused-labels": "error",
          "no-unused-private-class-members": "error",
          "no-unused-vars": "error",
          "no-useless-assignment": "error",
          "no-useless-backreference": "error",
          "no-useless-catch": "error",
          "no-useless-escape": "error",
          "no-with": "error",
          "preserve-caught-error": "error",
          "require-yield": "error",
          "use-isnan": "error",
          "valid-typeof": "error",
          "no-array-constructor": "error",
          "no-implied-eval": "off",
          "no-unused-expressions": "error",
          "no-useless-constructor": "error",
          "no-throw-literal": "off",
          "prefer-promise-reject-errors": "off",
          "require-await": "off",
          "typescript/await-thenable": "error",
          "typescript/ban-ts-comment": [
            "error",
            {
              minimumDescriptionLength: 10,
            },
          ],
          "typescript/no-array-delete": "error",
          "typescript/no-base-to-string": "error",
          "typescript/no-confusing-void-expression": "error",
          "typescript/no-deprecated": "error",
          "typescript/no-duplicate-enum-values": "error",
          "typescript/no-duplicate-type-constituents": "error",
          "typescript/no-dynamic-delete": "error",
          "typescript/no-empty-object-type": "error",
          "typescript/no-explicit-any": "error",
          "typescript/no-extra-non-null-assertion": "error",
          "typescript/no-extraneous-class": "error",
          "typescript/no-floating-promises": "error",
          "typescript/no-for-in-array": "error",
          "typescript/no-implied-eval": "error",
          "typescript/no-invalid-void-type": "error",
          "typescript/no-meaningless-void-operator": "error",
          "typescript/no-misused-new": "error",
          "typescript/no-misused-promises": "error",
          "typescript/no-misused-spread": "error",
          "typescript/no-mixed-enums": "error",
          "typescript/no-namespace": "error",
          "typescript/no-non-null-asserted-nullish-coalescing": "error",
          "typescript/no-non-null-asserted-optional-chain": "error",
          "typescript/no-non-null-assertion": "error",
          "typescript/no-redundant-type-constituents": "error",
          "typescript/no-require-imports": "error",
          "typescript/no-this-alias": "error",
          "typescript/no-unnecessary-boolean-literal-compare": "error",
          "typescript/no-unnecessary-condition": "error",
          "typescript/no-unnecessary-template-expression": "error",
          "typescript/no-unnecessary-type-arguments": "error",
          "typescript/no-unnecessary-type-assertion": "error",
          "typescript/no-unnecessary-type-constraint": "error",
          "typescript/no-unnecessary-type-conversion": "error",
          "typescript/no-unnecessary-type-parameters": "error",
          "typescript/no-unsafe-argument": "error",
          "typescript/no-unsafe-assignment": "error",
          "typescript/no-unsafe-call": "error",
          "typescript/no-unsafe-declaration-merging": "error",
          "typescript/no-unsafe-enum-comparison": "error",
          "typescript/no-unsafe-function-type": "error",
          "typescript/no-unsafe-member-access": "error",
          "typescript/no-unsafe-return": "error",
          "typescript/no-unsafe-unary-minus": "error",
          "typescript/no-useless-default-assignment": "error",
          "typescript/no-wrapper-object-types": "error",
          "typescript/only-throw-error": "error",
          "typescript/prefer-as-const": "error",
          "typescript/prefer-literal-enum-member": "error",
          "typescript/prefer-namespace-keyword": "error",
          "typescript/prefer-promise-reject-errors": "error",
          "typescript/prefer-reduce-type-parameter": "error",
          "typescript/prefer-return-this-type": "error",
          "typescript/related-getter-setter-pairs": "error",
          "typescript/require-await": "error",
          "typescript/restrict-plus-operands": [
            "error",
            {
              allowAny: false,
              allowBoolean: false,
              allowNullish: false,
              allowNumberAndString: false,
              allowRegExp: false,
            },
          ],
          "typescript/restrict-template-expressions": [
            "error",
            {
              allowAny: false,
              allowBoolean: false,
              allowNever: false,
              allowNullish: false,
              allowNumber: false,
              allowRegExp: false,
            },
          ],
          "typescript/return-await": ["error", "error-handling-correctness-only"],
          "typescript/triple-slash-reference": "error",
          "typescript/unbound-method": "error",
          "typescript/unified-signatures": "error",
          "typescript/use-unknown-in-catch-callback-variable": "error",
          "react/rules-of-hooks": "error",
          "react/exhaustive-deps": "warn",
          "react/only-export-components": [
            "error",
            {
              allowConstantExport: true,
            },
          ],
          "no-restricted-globals": [
            "error",
            {
              name: "fetch",
              message:
                "Direct fetch is banned outside src/api/. Use apiFetch (src/api/fetch.ts) so cookies, CSRF and Problem Details handling stay centralised.",
            },
          ],
          "no-restricted-properties": [
            "error",
            {
              object: "window",
              property: "fetch",
              message:
                "Direct fetch is banned outside src/api/. Use apiFetch (src/api/fetch.ts) so cookies, CSRF and Problem Details handling stay centralised.",
            },
            {
              object: "globalThis",
              property: "fetch",
              message:
                "Direct fetch is banned outside src/api/. Use apiFetch (src/api/fetch.ts) so cookies, CSRF and Problem Details handling stay centralised.",
            },
          ],
          "typescript/consistent-type-assertions": [
            "error",
            {
              assertionStyle: "as",
              objectLiteralTypeAssertions: "never",
            },
          ],
          "typescript/consistent-type-imports": "error",
          "react/jsx-key": "error",
          "react/no-array-index-key": "error",
          "react/no-danger": "error",
          "react/no-danger-with-children": "error",
          "react/jsx-no-script-url": "error",
          "react/no-find-dom-node": "error",
          "react/iframe-missing-sandbox": "error",
          "react/no-direct-mutation-state": "error",
          "react/no-render-return-value": "error",
          "react/void-dom-elements-no-children": "error",
          "react/no-unknown-property": "error",
          "react/jsx-no-duplicate-props": "error",
          "react/no-unstable-nested-components": "error",
          "react/react-compiler": "error",
        },
        env: {
          es2020: true,
          browser: true,
        },
      },
      {
        files: ["src/components/ui/**", "**/*.test.{ts,tsx}"],
        rules: {
          "typescript/consistent-type-assertions": "off",
        },
        plugins: ["typescript"],
      },
      {
        files: [
          "src/api/**",
          "src/lib/theme/api.ts",
          "src/hooks/useAuthMe.ts",
          "**/*.test.{ts,tsx}",
          "**/*.spec.{ts,tsx}",
          "tests/**",
        ],
        rules: {
          "no-restricted-globals": "off",
          "no-restricted-properties": "off",
        },
      },
      {
        files: ["vite.config.ts", "vite-plugins/**/*.ts", "*.config.ts"],
        env: {
          node: true,
          browser: false,
        },
      },
    ],
    settings: {
      react: {
        version: "19.2.6",
      },
    },
    jsPlugins: [
      {
        name: "vite-plus",
        specifier: "vite-plus/oxlint-plugin",
      },
    ],
    rules: {
      "vite-plus/prefer-vite-plus-imports": "error",
    },
  },
  // vp lazyPlugins returns Plugin<any>[] which TS cannot reconcile with the
  // PluginOption[] vite-plus expects across the rolldown-vite type graph
  // (vitejs/vite#20948); the assertion is the documented vp-recommended fix.
  plugins: lazyPlugins(() => [react(), tailwindcss(), cspHashPlugin()] as PluginOption[]),
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  build: {
    rollupOptions: {
      output: {
        // Route the dev-only design tree into its own chunk. main.tsx gates
        // the import behind `if (import.meta.env.DEV)`; in production
        // `import.meta.env.DEV` is replaced with literal `false`, the
        // dynamic-import branch becomes dead code, Vite tree-shakes the
        // chunk, and no `design-*.js` is emitted into `dist/assets/`.
        // Substring-grepping the minified output is unreliable (Vite
        // mangles names); the Level 4 gate in the plan checks for the
        // chunk file's structural absence instead.
        manualChunks(id) {
          if (id.includes("/src/routes/design") || id.includes("/src/pages/design/")) {
            return "design";
          }
        },
      },
    },
  },
  server: {
    headers: {
      "Content-Security-Policy": DEV_CSP,
    },
    // Bind on all interfaces (IPv4 + IPv6) so cloud dev environments
    // (Coder, Codespaces, Gitpod, ngrok) and same-host reverse proxies
    // can reach the dev server. Without this, Vite binds only to
    // localhost and an IPv4-side proxy hits ECONNREFUSED.
    host: true,
    // When fronted by a reverse proxy on a different external port
    // (e.g. a Cloudflare tunnel terminating TLS on 443), the browser
    // would otherwise try `wss://<host>:5173/` and fail — set
    // REVERIE_DEV_HMR_CLIENT_PORT to reconnect via the edge instead.
    // Localhost dev leaves it unset.
    ...parseHmrConfig(process.env.REVERIE_DEV_HMR_CLIENT_PORT),
    // DNS-rebinding guard active against an env-driven allowlist
    // (REVERIE_DEV_HOSTS, comma-separated). The guard rejects
    // non-loopback hostnames that are not in the allowlist; loopback
    // hosts (localhost, *.localhost, any IPv4/IPv6 literal) are
    // accepted unconditionally by Vite's hardcoded short-circuit (see
    // the comment in vite-plugins/allowed-hosts.ts). The proxy block
    // below forwards `/api`, `/auth`, and `/opds` to the backend,
    // including authenticated routes; bounding the allowlist closes
    // the DNS-rebind path that previously reached those routes when
    // the guard was disabled. Cloud dev environments (Coder,
    // Codespaces) must export REVERIE_DEV_HOSTS to match their
    // assigned hostname (see frontend/CLAUDE.md and dev/README.md).
    allowedHosts: parseAllowedHosts(process.env.REVERIE_DEV_HOSTS),
    proxy: {
      "/api": { target: "http://localhost:3000", changeOrigin: true },
      "/auth": { target: "http://localhost:3000", changeOrigin: true },
      "/opds": { target: "http://localhost:3000", changeOrigin: true },
    },
  },
  test: {
    // Coverage is configured at the root (not per-project) so a single
    // report aggregates both the vite-plugins and frontend projects. The
    // LCOV reporter writes coverage/lcov.info, which CI uploads to
    // SonarQube Cloud.
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      include: ["src/**", "vite-plugins/**", "scripts/a11y/**"],
    },
    projects: [
      {
        extends: true,
        test: {
          name: "vite-plugins",
          environment: "node",
          include: ["vite-plugins/**/__tests__/**/*.test.ts"],
        },
      },
      {
        // a11y gate logic (scripts/a11y/) is plain ESM tooling, not app code,
        // so it lives outside src/ and runs in a node env like vite-plugins.
        // The pure allowlist/verdict module is the only logic-bearing surface
        // and is unit-tested here; the runner that drives agent-browser is
        // exercised end-to-end via `npm run a11y`.
        extends: true,
        test: {
          name: "a11y",
          environment: "node",
          include: ["scripts/a11y/**/__tests__/**/*.test.mjs"],
        },
      },
      {
        extends: true,
        test: {
          name: "frontend",
          environment: "jsdom",
          globals: true,
          setupFiles: ["./tests/setup.ts"],
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
        },
      },
    ],
  },
});
