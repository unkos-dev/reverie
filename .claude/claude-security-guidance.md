# Reverie Security Guidance

Reverie is an open-source, self-hosted ebook library manager. Threat model:
multi-user instance exposed to the internet, not a private homelab deploy.

## RLS & database access

- Three role-scoped pools: `reverie_app` (user-facing), `reverie_ingestion`
  (background pipeline), `reverie_readonly` (reporting). Migration pool
  (`reverie` schema owner) is ephemeral and drops before runtime.
- Every user-facing query MUST go through `db::acquire_with_rls(pool, user_id)`.
  Bare `pool.acquire()` in request handlers bypasses RLS — always flag this.
- User context set via `set_config('app.current_user_id', uuid, true)` with
  LOCAL scope (auto-resets on commit/rollback). Missing set_config = RLS bypass.
- Writeback worker uses a dedicated pool with `app.system_context = 'writeback'`.
  Only `manifestations_*_system` policies match this GUC. Cross-pool usage is a bug.
- `tower_sessions` schema is RLS-exempt (session bootstraps user resolution).

## OIDC & sessions

- `OidcCredentials` must only be constructed from verified ID tokens (signature +
  claims validated by `openidconnect` crate). Never from unverified responses.
- Session cookies: `HttpOnly=true`, `SameSite=Lax`. No `Secure` flag (backend
  behind TLS-terminating proxy). Adding `Secure` breaks non-TLS dev setups.
- Session tokens must never appear in logs, error messages, or serialised output.
- Auth enforcement uses `CurrentUser::require_admin()` / `require_not_child()`.
  Direct field reads on role/is_child bypass the canonical check — flag them.

## Outbound HTTP

- EVERY `reqwest::ClientBuilder` in production code MUST call `.user_agent(...)`.
  `Client::new()` sends no User-Agent; Cloudflare WAFs 403 on empty UA.
  Minimum value: `reverie/<version>` via `concat!("reverie/", env!("CARGO_PKG_VERSION"))`.
- Cover-fetch client has SSRF guard denying redirects to private ranges (127/8,
  10/8, 172.16/12, 192.168/16, 169.254/16, 100.64/10). New outbound clients
  fetching user-influenced URLs must replicate this guard.

## EPUB & XML parsing

- `quick-xml` reader: no DTD processing, no entity expansion. Changing these
  defaults opens XXE surface — flag any `expand_empty_elements` or custom entity
  configuration.
- ZIP extraction enforces uncompressed size limits (zip-bomb prevention). Path
  entries validated: no `../` traversal, no absolute paths.
- All DB-sourced strings must pass `sanitise_xml_text()` before XML serialisation
  (strips control codepoints outside XML 1.0 Char production). Skipping this
  breaks strict OPDS clients.

## Secrets

- Never surface decrypted secret values. Describe presence by source/length/format
  only. This applies to env vars, API keys, OIDC client secrets, DB passwords,
  session cookies, and Bearer tokens.
- Error messages and log lines must not interpolate secret values. Use
  redacted placeholders or omit entirely.
