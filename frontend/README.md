# Frontend

This directory contains the Vite React frontend.

## Environment Variables

Configure the development environment using these variables.

**REVERIE_DEV_HOSTS**
Vite rejects unknown non-loopback hostnames to prevent DNS rebinding. Loopback hosts work without configuration. Set `REVERIE_DEV_HOSTS` to a comma-separated list of hostnames when using cloud development environments like Coder or Codespaces. The `vite-plugins/allowed-hosts.ts` script parses this value. The `vite-plugins/dev-csp.ts` script adds each hostname to the `connect-src` policy, allowing the HMR websocket to connect through TLS. Format hostnames without schemes, paths, whitespace, or semicolons.

**REVERIE_DEV_HMR_CLIENT_PORT**
The HMR websocket client defaults to port 5173. Set `REVERIE_DEV_HMR_CLIENT_PORT` to 443 when a reverse proxy fronts the dev server. The `vite-plugins/hmr-config.ts` script reads this value.

## Project Structure

```text
frontend/
├── public/              # Static assets
├── src/
│   ├── api/             # API client functions
│   ├── components/      # Reusable UI components
│   │   └── ui/          # generated shadcn/ui components
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
