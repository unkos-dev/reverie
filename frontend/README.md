# Frontend

This directory contains the Vite React frontend.

Run `just js::test` from the repository root to execute the frontend unit tests.

## Interactive components

React Aria Components owns buttons, selection controls, disclosures, tabs, progress, overlays, the command palette and
vocabulary pickers. Wrappers in `src/components/ui/` use Reverie's Tailwind classes and theme tokens. Prefer native
interaction defaults and keep API queries and domain state in their owning components.

Vocabulary pickers supply server suggestions to the `ComboBox` collection and render hydrated selections inside
`ComboBoxValue` separately. Selection order follows the selected keys, not server ranking. Multiple selection keeps the
native popup lifecycle. The command palette uses virtual focus from `Autocomplete` with scrolling on its `Menu`; dialog
and select labels belong to their native parent composition.

Selection controls compose native fields and buttons with labelled content. Use `isSelected`, `isReadOnly`, `isDisabled`
and `isPending` for their native states. Disclosure retains filter drafts while collapsed; ingestion mounts query
children only while expanded. Book tabs activate from arrow keys and mount only the selected panel. Buttons styled as
navigation links use React Router Link with `buttonVariants`.

Refine dismissal retains valid filter edits for Escape, close and outside clicks. Columns Reset closes the popover and
returns focus through native dismissal. Settings actions are disabled when pristine; Save stays in the tab order while
pending.

Radix UI remains installed for the unused avatar and scroll-area wrappers. Radix Colors and
`src/styles/themes/primitives.generated.css` remain the colour foundation independently of the interaction library. The
grid engine and its editing, sorting and loading state are unchanged.

## Environment Variables

Configure the development environment using these variables.

**REVERIE_DEV_HOSTS** Vite rejects unknown non-loopback hostnames to prevent DNS rebinding. Loopback hosts work without
configuration. Set `REVERIE_DEV_HOSTS` to a comma-separated list of hostnames when using cloud development environments
like Coder or Codespaces. The `vite-plugins/allowed-hosts.ts` script parses this value. The `vite-plugins/dev-csp.ts`
script adds each hostname to the `connect-src` policy, allowing the HMR websocket to connect through TLS. Format
hostnames without schemes, paths, whitespace, or semicolons.

**REVERIE_DEV_HMR_CLIENT_PORT** The HMR websocket client defaults to port 5173. Set `REVERIE_DEV_HMR_CLIENT_PORT` to 443
when a reverse proxy fronts the dev server. The `vite-plugins/hmr-config.ts` script reads this value.

## Project Structure

```text
frontend/
├── public/              # Static assets
├── src/
│   ├── api/             # API client functions
│   ├── components/      # Reusable UI components
│   │   └── ui/          # Token-styled interaction primitives
│   ├── fouc/            # Pre-paint script hashed into HTML CSP at build
│   ├── hooks/           # Custom React hooks
│   ├── pages/           # Route-level page components
│   ├── routes/          # Lazy route modules (incl. pre-auth /login, /setup, /forgot-password)
│   ├── lib/             # Utilities
│   ├── App.tsx          # Root component
│   └── main.tsx         # Entrypoint
├── vite-plugins/        # Custom Vite plugins
├── tests/               # Vitest setup
├── index.html
├── tsconfig.json
└── vite.config.ts       # Tailwind v4 and Vitest configuration
```
