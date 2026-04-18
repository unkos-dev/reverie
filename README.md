# Reverie

A high-performance, self-hosted ebook library manager.

[![CI](https://github.com/unkos-dev/reverie/actions/workflows/ci.yml/badge.svg)](https://github.com/unkos-dev/reverie/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

> **Status:** Pre-alpha. Under active development.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust + Axum |
| Frontend | React + Vite + TypeScript |
| Styling | Tailwind CSS + shadcn/ui |
| Database | PostgreSQL |

## Development

```bash
# Backend
cd backend && cargo run
```

```bash
# Frontend
cd frontend && npm install && npm run dev
```

```bash
# Docker (full stack)
docker compose up
```

## License

This project is licensed under the [GNU Affero General Public License v3.0](LICENSE).

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution terms.
