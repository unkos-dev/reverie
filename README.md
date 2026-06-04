# Reverie

A high-performance, self-hosted ebook library manager.

[![CI](https://github.com/unkos-dev/reverie/actions/workflows/ci.yml/badge.svg)](https://github.com/unkos-dev/reverie/actions/workflows/ci.yml)
[![CodeQL](https://github.com/unkos-dev/reverie/actions/workflows/codeql.yml/badge.svg)](https://github.com/unkos-dev/reverie/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/unkos-dev/reverie/badge)](https://scorecard.dev/viewer/?uri=github.com/unkos-dev/reverie)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/13071/badge)](https://www.bestpractices.dev/projects/13071)
[![codecov](https://codecov.io/gh/unkos-dev/reverie/graph/badge.svg)](https://codecov.io/gh/unkos-dev/reverie)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

> **Status:** Pre-alpha. Under active development.

## Tech Stack

| Layer    | Technology                |
| -------- | ------------------------- |
| Backend  | Rust + Axum               |
| Frontend | React + Vite + TypeScript |
| Styling  | Tailwind CSS + shadcn/ui  |
| Database | PostgreSQL                |

## Development

```bash
# Backend
cd backend && cargo run
```

> **Note:** `cargo run` verifies the schema is current but does not migrate; it
> refuses to start on a fresh or behind database. Run `cargo run -- migrate`
> first to initialise/upgrade the schema.

```bash
# Frontend
cd frontend && npm install && npm run dev
```

```bash
# Docker (full stack)
docker compose up
```

> **Upgrading from before postgres:18 mount-layout fix?** The dev volume
> path changed from `pgdata:/var/lib/postgresql/data` to
> `pgdata:/var/lib/postgresql`. Drop the old volume first:
> `docker compose down && docker volume rm reverie_pgdata` (Compose
> prefixes volume names with the project name, which defaults to the
> repo directory; if your checkout is named differently, run
> `docker volume ls | grep pgdata` to find the actual name).

## Security posture

Reverie ships a strict hash-based `Content-Security-Policy`, opt-in HSTS, and
the full Permissions-Policy / X-Content-Type-Options / Referrer-Policy /
X-Frame-Options header set by default. The backend owns all security
response headers — reverse proxies should pass them through unchanged.

Target grade: **A+** on [securityheaders.com](https://securityheaders.com)
and [Mozilla Observatory](https://observatory.mozilla.org) for any
deployment behind TLS.

See [docs/security/content-security-policy.md](docs/security/content-security-policy.md)
for operator configuration (HSTS subdomain behaviour, CSP violation
reporting, dev-vs-prod differences) and
[docs/deployment/reverse-proxy.md](docs/deployment/reverse-proxy.md) for
Caddy / nginx / Traefik samples.

### Verifying image signatures

Every published image is signed with [Sigstore](https://www.sigstore.dev/)
cosign using keyless signing — no long-lived key, with the signature
recorded in the public Rekor transparency log. Verify an image before
pulling it:

```bash
cosign verify \
  --certificate-identity-regexp '^https://github.com/unkos-dev/reverie/\.github/workflows/docker-publish\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ghcr.io/unkos-dev/reverie:<tag>
```

A successful verification confirms the image was built by this repo's
release workflow and has not been tampered with since.

## License

This project is licensed under the [GNU Affero General Public License v3.0](LICENSE).

See [CONTRIBUTING.md](.github/CONTRIBUTING.md) for contribution terms.
