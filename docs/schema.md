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

## Roles and row-level security

The database roles, their grants, every row-level-security policy and the session settings the policies read are
described in the Design
[Row-level security and database context](specs/security/design/0004-row-level-security-and-database-context.md).

## Design Decisions

- **`is_child` / `role` sync**: CHECK constraint `chk_child_role_sync` ensures `is_child = true` only when
  `role = 'child'`. `role` controls both permissions and the row-level-security content filtering; no policy reads
  `is_child`. The two must stay consistent.

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
