# Database Schema

Reverie uses PostgreSQL with a FRBR-inspired data model. **Works** represent abstract
titles; **Manifestations** represent concrete files (EPUBs, PDFs, etc.). This
separation allows multiple editions, formats, and translations to share metadata.

## Entity-Relationship Overview

```text
users ─────────┬──── shelves ──── shelf_items ────┐
               │                                   │
               ├──── user_identities               │
               │                                   │
               ├──── local_credentials             │
               │                                   │
               ├──── device_tokens                 │
               │                                   │
               ├──── user_preferences              │
               │                                   │
               └──── webhooks ──── webhook_deliveries
                                                   │
works ────┬──── work_authors ──── authors           │
          │                                        │
          ├──── series_works ──── series (self-ref) │
          │                                        │
          ├──── omnibus_contents                    │
          │                                        │
          └──── manifestations ◄───────────────────┘
                    │
                    ├──── metadata_versions
                    ├──── manifestation_tags ──── tags
                    ├──── manifestation_genres ──── genres
                    └──── manifestation_moods ──── moods

reading_sessions ──── users, manifestations
reading_positions ──── users, manifestations (reserved)

api_cache          (standalone)
ingestion_jobs     (standalone)
```

## Tables

### Core (FRBR Model)

| Table            | Purpose                    | Key Columns                                                                                                                         |
| ---------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `users`          | Canonical user identity    | `role`, `is_child`, `theme_preference`, `email` (`oidc_subject` is vestigial/nullable; identity resolves through `user_identities`) |
| `works`          | Abstract titles            | `title`, `sort_title`, `search_vector`                                                                                              |
| `authors`        | Author/contributor records | `name`, `sort_name`                                                                                                                 |
| `work_authors`   | Work-Author join (M:N)     | `work_id`, `author_id`, `role`, `position`                                                                                          |
| `manifestations` | Concrete files             | `work_id`, `format`, `file_path`, `ingestion_file_hash`, `current_file_hash`, `validation_status`, `ingestion_status`               |

### Series & Metadata

| Table                  | Purpose                                       | Key Columns                                                                    |
| ---------------------- | --------------------------------------------- | ------------------------------------------------------------------------------ |
| `series`               | Series with self-referential nesting          | `name`, `parent_id`                                                            |
| `series_works`         | Series-Work join                              | `series_id`, `work_id`, `position` (double precision for fractional ordering)  |
| `omnibus_contents`     | Omnibus edition mapping                       | `omnibus_manifestation_id`, `contained_work_id`, `position`                    |
| `metadata_versions`    | Metadata versioning (draft/accepted/rejected) | `manifestation_id`, `source`, `field_name`, `old_value`, `new_value`, `status` |
| `tags`                 | Flat tag vocabulary, unique on `lower(name)`  | `name`                                                                         |
| `manifestation_tags`   | Manifestation-Tag join                        | `manifestation_id`, `tag_id`, `source_version_id`                              |
| `genres`               | Genre vocabulary, unique on `lower(name)`     | `name`                                                                         |
| `manifestation_genres` | Manifestation-Genre join                      | `manifestation_id`, `genre_id`, `source_version_id`                            |
| `moods`                | Mood vocabulary, unique on `lower(name)`      | `name`                                                                         |
| `manifestation_moods`  | Manifestation-Mood join                       | `manifestation_id`, `mood_id`, `source_version_id`                             |

### User Features

| Table              | Purpose                          | Key Columns                                                       |
| ------------------ | -------------------------------- | ----------------------------------------------------------------- |
| `shelves`          | Per-user collections             | `user_id`, `name`, `is_system`                                    |
| `shelf_items`      | Shelf-Manifestation join         | `shelf_id`, `manifestation_id`, `position`                        |
| `device_tokens`    | OPDS/reader device auth          | `user_id`, `token_hash`, `revoked_at`, `scopes`, `expires_at`     |
| `user_preferences` | Per-user library display choices | `user_id` (PK), `hidden_columns`, `density`, `view`, `sort_stack` |

Every override column on `user_preferences` is nullable with no `DEFAULT`:
`NULL` means the account has not customised that group and inherits the
installation default, which the API resolves at read time. Rows are created
lazily on first write, so a fresh account has no row at all.

### Auth & Identity

