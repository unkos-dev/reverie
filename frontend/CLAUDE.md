# Frontend — React + Vite + TypeScript

## Components

- Functional components only. No class components.
- One primary export per file (small helpers may share a file).
- Components render UI only — logic in custom hooks.
- Props:
  - TS interface `XxxProps`.
  - Callbacks: `onXxx` (`onClick`, `onSubmit`).
- Return type: `ReactElement` (not `JSX.Element`, deprecated React 19).
- No complex inline JSX expressions — extract to vars or helpers.
- Page components need error boundary.
- Every async op handles four states: loading, error, empty, success. User-friendly messages; log raw errors to console.
- No inline style objects (except dynamic values). No `!important`. Use `clsx` / `cn` for conditional classes.

## Hooks

- Custom hook file + export names prefixed `use` (`useAuth.ts`).
- One thing per hook.
- `useEffect` has complete dep array. Never suppress with `// eslint-disable-next-line`.
- `useEffect` with side effects returns cleanup.
- Never pass async fn directly to `useEffect`. Use `AbortController` pattern:

  ```tsx
  useEffect(() => {
    const controller = new AbortController();
    const load = async () => {
      try {
        const data = await fetchData({ signal: controller.signal });
        setData(data);
      } catch (error) {
        if (!controller.signal.aborted) setError(error);
      }
    };
    load();
    return () => controller.abort();
  }, []);
  ```

## Performance

- `React.memo` / `useMemo` / `useCallback`: use only when a measured need
  exists — not preemptively. Valid triggers: expensive derivations; hook
  dependencies; props passed to memoized children.
- List `key` values must be stable and unique. Never use array index.
- For large lists, use virtualization (`react-virtual` / `react-window`).

## TypeScript

- Strict mode is mandatory. No `any` — use `unknown` when the type is
  genuinely uncertain, then narrow with type guards.
- Public functions have explicit return types; internal helpers may rely on
  inference.
- No `as` type assertions (unless narrowing from `unknown` with a documented
  reason).
- No `!` non-null assertions — use optional chaining (`?.`) or explicit null
  checks.
- No `enum` — prefer `as const` objects + union types
  (`type X = typeof X[keyof typeof X]`).
- Typed catch blocks: `catch (error) { if (error instanceof Foo) … }`; never
  `catch (e: any)`.
- `exactOptionalPropertyTypes`: pass optional props via conditional spread
  (`{...(flag ? { helper: "x" } : {})}`), never
  `helper={flag ? "x" : undefined}`.
- `import type` separate from value imports.
- No `@ts-ignore` / `@ts-expect-error` without a comment explaining why.

## State & data

- Start with React built-ins (`useState`, `useReducer`, `useContext`). Add an
  external state manager only when a clear need emerges.
- Prop drilling beyond 2 levels → Context (or a state manager).
- **Runtime validation at system boundaries:** all API response bodies, URL
  params, and form inputs parsed through a schema (Zod or equivalent) before
  use. Derive the compile-time type with `z.infer<typeof schema>`.
- API calls centralise in `src/api/`. Components never call `fetch` directly.

## Styling

- Tailwind CSS (v4) utility classes. Tailwind is configured via
  `@tailwindcss/vite` in `vite.config.ts`. Design tokens live in
  `src/styles/themes/` as a three-tier tree: `primitives.generated.css`
  (Radix ramps generated from the brand anchors — never hand-edited,
  regenerate), `index.css` (semantic roles + shadcn aliases via
  `@theme inline` / `var()`, hex-banned by stylelint), and
  `atmosphere.css` (sealed art-directed tier — chrome must not consume).
  This tree implements the palette; its source of truth is the
  reverie-branding repo (`identity.md`), and drift resolves in
  branding's favour.
  Self-hosted variable woff2 fonts at
  `public/fonts/fontshare/files/` (Author + Satoshi + JetBrains Mono).
  Never use arbitrary hex values — reuse a token. The Lockup component
  is the documented exemption (philosophy §11C invariant; must render
  correctly even before the theme tree resolves).
- **shadcn/ui:** components added via CLI (`npx shadcn@latest add <component>`).
  Do not manually create shadcn components. The shadcn-namespace CSS
  variables (`--color-background`, `--color-card`, `--color-primary`,
  etc.) are aliased onto the brand palette in
  `styles/themes/index.css`, so stock primitives render brand-aligned
  without per-file rewrites.

## Dev environment variables

- `REVERIE_DEV_HOSTS` (optional, comma-separated) — non-loopback hostnames
  that the Vite dev server accepts in the request `Host` header. Vite's
  DNS-rebinding guard rejects unknown non-loopback hostnames; loopback hosts
  (`localhost`, `*.localhost`, any IPv4 / IPv6 literal) are accepted
  unconditionally by a hardcoded short-circuit in Vite's host-validation
  middleware regardless of the allowlist. Cloud dev environments (Coder,
  Codespaces) must export the workspace-assigned hostname, e.g.
  `REVERIE_DEV_HOSTS=dev.example.com npm run dev`. Parsing lives in
  `vite-plugins/allowed-hosts.ts`; the value is a strict replacement of the
  declarative defaults, not a merge. The same value also seeds the dev
  Content-Security-Policy: each non-loopback host gets a `wss://<host>` origin
  added to `connect-src` by `buildDevCsp` (`vite-plugins/dev-csp.ts`), so the
  HMR websocket is permitted through a TLS edge without a separate env var.
  Entries are validated as bare hostnames — a scheme, path, whitespace, or `;`
  throws at startup.

- `REVERIE_DEV_HMR_CLIENT_PORT` (optional, integer 1..=65535) — port the
  HMR websocket client reconnects to. Default (unset) = the dev server's
  own port (5173), which is correct for localhost / Coder port-forward
  access. Set to `443` when fronting the dev server with a reverse proxy
  on a different external port (e.g. a Cloudflare tunnel terminating
  TLS on 443) so the browser reconnects via the edge instead of trying
  `wss://<host>:5173/`. Parsing lives in `vite-plugins/hmr-config.ts`.

## Testing & tooling

- Vitest + React Testing Library. Test behaviour, not implementation.
- Two test projects in `vite.config.ts`: `vite-plugins` (node env, plugin
  tests under `vite-plugins/__tests__/`) and `frontend` (jsdom env, component
  and unit tests under `src/**/*.{test,spec}.{ts,tsx}` with setup file at
  `tests/setup.ts`). Both run together via `npm test`.
- Formatting enforced by ESLint. Do not disable rules without a documented
  reason.

## Project Structure (as it grows)

```text
frontend/
├── public/              # Static assets
├── src/
│   ├── api/             # API client functions
│   ├── components/      # Reusable UI components
│   │   └── ui/          # shadcn/ui components (generated)
│   ├── fouc/            # Pre-paint script hashed into HTML CSP at build
│   ├── hooks/           # Custom React hooks
│   ├── pages/           # Route-level page components
│   ├── lib/             # Utilities
│   ├── App.tsx          # Root component
│   └── main.tsx         # Entrypoint
├── vite-plugins/        # Custom Vite plugins (csp-hash.ts)
├── tests/               # Vitest setup (setup.ts)
├── index.html
├── tsconfig.json
└── vite.config.ts       # Tailwind v4 + Vitest projects configured here
```
