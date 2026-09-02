# dev/

Local-development helper scripts and notes that are not part of the
runtime build.

## Scripts

### `seed-library.sh`

Populates `$REVERIE_LIBRARY_ROOT` with a curated set of public-domain
EPUBs from Standard Ebooks for backend integration tests and frontend
onboarding flows. No binaries are committed to the repo — the script
fetches at runtime.

## Environment variables

### `REVERIE_DEV_HOSTS`

Comma-separated list of **non-loopback** hostnames the Vite dev server
will accept in the request `Host` header. Bounds Vite's DNS-rebinding
guard.

Loopback hosts — `localhost`, any `*.localhost` subdomain, and any
IPv4 / IPv6 literal Host header — are accepted unconditionally by a
hardcoded short-circuit in Vite's host-validation middleware,
regardless of this allowlist. The allowlist only matters for
non-loopback hostnames a developer chooses to expose the dev bundle
under (cloud workspace URL, ngrok tunnel, reverse-proxy alias, etc).

```bash
# Local workstation — env var unset. Loopback access works via
# Vite's short-circuit; no other hostnames are accepted.
vp dev

# Cloud dev environment (Coder, Codespaces) — set to the workspace's
# stable hostname so the dev bundle is reachable through it.
REVERIE_DEV_HOSTS=dev.example.com vp dev

# Multiple hostnames — comma-separated.
REVERIE_DEV_HOSTS=dev.example.com,my-tunnel.ngrok.app vp dev
```

Parsing lives in `frontend/vite-plugins/allowed-hosts.ts`; the inline
security comment in `frontend/vite.config.ts` documents the threat
model the allowlist closes.

Setting this variable also affects the dev Content-Security-Policy:
`buildDevCsp` (`frontend/vite-plugins/dev-csp.ts`) adds a `wss://<host>`
origin to `connect-src` for each non-loopback hostname, so the HMR
websocket is permitted through a TLS-terminating edge without a separate
env var. Entries are validated as bare hostnames — a value carrying a
scheme, path, whitespace, or `;` throws at startup rather than corrupting
the CSP header.

### `REVERIE_DEV_HMR_CLIENT_PORT`

Optional integer (1..=65535). Overrides the port the browser uses for
the HMR websocket reconnect. Default (unset) = the dev server's own
port (5173) — correct for localhost / Coder port-forward access.

Required when fronting the dev server with a reverse proxy on a
different external port. Common case: a Cloudflare tunnel terminating
TLS at `dev.example.com` and forwarding to `:5173` inside the
workspace. Without the override the browser tries
`wss://dev.example.com:5173/` and the tunnel does not forward
that port.

```bash
# Cloudflare tunnel scenario — workspace serves on 5173, edge serves
# the bundle on 443. Pair with REVERIE_DEV_HOSTS so Vite accepts the
# external Host header.
REVERIE_DEV_HOSTS=dev.example.com REVERIE_DEV_HMR_CLIENT_PORT=443 vp dev
```

Parsing lives in `frontend/vite-plugins/hmr-config.ts`. Invalid values
(non-integer, out of range) throw at startup with the bad value
echoed.