| Table               | Purpose                                  | Key Columns                                                            |
| ------------------- | ---------------------------------------- | ---------------------------------------------------------------------- |
| `user_identities`   | External-provider identity links         | `user_id`, `provider`, `issuer`, `subject`; `UNIQUE (issuer, subject)` |
| `local_credentials` | Local password credential (one per user) | `user_id` (PK), `password_hash` (Argon2id PHC; secret, app-grant only) |

### System

| Table                | Purpose                                   | Key Columns                                             |
| -------------------- | ----------------------------------------- | ------------------------------------------------------- |
| `api_cache`          | External API response cache               | `source`, `lookup_key`, `response`, `expires_at`        |
| `ingestion_jobs`     | Batch job tracking                        | `batch_id`, `source_path`, `status`                     |
| `writeback_jobs`     | Queue of pending OPF writeback operations | `manifestation_id`, `reason`, `status`, `attempt_count` |
| `webhooks`           | User-configured webhooks                  | `user_id`, `url`, `events`, `enabled`                   |
| `webhook_deliveries` | Delivery log                              | `webhook_id`, `event_type`, `response_status`           |

### Reserved (Phase 2)

| Table               | Purpose                  | Notes                               |
| ------------------- | ------------------------ | ----------------------------------- |
| `reading_sessions`  | Reading session tracking | Empty structure, no logic yet       |
| `reading_positions` | Reader position sync     | Has `updated_at` but no trigger yet |

## Enum Types

| Type                     | Values                                          | Used By                            |
| ------------------------ | ----------------------------------------------- | ---------------------------------- |
| `user_role`              | admin, adult, child                             | `users.role`                       |
| `identity_provider`      | oidc                                            | `user_identities.provider`         |
| `scope`                  | read, write, admin                              | `device_tokens.scopes`             |
| `author_role`            | author, editor, translator, narrator            | `work_authors.role`                |
| `manifestation_format`   | epub, pdf, mobi, azw3, cbz, cbr                 | `manifestations.format`            |
| `validation_status`      | pending, clean, repaired, degraded              | `manifestations.validation_status` |
| `ingestion_status`       | pending, processing, complete, failed, skipped  | `manifestations.ingestion_status`  |
| `metadata_source`        | opf, openlibrary, googlebooks, manual, ai       | `metadata_versions.source`         |
| `metadata_review_status` | draft, accepted, rejected                       | `metadata_versions.status`         |
| `content_rating`         | everyone, teen, mature, adult, explicit         | `manifestations.content_rating`    |
| `job_status`             | queued, running, complete, failed               | `ingestion_jobs.status`            |
| `writeback_status`       | pending, in_progress, complete, failed, skipped | `writeback_jobs.status`            |
| `library_density`        | comfortable, compact                            | `user_preferences.density`         |
| `library_view`           | grid, table                                     | `user_preferences.view`            |

**Note:** `ingestion_status` tracks per-file lifecycle on manifestations.
`job_status` tracks batch orchestration on ingestion_jobs. These are intentionally
separate, as a job can fail while individual files succeeded, and vice versa.

## Database Role Architecture

| Role                | Purpose                              | Privileges                                                            | RLS                              |
| ------------------- | ------------------------------------ | --------------------------------------------------------------------- | -------------------------------- |
| `reverie`           | Cluster bootstrap — provisions roles | Superuser; not used at runtime or for migrations                      | Bypasses (superuser)             |
| `reverie_migrator`  | Runs migrations (`reverie migrate`)  | CREATE on database + schema `public`; owns created objects            | Enforced — NOBYPASSRLS           |
| `reverie_app`       | Web app, OPDS, webhooks              | DML on all tables                                                     | Enforced — user-scoped           |
| `reverie_ingestion` | Background pipeline                  | DML on pipeline tables only                                           | Own permissive policy            |
| `reverie_readonly`  | Debugging, reporting                 | SELECT on most tables (excludes `device_tokens`, `local_credentials`) | Enforced — same as `reverie_app` |

Migrations run as the dedicated least-privilege `reverie_migrator`
(`NOSUPERUSER NOCREATEROLE NOBYPASSRLS`), **not** the cluster superuser.
This keeps cluster-wide authority out of the schema-management path: the
migrator can create and own schema objects but cannot create roles, alter
the server, or bypass row-level security. The application process holds no
migration credential at all on the default path; see
[Database migrations](deployment/database-migrations.md).

