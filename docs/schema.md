# Database Schema

Reverie uses PostgreSQL with a FRBR-inspired data model. **Works** represent abstract titles; **Manifestations**
represent concrete files (EPUBs, PDFs, etc.). This separation allows multiple editions, formats, and translations to
share metadata.

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

api_cache          (standalone)
ingestion_jobs     (standalone)
```

## Tables

### Core (FRBR Model)

| Table | Purpose | Key Columns |
| ----- | ------- | ----------- |
| `users` | Canonical user identity | `role`, `is_child`, `theme_preference`, `email` (`oidc_subject` is vestigial/nullable; identity resolves through `user_identities`) |
| `works` | Abstract titles | `title`, `sort_title`, `search_vector` |
| `authors` | Author/contributor records | `name`, `sort_name` |
| `work_authors` | Work-Author join (M:N) | `work_id`, `author_id`, `role`, `position` |
| `manifestations` | Concrete files | `work_id`, `format`, `file_path`, `ingestion_file_hash`, `current_file_hash`, `validation_status`, `ingestion_status` |

### Series & Metadata

| Table | Purpose | Key Columns |
| ----- | ------- | ----------- |
| `series` | Series with self-referential nesting | `name`, `parent_id` |
| `series_works` | Series-Work join | `series_id`, `work_id`, `position` (double precision for fractional ordering) |
| `omnibus_contents` | Omnibus edition mapping | `omnibus_manifestation_id`, `contained_work_id`, `position` |
| `metadata_versions` | Per-field metadata versions (`pending` or `rejected`; accepting one moves the canonical field's version pointer) | `manifestation_id`, `source`, `field_name`, `old_value`, `new_value`, `status` |
| `metadata_sources` | Metadata source registry, seeded by the initial migration | `id`, `display_name`, `kind`, `enabled`, `base_priority`; `metadata_versions.source` references `id` |
| `tags` | Flat tag vocabulary, unique on `lower(name)` | `name` |
| `manifestation_tags` | Manifestation-Tag join | `manifestation_id`, `tag_id`, `source_version_id` |
| `genres` | Genre vocabulary, unique on `lower(name)` | `name` |
| `manifestation_genres` | Manifestation-Genre join | `manifestation_id`, `genre_id`, `source_version_id` |
| `moods` | Mood vocabulary, unique on `lower(name)` | `name` |
| `manifestation_moods` | Manifestation-Mood join | `manifestation_id`, `mood_id`, `source_version_id` |

### User Features

| Table | Purpose | Key Columns |
| ----- | ------- | ----------- |
| `shelves` | Per-user collections | `user_id`, `name`, `is_system` |
| `shelf_items` | Shelf-Manifestation join | `shelf_id`, `manifestation_id`, `position` |
| `device_tokens` | OPDS/reader device auth | `user_id`, `token_hash`, `revoked_at`, `scopes`, `expires_at` |
| `user_preferences` | Per-user library display choices | `user_id` (PK), `hidden_columns`, `density`, `view`, `sort_stack` |

Every override column on `user_preferences` is nullable with no `DEFAULT`: `NULL` means the account has not customised
that group and inherits the installation default, which the API resolves at read time. Rows are created lazily on first
write, so a fresh account has no row at all.

### Auth & Identity

| Table | Purpose | Key Columns |
| ----- | ------- | ----------- |
| `user_identities` | External-provider identity links | `user_id`, `provider`, `issuer`, `subject`; `UNIQUE (issuer, subject)` |
| `local_credentials` | Local password credential (one per user) | `user_id` (PK), `password_hash` (Argon2id PHC; secret, app-grant only) |

### System

| Table | Purpose | Key Columns |
| ----- | ------- | ----------- |
| `api_cache` | External API response cache | `source`, `lookup_key`, `response`, `expires_at` |
| `ingestion_jobs` | Batch job tracking | `batch_id`, `source_path`, `status` |
| `writeback_jobs` | Queue of pending OPF writeback operations | `manifestation_id`, `reason`, `status`, `attempt_count` |

### Reserved (Phase 2)

| Table                | Purpose                  | Notes                                         |
| -------------------- | ------------------------ | --------------------------------------------- |
| `reading_sessions`   | Reading session tracking | Empty structure, no logic yet                 |
| `webhooks`           | User-configured webhooks | RLS enabled with no policies; no handlers yet |
| `webhook_deliveries` | Webhook delivery log     | RLS enabled with no policies; no handlers yet |

## Enum Types

| Type                     | Values                                              | Used By                            |
| ------------------------ | --------------------------------------------------- | ---------------------------------- |
| `user_role`              | admin, adult, child                                 | `users.role`                       |
| `theme_preference`       | system, light, dark                                 | `users.theme_preference`           |
| `identity_provider`      | oidc                                                | `user_identities.provider`         |
| `scope`                  | read, write, admin                                  | `device_tokens.scopes`             |
| `author_role`            | author, editor, translator, narrator                | `work_authors.role`                |
| `manifestation_format`   | epub, pdf, mobi, azw3, cbz, cbr                     | `manifestations.format`            |
| `validation_status`      | pending, clean, repaired, degraded, failed          | `manifestations.validation_status` |
| `ingestion_status`       | pending, processing, complete, failed, skipped      | `manifestations.ingestion_status`  |
| `enrichment_status`      | pending, in_progress, complete, failed, skipped     | `manifestations.enrichment_status` |
| `metadata_review_status` | pending, rejected                                   | `metadata_versions.status`         |
| `content_rating`         | everyone, teen, mature, adult, explicit             | `manifestations.content_rating`    |
| `job_status`             | queued, running, complete, failed, skipped          | `ingestion_jobs.status`            |
| `writeback_status`       | pending, in_progress, complete, failed, skipped     | `writeback_jobs.status`            |
| `api_cache_kind`         | hit, miss, error                                    | `api_cache.response_kind`          |
| `reading_status`         | want_to_read, reading, on_hold, finished, abandoned | `reading_state.status`             |
| `library_density`        | comfortable, compact                                | `user_preferences.density`         |
| `library_view`           | grid, table                                         | `user_preferences.view`            |

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
