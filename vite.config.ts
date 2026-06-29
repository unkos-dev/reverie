import { defineConfig } from "vite-plus";

// Repo-root config. This fmt block is the single oxfmt config for the whole
// tree; ignores are root-relative. Rust .rs stays on cargo fmt.
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
      "frontend/a11y-results.json",
      "backend/config.schema.json",
      "docs/src/content/docs/reference/configuration.mdx",
      "docs/openapi.json",
    ],
  },
});
