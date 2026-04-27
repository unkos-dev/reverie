# Database Schema

Reverie uses PostgreSQL with a FRBR-inspired data model. **Works** represent abstract
titles; **Manifestations** represent concrete files (EPUBs, PDFs, etc.). This
separation allows multiple editions, formats, and translations to share metadata.

## Entity-Relationship Overview

```text
users ─────────┬──── shelves ──── shelf_items ────┐
               │                                   │
               ├──── device_tokens                 │
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
                    └──── manifestation_tags ──── tags

reading_sessions ──── users, manifestations
reading_positions ──── users, manifestations (reserved)

api_cache          (standalone)
ingestion_jobs     (standalone)
```

## Tables

### Core (FRBR Model)

| Table | Purpose | Key Columns |
|---|---|---|
| `users` | User accounts (OIDC) | `oidc_subject`, `role`, `is_child`, `theme_preference` |
| `works` | Abstract titles | `title`, `sort_title`, `search_vector` |
| `authors` | Author/contributor records | `name`, `sort_name` |
| `work_authors` | Work-Author join (M:N) | `work_id`, `author_id`, `role`, `position` |
| `manifestations` | Concrete files | `work_id`, `format`, `file_path`, `ingestion_file_hash`, `current_file_hash`, `validation_status`, `ingestion_status` |

### Series & Metadata

| Table | Purpose | Key Columns |
|---|---|---|
| `series` | Series with self-referential nesting | `name`, `parent_id` |
| `series_works` | Series-Work join | `series_id`, `work_id`, `position` (NUMERIC for fractional ordering) |
| `omnibus_contents` | Omnibus edition mapping | `omnibus_manifestation_id`, `contained_work_id`, `position` |
| `metadata_versions` | Metadata versioning (draft/accepted/rejected) | `manifestation_id`, `source`, `field_name`, `old_value`, `new_value`, `status` |
| `tags` | Genre, sub-genre, trope, theme tags | `name`, `tag_type` |
| `manifestation_tags` | Manifestation-Tag join | `manifestation_id`, `tag_id` |

### User Features

| Table | Purpose | Key Columns |
|---|---|---|
| `shelves` | Per-user collections | `user_id`, `name`, `is_system` |
| `shelf_items` | Shelf-Manifestation join | `shelf_id`, `manifestation_id`, `position` |
| `device_tokens` | OPDS/reader device auth | `user_id`, `token_hash`, `revoked_at` |

### System

| Table | Purpose | Key Columns |
|---|---|---|
| `api_cache` | External API response cache | `source`, `lookup_key`, `response`, `expires_at` |
| `ingestion_jobs` | Batch job tracking | `batch_id`, `source_path`, `status` |
| `writeback_jobs` | Queue of pending OPF writeback operations | `manifestation_id`, `reason`, `status`, `attempt_count` |
| `webhooks` | User-configured webhooks | `user_id`, `url`, `events`, `enabled` |
| `webhook_deliveries` | Delivery log | `webhook_id`, `event_type`, `response_status` |

### Reserved (Phase 2)

| Table | Purpose | Notes |
|---|---|---|
| `reading_sessions` | Reading session tracking | Empty structure, no logic yet |
| `reading_positions` | Reader position sync | Has `updated_at` but no trigger yet |

## Enum Types

| Type | Values | Used By |
|---|---|---|
| `user_role` | admin, adult, child | `users.role` |
| `author_role` | author, editor, translator, narrator | `work_authors.role` |
| `manifestation_format` | epub, pdf, mobi, azw3, cbz, cbr | `manifestations.format` |
| `validation_status` | pending, valid, invalid, repaired | `manifestations.validation_status` |
| `ingestion_status` | pending, processing, complete, failed, skipped | `manifestations.ingestion_status` |
| `metadata_source` | opf, openlibrary, googlebooks, manual, ai | `metadata_versions.source` |
| `metadata_review_status` | draft, accepted, rejected | `metadata_versions.status` |
| `tag_type` | genre, sub_genre, trope, theme | `tags.tag_type` |
| `job_status` | queued, running, complete, failed | `ingestion_jobs.status` |
| `writeback_status` | pending, in_progress, complete, failed, skipped | `writeback_jobs.status` |

