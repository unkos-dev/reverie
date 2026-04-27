import js from '@eslint/js'                                                                                                                                                          
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'                                                                                                                                   
import reactRefresh from 'eslint-plugin-react-refresh'                                                         
import tseslint from 'typescript-eslint'     
import { defineConfig, globalIgnores } from 'eslint/config'

// Hex-literal ban — every brand colour must come from the canonical token
// system (--color-canvas, --color-fg, --color-accent, etc.). The matching
// Stylelint rule covers .css files; this rule covers .ts/.tsx call sites
// where a developer might be tempted to inline e.g.
// `style={{ color: '#C9A961' }}`. Lockup.tsx (and its test, which asserts
// on those very hex values) is the documented exemption — see the
// overrides block below and philosophy §11C.
const hexBanRule = {
  'no-restricted-syntax': [
    'error',
    {
      selector: "Literal[value=/^#[0-9a-fA-F]{3,8}$/]",
      message:
        'No raw hex codes in .tsx/.ts. Use semantic tokens (bg-canvas, text-fg, etc.).',
    },
  ],
}

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      ...hexBanRule,
      // Machine-enforces the frontend/CLAUDE.md ban on `as` casts against
      // object literals (`{ ... } as X`). The chained-through-`unknown`
      // form (`{ ... } as unknown as X`) — the documented escape hatch —
      // is unaffected. Two overrides below carve out the directories
      // where this pattern is unavoidable.
      '@typescript-eslint/consistent-type-assertions': [
        'error',
        { assertionStyle: 'as', objectLiteralTypeAssertions: 'never' },
      ],
    },
  },
  // Lockup is the canonical brand-identifier component and intentionally
  // inlines #C9A961 / #0E0D0A / #E8E0D0 as constants — see philosophy
  // spec §11C: the Lockup must render correctly even before
  // themes/index.css resolves (e.g. on the OIDC error page). The test
  // file asserts on those very hex values so the brand invariant
  // surfaces if anyone changes them. Per-file `eslint-disable-next-line`
  // would scatter the rationale; this overrides block keeps it visible
  // at the config level.
  {
    files: ['src/components/Lockup.tsx', 'src/components/Lockup.test.tsx'],
    rules: {
      'no-restricted-syntax': 'off',
    },
  },
  // shadcn/ui primitives are CLI-generated (`npx shadcn@latest add`).
  // The CLI emits `style={{...} as React.CSSProperties}` to type-cast
  // CSS-variable inline-style objects — a documented gap in
  // `@types/react`'s `style` typing that every React+CSS-vars project
  // hits. Rewriting the CLI output would conflict with future shadcn
  // registry updates; the alias-layer in `styles/themes/index.css`
  // handles theming without per-file edits. Off the cast rule here.
  {
    files: ['src/components/ui/**'],
    rules: {
      '@typescript-eslint/consistent-type-assertions': 'off',
    },
  },
  // Test fixture casts (`{ ok, status, json } as Response`) are partial
  // mocks — only the surface the test exercises is implemented. The
  // `as unknown as X` chain is the documented escape hatch but reads as
  // a verbose workaround for what is already a recognised testing
  // idiom. Most OSS TS projects (TanStack, tRPC, Astro, Prisma) carve
  // tests out of this rule for the same reason. Production code stays
  // strict.
  {
    files: ['**/*.test.{ts,tsx}'],
    rules: {
      '@typescript-eslint/consistent-type-assertions': 'off',
    },
  },
])