### `reverie_ingestion` Access Scope

Has DML on: `works`, `authors`, `work_authors`, `manifestations`, `series`,
`series_works`, `omnibus_contents`, `metadata_versions`, `tags`, `manifestation_tags`,
`genres`, `manifestation_genres`, `moods`, `manifestation_moods`, `api_cache`,
`ingestion_jobs`.

Denied: `users`, `user_identities`, `local_credentials`, `shelves`, `shelf_items`,
`device_tokens`, `user_preferences`, `webhooks`, `webhook_deliveries`,
`reading_sessions`, `reading_positions`.

## Row Level Security (RLS)

### `manifestations`

Six per-operation policies control access:

| Policy                                 | Operation | Roles                             | Logic                            |
| -------------------------------------- | --------- | --------------------------------- | -------------------------------- |
| `manifestations_select_adult`          | SELECT    | `reverie_app`, `reverie_readonly` | Adults/admins see all            |
| `manifestations_select_child`          | SELECT    | `reverie_app`, `reverie_readonly` | Children see shelf-assigned only |
| `manifestations_insert`                | INSERT    | `reverie_app`                     | Unrestricted (WITH CHECK true)   |
| `manifestations_update`                | UPDATE    | `reverie_app`                     | Admin/adult only                 |
| `manifestations_delete`                | DELETE    | `reverie_app`                     | Admin/adult only                 |
| `manifestations_ingestion_full_access` | ALL       | `reverie_ingestion`               | Unconditional access             |

Children cannot UPDATE or DELETE manifestations: these are shared library records.
Children manage their visibility through `shelf_items` instead.

### Owner-scoped tables

Tables holding one row per user carry a single `ALL` policy keyed on the
session variable, matching `user_id` in both `USING` and `WITH CHECK` so a
caller can neither read nor write another account's row:

| Policy                   | Operation | Roles                             | Logic                                  |
| ------------------------ | --------- | --------------------------------- | -------------------------------------- |
| `reading_state_owner`    | ALL       | `reverie_app`, `reverie_readonly` | `user_id` equals `app.current_user_id` |
| `user_preferences_owner` | ALL       | `reverie_app`, `reverie_readonly` | `user_id` equals `app.current_user_id` |

`reverie_ingestion` holds no grant on either table, so the pipeline cannot
reach personal rows at all.

### Session Variable Contract

`reverie_app` and `reverie_readonly` must set the user ID in a transaction:

```sql
BEGIN;
SELECT set_config('app.current_user_id', $1::text, true);
-- queries here see RLS-filtered rows
COMMIT;
```

`SET LOCAL` (the `true` parameter) is transaction-scoped and auto-resets on
commit/rollback, which is safe with connection pools. If the variable is not set,
`current_setting('app.current_user_id', true)` returns NULL, and `NULL::uuid`
causes all visibility checks to fail, so queries return zero rows.

## Design Decisions

- **`is_child` / `role` sync**: CHECK constraint `chk_child_role_sync` ensures
  `is_child = true` only when `role = 'child'`. `role` controls permissions;
  `is_child` drives content filtering (RLS). They must stay consistent.

- **`sort_title` / `sort_name`**: Separate columns strip leading articles for display
  ordering. Application logic populates these on insert.

- **`position double precision`** in `series_works`: Allows fractional ordering
  (e.g., 1.5 for novellas between volumes 1 and 2). Matches the f64 the API
  serves and every writer binds, so no cast sits between storage and decode.

- **Self-referential `series.parent_id`**: Uses `ON DELETE SET NULL` to orphan children
  rather than cascade-delete entire series trees.

- **`updated_at` triggers**: Active on `users`, `works`, `manifestations`. Reserved
  table `reading_positions` has the column but no trigger yet, add via the reusable
  `set_updated_at()` function when activated.

- **pgvector**: Reserved as a SQL comment in migration 7. When ready, create a new
  migration to add the extension, column, and index.

## Naming Convention

All identifiers use `snake_case`. No hyphens anywhere, enum values, column names,
table names all use underscores (e.g., `sub_genre` not `sub-genre`).
