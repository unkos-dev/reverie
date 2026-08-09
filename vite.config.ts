import { defineConfig } from "vite-plus";

// Repo-root config: the single authoritative fmt + lint config for the monorepo.
// fmt governs the whole tree (Rust .rs stays on cargo fmt); lint is frontend-only,
// so every lint override is scoped to frontend/**. Ignores are root-relative.
// Frontend build/server/test config stays in frontend/vite.config.ts.
export default defineConfig({
  fmt: {
    semi: true,
    singleQuote: false,
    trailingComma: "all",
    printWidth: 100,
    tabWidth: 2,
    proseWrap: "preserve",
    endOfLine: "lf",
    sortPackageJson: false,
    ignorePatterns: [
      "CHANGELOG.md",
      "backend/.sqlx/",
      "frontend/src/components/ui/",
      "docs/security/codeguard/",
      "backend/config.schema.json",
      "docs/src/content/docs/reference/configuration.mdx",
      "docs/openapi.json",
    ],
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
    ignorePatterns: ["frontend/dist"],
    overrides: [
      {
        files: ["frontend/**/*.{ts,tsx}"],
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
        files: ["frontend/src/components/ui/**", "frontend/**/*.test.{ts,tsx}"],
        rules: {
          "typescript/consistent-type-assertions": "off",
        },
        plugins: ["typescript"],
      },
      {
        // The library browse surface owns its filter, sort and view state as
        // typed search params (frontend/src/lib/hooks/use-library-filters.ts).
        // Those writes reach the URL through history.replaceState, which React
        // Router's history does not observe, so useSearchParams cannot see
        // them: a read through it returns a snapshot that goes stale the
        // moment anything writes, and a write through it lands somewhere the
        // surface cannot read. The ban covers both directions, which is why it
        // is on the import rather than on the setter alone.
        //
        // The design-system pages under pages/design/** are deliberately out
        // of scope: they are standalone specimens with their own local params
        // and no shared filter state.
        files: [
          "frontend/src/pages/library/**",
          "frontend/src/components/library/**",
          "frontend/src/components/shell/FilterRail.tsx",
        ],
        rules: {
          "no-restricted-imports": [
            "error",
            {
              paths: [
                {
                  name: "react-router",
                  importNames: ["useSearchParams"],
                  message:
                    "The library surface reads and writes search params through lib/hooks/use-library-filters. useSearchParams cannot observe those writes, so it would return stale filter state.",
                },
              ],
            },
          ],
        },
      },
      {
        // Vendored Radix color engine (see its file header): upstream is
        // written against looser colorjs.io typings, so the type-aware rules
        // and the @ts-nocheck escape are exempted for this one file. The
        // drift test pins its emitted artifact, which is the real gate.
        files: ["frontend/scripts/radix-gen/generate-radix-colors.ts"],
        rules: {
          "typescript/ban-ts-comment": "off",
          "typescript/no-unnecessary-condition": "off",
          "typescript/restrict-plus-operands": "off",
          "typescript/restrict-template-expressions": "off",
        },
        plugins: ["typescript"],
      },
      {
        files: [
          "frontend/src/api/**",
          "frontend/src/lib/theme/api.ts",
          "frontend/src/hooks/useAuthMe.ts",
          "frontend/**/*.test.{ts,tsx}",
          "frontend/**/*.spec.{ts,tsx}",
          "frontend/tests/**",
        ],
        rules: {
          "no-restricted-globals": "off",
          "no-restricted-properties": "off",
        },
      },
      {
        files: ["frontend/vite.config.ts", "frontend/vite-plugins/**/*.ts", "frontend/*.config.ts"],
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
});
