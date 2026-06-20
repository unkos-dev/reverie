# Database migrations

Reverie manages its schema with embedded, forward-only migrations. How those
migrations _run_ is deliberately separated from the long-lived application
process so that an internet-exposed server never carries schema-management
credentials.

## Two identities, one principle

| Identity           | Holds the migration credential? | Privileges                                                                       |
| ------------------ | ------------------------------- | -------------------------------------------------------------------------------- |
| `reverie_migrator` | Yes — only while migrating      | `NOSUPERUSER NOCREATEROLE NOBYPASSRLS`; CREATE on the database + schema `public` |
| `reverie_app`      | No                              | DML under row-level security; `SELECT` on migration history only                 |

Migrations run as `reverie_migrator`, a dedicated least-privilege role. It can
create and own schema objects but cannot provision roles, alter the server, or
bypass RLS. The application connects only as `reverie_app`, which has no
schema-management rights and, on the default topology, never sees
`DATABASE_URL_MIGRATION`.

## Running migrations: the default (out-of-band)

The shipped model runs migrations as a one-shot step **before** the server
starts, using the `migrate` subcommand:

```bash
reverie migrate
```

This reads `DATABASE_URL_MIGRATION` (the `reverie_migrator` DSN), applies any
pending migrations, and exits. It does not read the application config; a
migrate step needs no OIDC secret or application DSN. A non-zero exit means
migration failed; the error names the failure mode and recovery.

In local development:

```bash
cargo run -- migrate
```

## What the server does on startup

With `REVERIE_AUTO_MIGRATE` unset (the default), the server **does not migrate**.
Instead it performs a read-only check that the database schema matches the
binary, using its own `reverie_app` pool, and **refuses to start** if they
diverge. The check is fail-closed in both directions:

- **Database behind the binary** (the common "forgot to run `reverie migrate`"
  case): the server refuses rather than serving against a schema missing
  tables or columns it expects, which would otherwise surface as opaque
  runtime errors.
- **Database ahead of the binary**: the server refuses rather than writing
  against a schema it does not understand.
- **Never migrated** (no migration history at all): the server reports that
  the database is not initialized and points you at `reverie migrate`, instead
  of emitting a cryptic missing-table error.

Refusing on a behind schema is intentional. For an exposed multi-user instance
there is no legitimate window where the application should run against a schema
older than itself, so divergence is surfaced as a single legible startup error
rather than a stream of failed requests.

## Bare `docker` (no orchestration): two steps

If you run the image directly without a one-shot migrate service, migrate
first, then start the server:

> Pre-v0.1.0 the conventional `latest` tag is intentionally unset. These
> examples pin `:main` (the floating staging tag, **`linux/arm64` only** until
> the first amd64 multi-arch release ships). Once `v0.1.0` is tagged, swap
> `:main` for `:vX.Y.Z`.

```bash
docker run --rm --env-file .env.migrate ghcr.io/unkos-dev/reverie:main migrate
```

```bash
docker run -d --env-file .env.runtime -p 3000:3000 ghcr.io/unkos-dev/reverie:main
```

The second command starts the server, which verifies the schema and serves.

The two steps deliberately use **separate env files**, mirroring the compose
split (`.env.migrate` / `.env.runtime`): `.env.migrate` carries only
`DATABASE_URL_MIGRATION` (the `reverie_migrator` DSN), and `.env.runtime`
carries the application configuration (`DATABASE_URL`, OIDC settings, …)
without the migration DSN. A single combined `.env` passed to both commands
works, but it hands the long-lived server the `reverie_migrator` credential
for its entire lifetime, exactly the exposure the two-identity model above
exists to avoid.

## Opt-in: in-process migration on startup

Setting `REVERIE_AUTO_MIGRATE=true` makes the server run migrations itself at
startup instead of verifying. This is an escape hatch, not the recommended
path: the long-lived server process then holds `DATABASE_URL_MIGRATION` (and
therefore the `reverie_migrator` credential) for its entire lifetime. When the
flag is true, `DATABASE_URL_MIGRATION` is required and the server fails to
start without it.

Use it only when a separate migrate step is impractical (for example, a
single-container deployment with no orchestration to sequence a migrate step
before the server). Prefer the out-of-band `reverie migrate` step everywhere
else.
