# Database Schema

Reverie uses PostgreSQL with a FRBR-inspired data model. **Works** represent abstract titles; **Manifestations**
represent concrete files (EPUBs, PDFs, etc.). This separation allows multiple editions, formats, and translations to
share metadata.

## Tables and types

[`backend/schema.sql`](../backend/schema.sql) is the full schema: every table, column, enum type, constraint, index,
function, trigger, grant and policy, as `pg_dump --schema-only` prints it for a database with every migration applied.
CI rebuilds it from the migrations and fails when the committed file differs.

Every override column on `user_preferences` is nullable with no `DEFAULT`: `NULL` means the account has not customised
that group and inherits the installation default, which the API resolves at read time. Rows are created lazily on first
write, so a fresh account has no row at all.

**Note:** `ingestion_status` tracks per-file lifecycle on manifestations. `job_status` tracks batch orchestration on
`ingestion_jobs`. These are intentionally separate, as a job can fail while individual files succeeded, and vice versa.

## Database Role Architecture

| Role | Purpose | Privileges | RLS |
| ---- | ------- | ---------- | --- |
| `reverie` | Cluster bootstrap — provisions roles | Superuser; not used at runtime or for migrations | Bypasses (superuser) |
| `reverie_migrator` | Runs migrations (`reverie migrate`) | CREATE on database + schema `public`; owns created objects | Enforced — NOBYPASSRLS |
| `reverie_app` | Web app and OPDS | DML on most tables; exceptions below | Enforced — user-scoped |
| `reverie_ingestion` | Background pipeline | DML on pipeline tables only | Own permissive policy |
| `reverie_readonly` | Debugging, reporting | SELECT on most tables (excludes `device_tokens`, `local_credentials`) | Enforced — same as `reverie_app` |

`reverie_app` holds `SELECT` only on `identifier_schemes`, `metadata_sources`, `rating_sources` and
`manifestation_external_ratings`. On `settings` it holds `SELECT` and `UPDATE`, on `instance_bootstrap` `SELECT` and
`INSERT`, and on `user_preferences` everything except `DELETE`.

Migrations run as the dedicated least-privilege `reverie_migrator` (`NOSUPERUSER NOCREATEROLE NOBYPASSRLS`), **not** the
cluster superuser. This keeps cluster-wide authority out of the schema-management path: the migrator can create and own
schema objects but cannot create roles, alter the server, or bypass row-level security. The application process holds no
migration credential at all on the default path; see
[Database migrations](deployment/database-migrations.md).

### `reverie_ingestion` Access Scope

Has DML on: `works`, `authors`, `work_authors`, `manifestations`, `series`, `series_works`, `omnibus_contents`,
`metadata_versions`, `tags`, `manifestation_tags`, `genres`, `manifestation_genres`, `moods`, `manifestation_moods`,
`api_cache`, `ingestion_jobs`.

Denied: `users`, `user_identities`, `local_credentials`, `shelves`, `shelf_items`, `device_tokens`, `user_preferences`,
`webhooks`, `webhook_deliveries`, `reading_sessions`.

## Row Level Security (RLS)

### `manifestations`

Six per-operation policies control access:

| Policy | Operation | Roles | Logic |
| ------ | --------- | ----- | ----- |
| `manifestations_select_adult` | SELECT | `reverie_app`, `reverie_readonly` | Adults/admins see all |
| `manifestations_select_child` | SELECT | `reverie_app`, `reverie_readonly` | Children see shelf-assigned only |
| `manifestations_insert` | INSERT | `reverie_app` | Unrestricted (WITH CHECK true) |
| `manifestations_update` | UPDATE | `reverie_app` | Admin/adult only |
| `manifestations_delete` | DELETE | `reverie_app` | Admin/adult only |
| `manifestations_ingestion_full_access` | ALL | `reverie_ingestion` | Unconditional access |

Children cannot UPDATE or DELETE manifestations: these are shared library records. Children manage their visibility
through `shelf_items` instead.

### `manifestation_genres`, `manifestation_moods`, `manifestation_tags`

Each junction table carries the same five policies, scoped through `manifestations` visibility rather than a direct role
or shelf check, so a session that reads or writes the junction table directly gets the same authorization result as a
session that joins through `manifestations`:

| Policy | Operation | Roles | Logic |
| ------ | --------- | ----- | ----- |
| `<table>_select` | SELECT | `reverie_app`, `reverie_readonly` | Visible iff the linked manifestation is visible |
| `<table>_insert` | INSERT | `reverie_app` | Admin/adult, and the manifestation is visible |
| `<table>_update` | UPDATE | `reverie_app` | Admin/adult, and the manifestation is visible |
| `<table>_delete` | DELETE | `reverie_app` | Admin/adult, and the manifestation is visible |
| `<table>_ingestion_full_access` | ALL | `reverie_ingestion` | Unconditional access |

The vocabulary tables (`genres`, `moods`, `tags`) hold no per-manifestation data and carry no RLS.

### Owner-scoped tables

Tables holding one row per user carry a single `ALL` policy keyed on the session variable, matching `user_id` in both
`USING` and `WITH CHECK` so a caller can neither read nor write another account's row:

| Policy                   | Operation | Roles                             | Logic                                  |
| ------------------------ | --------- | --------------------------------- | -------------------------------------- |
| `reading_state_owner`    | ALL       | `reverie_app`, `reverie_readonly` | `user_id` equals `app.current_user_id` |
| `user_preferences_owner` | ALL       | `reverie_app`, `reverie_readonly` | `user_id` equals `app.current_user_id` |

`reverie_ingestion` holds no grant on either table, so the pipeline cannot reach personal rows at all.

### Session Variable Contract

`reverie_app` and `reverie_readonly` must set the user ID in a transaction:

```sql
BEGIN;
SELECT set_config('app.current_user_id', $1::text, true);
-- queries here see RLS-filtered rows
COMMIT;
```

`SET LOCAL` (the `true` parameter) is transaction-scoped and auto-resets on commit/rollback, which is safe with
connection pools. If the variable is not set, `current_setting('app.current_user_id', true)` returns NULL, and
`NULL::uuid` causes all visibility checks to fail, so queries return zero rows.

## Design Decisions

- **`is_child` / `role` sync**: CHECK constraint `chk_child_role_sync` ensures `is_child = true` only when
  `role = 'child'`. `role` controls permissions; `is_child` drives content filtering (RLS). They must stay consistent.

- **`sort_title` / `sort_name`**: Separate columns strip leading articles for display ordering. Application logic
  populates these on insert.

- **`position double precision`** in `series_works`: Allows fractional ordering (e.g., 1.5 for novellas between volumes
  1 and 2). Matches the f64 the API serves and every writer binds, so no cast sits between storage and decode.

- **Self-referential `series.parent_id`**: Uses `ON DELETE SET NULL` to orphan children rather than cascade-delete
  entire series trees.

- **`updated_at` triggers**: Active on `users`, `works`, `manifestations`.

- **pgvector**: Reserved as a SQL comment in migration 7. When ready, create a new migration to add the extension,
  column, and index.

## Naming Convention

All identifiers use `snake_case`. No hyphens anywhere, enum values, column names, table names all use underscores (e.g.,
`sub_genre` not `sub-genre`).