**Note:** `ingestion_status` tracks per-file lifecycle on manifestations.
`job_status` tracks batch orchestration on ingestion_jobs. These are intentionally
separate — a job can fail while individual files succeeded, and vice versa.

## Database Role Architecture

| Role | Purpose | Privileges | RLS |
|---|---|---|---|
| `reverie` | Schema owner, runs migrations | DDL + full DML | Bypasses (owner) |
| `reverie_app` | Web app, OPDS, webhooks | DML on all tables | Enforced — user-scoped |
| `reverie_ingestion` | Background pipeline | DML on pipeline tables only | Own permissive policy |
| `reverie_readonly` | Debugging, reporting | SELECT on most tables (excludes `device_tokens`) | Enforced — same as `reverie_app` |

### `reverie_ingestion` Access Scope

Has DML on: `works`, `authors`, `work_authors`, `manifestations`, `series`,
`series_works`, `omnibus_contents`, `metadata_versions`, `tags`, `manifestation_tags`,
`api_cache`, `ingestion_jobs`.

Denied: `users`, `shelves`, `shelf_items`, `device_tokens`, `webhooks`,
`webhook_deliveries`, `reading_sessions`, `reading_positions`.

## Row Level Security (RLS)

RLS is enabled on `manifestations` only. Six per-operation policies control access:

| Policy | Operation | Roles | Logic |
|---|---|---|---|
| `manifestations_select_adult` | SELECT | `reverie_app`, `reverie_readonly` | Adults/admins see all |
| `manifestations_select_child` | SELECT | `reverie_app`, `reverie_readonly` | Children see shelf-assigned only |
| `manifestations_insert` | INSERT | `reverie_app` | Unrestricted (WITH CHECK true) |
| `manifestations_update` | UPDATE | `reverie_app` | Admin/adult only |
| `manifestations_delete` | DELETE | `reverie_app` | Admin/adult only |
| `manifestations_ingestion_full_access` | ALL | `reverie_ingestion` | Unconditional access |

Children cannot UPDATE or DELETE manifestations — these are shared library records.
Children manage their visibility through `shelf_items` instead.

### Session Variable Contract

`reverie_app` and `reverie_readonly` must set the user ID in a transaction:

```sql
BEGIN;
SELECT set_config('app.current_user_id', $1::text, true);
-- queries here see RLS-filtered rows
COMMIT;
```

`SET LOCAL` (the `true` parameter) is transaction-scoped and auto-resets on
commit/rollback — safe with connection pools. If the variable is not set,
`current_setting('app.current_user_id', true)` returns NULL, and `NULL::uuid`
causes all visibility checks to fail — queries return zero rows.

## Design Decisions

- **`is_child` / `role` sync**: CHECK constraint `chk_child_role_sync` ensures
  `is_child = true` only when `role = 'child'`. `role` controls permissions;
  `is_child` drives content filtering (RLS). They must stay consistent.

- **`sort_title` / `sort_name`**: Separate columns strip leading articles for display
  ordering. Application logic populates these on insert.

- **`position NUMERIC`** in `series_works`: Allows fractional ordering (e.g., 1.5 for
  novellas between volumes 1 and 2).

- **Self-referential `series.parent_id`**: Uses `ON DELETE SET NULL` to orphan children
  rather than cascade-delete entire series trees.

- **`updated_at` triggers**: Active on `users`, `works`, `manifestations`. Reserved
  table `reading_positions` has the column but no trigger yet — add via the reusable
  `set_updated_at()` function when activated.

- **pgvector**: Reserved as a SQL comment in migration 7. When ready, create a new
  migration to add the extension, column, and index.

## Naming Convention

All identifiers use `snake_case`. No hyphens anywhere — enum values, column names,
table names all use underscores (e.g., `sub_genre` not `sub-genre`).
