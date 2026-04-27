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
])
