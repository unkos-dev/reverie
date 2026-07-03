<!-- markdownlint-disable-next-line MD041 -- brand lockup header, no h1 -->
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend/public/brand/lockup/lockup-on-dark.svg">
    <img src="frontend/public/brand/lockup/lockup-on-light.svg" alt="Reverie" width="340">
  </picture>
</div>

<p align="center">A self-hosted ebook library manager, built in Rust.</p>

<p align="center">
  <a href="https://github.com/unkos-dev/reverie/actions/workflows/ci.yml"><img src="https://github.com/unkos-dev/reverie/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/unkos-dev/reverie/actions/workflows/codeql.yml"><img src="https://github.com/unkos-dev/reverie/actions/workflows/codeql.yml/badge.svg" alt="CodeQL"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/unkos-dev/reverie"><img src="https://api.securityscorecards.dev/projects/github.com/unkos-dev/reverie/badge" alt="OpenSSF Scorecard"></a>
  <a href="https://www.bestpractices.dev/projects/13071"><img src="https://www.bestpractices.dev/projects/13071/badge" alt="OpenSSF Best Practices"></a>
  <a href="https://snyk.io/test/github/unkos-dev/reverie"><img src="https://snyk.io/test/github/unkos-dev/reverie/badge.svg" alt="Known Vulnerabilities"></a>
  <a href="https://sonarcloud.io/summary/new_code?id=unkos-dev_reverie"><img src="https://sonarcloud.io/api/project_badges/measure?project=unkos-dev_reverie&metric=coverage" alt="Coverage"></a>
  <a href="https://www.gnu.org/licenses/agpl-3.0"><img src="https://img.shields.io/badge/License-AGPL--3.0-blue.svg" alt="License: AGPL-3.0"></a>
</p>

Reverie is an ebook library manager for self-hosting. It is early in
development: the design is settled and the badges above track the
engineering, but there is no supported install path yet.

> **Status:** Pre-alpha. APIs, schema, and behaviour all change without
> notice.

## Design

These constraints are fixed:

- Source files are read-only. Ingestion copies or hardlinks into a managed
  library and never modifies or deletes an original.
- A book is a work, not a file. Editions and formats of the same title group
  under one catalogue entry.
- Fetched metadata is staged, with its source recorded, until you accept or
  reject it. External sources cannot write to the catalogue directly.
- PostgreSQL row-level security enforces user isolation, rather than
  application-level filtering.
- Child accounts see nothing by default. Access is granted per shelf.
- No telemetry. Reverie sends nothing about you, your library, or your
  deployment anywhere.
- The whole application deploys as one container plus PostgreSQL. EPUB
  processing is pure Rust with no Java dependency, images are published for
  amd64 and arm64, and the idle memory target is under 200 MB.
- Authentication is OIDC with PKCE. OPDS clients and reader apps use hashed
  device tokens.

## Tech stack

| Layer    | Technology                                      |
| -------- | ----------------------------------------------- |
| Backend  | Rust + Axum                                     |
| Frontend | React + Vite + TypeScript, Tailwind + shadcn/ui |
| Database | PostgreSQL                                      |

## Documentation

Guides and reference material live on the
[documentation site](https://unkos-dev.github.io/reverie/). Architectural
decisions are recorded in [`adr/`](adr/).

## Security

Reverie is built on the assumption it will face the public internet. The
backend sends modern browser protection headers on every response, including
a strict `Content-Security-Policy`, and reverse proxies should pass them
through unchanged.

To report a vulnerability, use
[GitHub private advisories](https://github.com/unkos-dev/reverie/security/advisories/new).
Process and response times are in [SECURITY.md](.github/SECURITY.md).

### Verifying image signatures

Published container images are signed with [Sigstore](https://www.sigstore.dev/)
cosign, and each signature is recorded in the public Rekor transparency log.
Images also carry an SBOM and full build-provenance attestations.
Verification confirms an image was built by this repository's release
workflow and has not been altered since:

```bash
cosign verify \
  --certificate-identity-regexp '^https://github.com/unkos-dev/reverie/\.github/workflows/docker-publish\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ghcr.io/unkos-dev/reverie:<tag>
```

## Contributing

[CONTRIBUTING.md](.github/CONTRIBUTING.md) covers development setup,
contribution terms, and the pull request process. The
[Code of Conduct](.github/CODE_OF_CONDUCT.md) applies across the project.

## License

[GNU Affero General Public License v3.0](LICENSE).
