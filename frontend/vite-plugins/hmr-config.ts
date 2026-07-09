// When the dev server is fronted by a reverse proxy on a different
// external port (e.g. a Cloudflare tunnel terminating TLS on 443 for
// the bundle Vite serves on 5173), the browser must reconnect HMR via
// the proxy port — otherwise it tries `wss://<host>:5173/` and the
// tunnel does not forward it. REVERIE_DEV_HMR_CLIENT_PORT carries that
// override value.
/**
 * Parse `REVERIE_DEV_HMR_CLIENT_PORT` into the partial Vite `server` config
 * the dev server merges with its own defaults. Empty / unset / whitespace
 * resolves to an empty object so the caller can spread the result
 * unconditionally without branching on the env var being present.
 *
 * Validation is strict on purpose: the env var ends up in a websocket URL
 * the browser dials, and silently coercing junk would turn into a
 * connection-refused loop that is hard to diagnose from the browser
 * console. The error message JSON-encodes the original value so quoting
 * artefacts (a trailing newline, a stray whitespace character) survive to
 * the operator's terminal verbatim.
 *
 * @param envValue - Raw `REVERIE_DEV_HMR_CLIENT_PORT` value or undefined.
 * @returns Empty object when the env var is unset; `{ hmr: { clientPort } }`
 *   when a valid port literal is supplied.
 * @throws Error if the value is non-empty but is not an integer in
 *   1..=65535.
 */
export function parseHmrConfig(envValue: string | undefined): { hmr?: { clientPort: number } } {
  if (envValue === undefined || envValue.trim().length === 0) return {};
  const trimmed = envValue.trim();
  const invalid = new Error(
    `REVERIE_DEV_HMR_CLIENT_PORT must be an integer in 1..=65535, got ${JSON.stringify(envValue)}`,
  );
  if (!/^\d+$/.test(trimmed)) throw invalid;
  const port = Number(trimmed);
  if (port < 1 || port > 65535) throw invalid;
  return { hmr: { clientPort: port } };
}
