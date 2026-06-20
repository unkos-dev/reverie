---
severity: low
surfaces: [developer, server-operator]
adopted: 2026-05-05
adopted-because: Coder workspace's shared-postgres container occupies host port 5432; project compose used 5433 to coexist; recognised as debt 2026-05-05
lift-when-class: external-standard
lift-when: the postgres port revert task (revert to OSS-convention 5432) merged to main
---

# Dev postgres host port 5433

## Constraint

The Coder workspace where the project was originally developed runs a
`shared-postgres` container that maps the postgres protocol to host
port `5432`. When `docker/compose.dev.yml` was written for the project's
own dev postgres, mapping to `5432` would have collided with the
workspace's existing service. `5433` was chosen to coexist.

The constraint is workspace-specific. It does not apply to anyone
running Reverie outside that specific Coder workspace setup.

## Workaround

`docker/compose.dev.yml`:

```yaml
services:
  postgres:
    ports:
      - "5433:5432"
```

Connection string in `backend/CLAUDE.md` and other dev docs:

> `postgres://reverie:reverie@localhost:5433/reverie_dev`

Self-hosters cloning the repo and running `docker compose up` get a
non-standard port for no reason that helps them.

## Why this isn't the right shape

OSS Postgres dev environments overwhelmingly use `5432` on the host
(Supabase, Hasura, PostgREST, Phoenix / Rails / Django templates). Anyone
running their own local Postgres knows how to handle the port
collision (stop the conflicting service, or override compose
externally). Picking a non-default port to dodge a conflict that only
affects one specific workspace setup leaks workspace concerns into a
public-facing repo.

The right shape:

- Project compose uses `5432:5432` (OSS convention).
- Workspace-specific overrides live workspace-side, outside the
  project working tree. The Coder workspace template can generate a
  `~/.config/coder/reverie-compose-override.yml` mapping the host
  port to whatever it wants and export
  `COMPOSE_FILE=docker/compose.dev.yml:~/.config/coder/reverie-compose-override.yml`
  in the shell init. Project repo never sees the override.

## Lift conditions

The postgres port revert task: revert host port
to `5432`, update connection-string examples, add a one-line README
note for the host-collision case. Cross-repo coordination with the
Coder workspace template (homelab repo) for the workspace-side
override file generation.

When that PR merges:

1. Flip this entry to `status: lifted`, set `lifted`, set
   `superseded-by`.
2. Verify Coder workspace continues to work via the homelab-side
   override (cross-repo handoff).

## Related

- The postgres port revert (lift trigger)
- `docker/compose.dev.yml`: workaround site
- `backend/CLAUDE.md`: connection-string examples that will need
  updating to `5432`
