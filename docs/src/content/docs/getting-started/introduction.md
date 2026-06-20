---
title: Introduction
description: What is Reverie and how to get started.
---

Reverie is a self-hosted ebook library manager built with Rust and React.

## Quick Start

> **Pre-alpha note:** No semver release has been cut yet, so the
> conventional `latest` tag on `ghcr.io/unkos-dev/reverie` is
> intentionally unset. Until the first `v0.1.0` ships, only the floating
> `main` tag exists, and it is **`linux/arm64` only**; amd64 users must
> wait for the first semver release. Track
> [Releases](https://github.com/unkos-dev/reverie/releases) for the first
> `vX.Y.Z` tag; once it ships, replace `:main` below with `:vX.Y.Z` (a
> multi-arch manifest).

```bash
docker pull ghcr.io/unkos-dev/reverie:main
```

```bash
docker run -p 3000:3000 ghcr.io/unkos-dev/reverie:main
```

> **Note:** the server checks the database schema at startup and **refuses to
> start** against an uninitialized or out-of-date database. Before the
> `docker run` above you need a Postgres database, the connection environment
> variables, and a one-shot `migrate` step; see
> [Database migrations](https://github.com/unkos-dev/reverie/blob/main/docs/deployment/database-migrations.md)
> for the full startup sequence.
>
> **Note:** Reverie is in pre-alpha. These instructions will be expanded as the project matures.

## Documentation is part of done

Reverie treats reference documentation like tests: it ships with the change, not
after it. The [Configuration](/reverie/reference/configuration/) page and the API
Reference are generated from source (the config schema and the OpenAPI 3.1 spec
respectively) and CI fails when a committed artifact drifts from the code, or
when the docs site stops building. Contributors regenerate with `REGEN=1 cargo
test --test gen_openapi --test gen_config_ref` and commit the result alongside
their change.
