---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0009"
title: "Works and manifestations data model"
satisfies:
  - "REV-REQ-0025"
  - "REV-REQ-0026"
  - "REV-REQ-0027"
  - "REV-REQ-0028"
---

# Works and manifestations data model

Reverie borrows the work and manifestation names from the IFLA FRBR model. A Reverie manifestation is one ingested file,
which the IFLA Library Reference Model calls an item: a manifestation in that model is the set of all carriers sharing
one content and production plan, so every download of one online file belongs to the same manifestation. Reverie has no
expression tier, and `language` is a `works` column where the reference model places it on the expression. This Design
covers the works/manifestations split and everything the migrations build directly around it: authors and their per-work
roles, series and their self-referential nesting, the omnibus mapping, the flat vocabulary tables and their
per-manifestation junctions, the `updated_at` and search-vector triggers, and the pointer columns every other subject
that touches a work or a manifestation attaches through.

## Purpose and boundaries

This subject owns the schema and its own structural guarantees for: `works` and `authors` and their join table
`work_authors`; `manifestations`, including its file-identity columns (`file_path`, `ingestion_file_hash`,
`current_file_hash`) and its three lifecycle status enums (`validation_status`, `ingestion_status`,
`enrichment_status`); `series` and `series_works`; `omnibus_contents`; the vocabulary tables `genres`, `moods` and
`tags` and their junctions `manifestation_genres`, `manifestation_moods` and `manifestation_tags`; the `set_updated_at`
and `works_search_vector_update` triggers; and the foreign-key graph, unique constraints and CHECK constraints that
bound all of the above. All of it lives in `backend/migrations/20260810000000_initial_schema.up.sql`, with the three
vocabulary junctions' row-level-security policies added by
`backend/migrations/20260903000000_junction_table_rls.up.sql`.

It does not own row-level security or the grants that decide who may query these tables at all; that mechanism, and the
full policy grid for the tables in this model that carry it, is the Design "Row-level security and database context". It
does not own the role and scope checks a handler applies before it reaches this model; that is the Design "Authorization
axes". It does not own the Ingestion pipeline subject (`backend/src/services/ingestion/`), the EPUB validation and
repair subject (`backend/src/services/epub/`), the Enrichment pipeline subject (`backend/src/services/enrichment/`), the
Writeback pipeline subject (`backend/src/services/writeback/`), the Metadata review and editing subject
(`backend/src/routes/metadata.rs`), or the Books list query contract subject (`backend/src/routes/library/mod.rs`,
`filters.rs`, and the shared `sort_spec.rs`/`cursor.rs` modules it imports from `backend/src/routes/`), each of which
reads or writes this model without being part of it. It does not own the Identifier and rating registry subject's
external identifiers and ratings (`manifestation_external_identifiers`, `manifestation_external_ratings`,
`work_external_identifiers`, `identifier_schemes`, `rating_sources`). It does not own covers (`manifestations.cover_*`,
`has_embedded_cover`, the Covers subject) or accessibility metadata (`manifestations.accessibility_metadata`) beyond
carrying them as columns.

Depends on: `metadata_versions`, owned by the Metadata review and editing subject, as the target of every `*_version_id`
and `source_version_id` pointer column in this model; Postgres's native `uuidv7()` for the default on every entity
table's surrogate `id` column; the `pg_trgm` and `unaccent` extensions the migration installs, whose trigram operators
and folding function back the indexes the Search and vocabulary suggest subject queries.

Depended on by: the Ingestion pipeline subject and the EPUB validation and repair subject, which create `manifestations`
rows and the `works` row each attaches to; the Enrichment pipeline subject and the Metadata review and editing subject,
which apply canonical field values and journal them through the version pointers; the Writeback pipeline subject, which
updates `file_path`, `current_file_hash` and `updated_at` after a successful on-disk rewrite; the Books list query
contract subject, the OPDS catalogue subject, and the Search and vocabulary suggest subject, which read this model to
build their responses; the Shelves subject and the Reading state subject, whose tables reference `manifestations.id`;
and the Design "Library filter and sort state", whose sort and filter axes name columns this model owns.

## Structure

| Table | Purpose | Notable columns |
| ----- | ------- | --------------- |
| `works` | One abstract title | `title`, `sort_title`, `subtitle`, `description`, `language`, `search_vector` |
| `authors` | One author identity, unique by exact name | `name`, `sort_name` |
| `work_authors` | Work-author membership, one row per work/author/role | `role`, `position`, `source_version_id` |
| `manifestations` | One ingested file of one format | file-identity trio, three status enums, version pointers |
| `series` | A series, with self-referential nesting | `name`, `sort_name`, `parent_id` |
| `series_works` | Series membership, fractional ordering | `series_id`, `work_id`, `position`, `is_omnibus`, `note` |
| `omnibus_contents` | Omnibus to its works | `omnibus_manifestation_id`, `contained_work_id`, `position` |
| `genres`, `moods`, `tags` | Flat vocabulary terms, unique case-insensitively | `name` |
| `manifestation_genres`, `manifestation_moods`, `manifestation_tags` | Term links | `manifestation_id`, `<term>_id` |

**Works, authors and their join.** `work_authors`'s primary key is `(work_id, author_id, role)`, so one author can hold
several distinct roles on the same work (author and narrator, say) but not the same role twice; `role` is the
`author_role` enum (`author`, `editor`, `translator`, `narrator`) and `position` orders same-role rows for display.
`authors_name_unique` is a `UNIQUE (name)` constraint, an exact-string deduplication; `series_name_unique` applies the
same deduplication to `series`. The vocabulary tables below apply case-insensitive deduplication instead, on
`lower(name)`; the split is authors and series against genres/moods/tags. `works.first_author_sort_name` is a redundant
copy of the lowest-`position` `author`-role contributor's `sort_name`, kept directly on `works` and backing the two
`idx_works_first_author_sort_*` indexes so the books list can sort by author without a join; `refresh_first_author_sort`
(`backend/src/models/work.rs`) is its only writer. `upgrade_stub` in that same file calls it whenever the extracted
metadata carries any contributor row; `insert_role_rows`/`delete_role_rows` behind `apply_contributors_patch` in
`backend/src/routes/metadata.rs`, and the same two helpers called directly from `apply_version`'s contributors arm in
that file (the accept/revert path for an enrichment-suggested contributor value), each call it only when the write
touched the `author` role's rows. A third `work_authors` delete site, `clear_field`'s contributor arm in `metadata.rs`,
never calls it: that path is restricted to the `editor` and `translator` roles, since clearing the `author` role is
rejected outright, so the sort key never needs refreshing there.

**Sort columns.** `works.sort_title` is `title` converted to lower case; `authors.sort_name` is the comma-inverted form
`generate_sort_name` in `backend/src/services/metadata/extractor.rs` produces (`"Tolkien, J. R. R."` from
`"J. R. R. Tolkien"`, a single-word name unchanged); `series.sort_name` is the series name converted to lower case,
written by `find_or_create_series`'s caller in `backend/src/models/work.rs`. None of the three strips a leading article.

**Manifestations.** `manifestations.work_id` is `NOT NULL`, `ON DELETE CASCADE` from `works`. The file-identity trio is
`file_path` (`UNIQUE`), `ingestion_file_hash` (`UNIQUE`, `NOT NULL`), and `current_file_hash` (`NOT NULL`, no
uniqueness). The three status enums (`validation_status`, `ingestion_status`, `enrichment_status`) each default to
`pending`; this model carries them as manifestation columns, but their transition rules belong to the pipelines that
drive them. `manifestations.ingestion_status` is the lifecycle of the manifestation row itself, once one exists;
`ingestion_jobs`, owned by the Ingestion pipeline subject, is a separate per-file row (`job_status`: `queued`,
`running`, `complete`, `failed`, `skipped`) grouped by `batch_id` for one scan, written for every scanned file including
one skipped as a duplicate or one that fails before any manifestation row is created, so the two need not agree.
`content_rating`, the cover columns (`cover_path`, `cover_sha256`, `cover_size_bytes`, `cover_source`,
`has_embedded_cover`), and `accessibility_metadata` are likewise columns this table carries for a neighbouring subject's
semantics. `suspected_duplicate_work_id` (`ON DELETE SET NULL` to `works`) is the one column that points sideways, at a
candidate duplicate rather than the owning work.

**Series.** `series.parent_id` self-references `series.id` with `ON DELETE SET NULL`: deleting a parent orphans its
children instead of cascading the delete through the tree. `series_works`'s primary key is `(series_id, work_id)`, so a
work belongs to a series at most once; `position` is `double precision` rather than an integer, so a work can be
inserted between two existing positions (a novella at `1.5` between volumes `1` and `2`) without renumbering every later
row. `double precision` carries about 15 significant decimal digits, so repeated midpoint insertions between the same
two positions converge toward that precision limit rather than bisecting indefinitely; no code renumbers positions.
`is_omnibus` and `note` exist on `series_works`, but no code under `backend/src` or `frontend/src` reads or writes
either column; the one production writer of the row, `find_or_create_series`/`upgrade_stub`'s
`INSERT INTO series_works (series_id, work_id, position)` in `backend/src/models/work.rs`, leaves both at their
defaults.

**Omnibus contents.** `omnibus_contents` maps one omnibus `manifestations` row to the several `works` rows it collects,
each at a `position`. No code under `backend/src` or `frontend/src` reads or writes this table; it exists in the schema,
with grants and an index, but no application path creates, queries, or deletes a row in it.

**Vocabulary and its junctions.** `genres`, `moods` and `tags` are flat term tables, each with a `UNIQUE (lower(name))`
index for case-insensitive deduplication and a trigram index (`immutable_unaccent(name)`) for the fuzzy matching the
Search and vocabulary suggest subject performs. Each junction table's primary key pairs `manifestation_id` with its term
id, so a manifestation carries a given term at most once; `source_version_id` points at the `metadata_versions` row that
supplied the link, when known. Unlike the vocabulary tables themselves, the three junctions carry row-level security
(`backend/migrations/20260903000000_junction_table_rls.up.sql`); see "Security and operations" below.

**Triggers.** `set_updated_at` runs `BEFORE UPDATE` on `works` and `manifestations` within this model; other subjects'
tables carry the same trigger function. `works_search_vector_update` fires before an insert, or an update of `title` or
`description`, on `works`, and rebuilds `search_vector` with `to_tsvector` under the `unaccent_english` configuration
over the title and description, each coalesced to an empty string when null and joined by a space; `subtitle` is a
versioned `works` column but is not part of the vector.

**The version-pointer pattern.** A canonical field's value lives directly on its `works` or `manifestations` column
(`title`, `description`, `language`, `subtitle` on `works`; `publisher`, `pub_date`, `isbn_10`, `isbn_13`, `pages`,
`content_rating`, the cover fields on `manifestations`), and a paired nullable `<field>_version_id` column, set to null
on delete of its `metadata_versions` target, names which journal row is currently attributed as that value.
`work_authors.source_version_id` and the three vocabulary junctions' `source_version_id` follow the same pattern for
their own rows. The pointer can be null even when the value it would attribute is not: a stub work created before any
draft exists, or a value set from a source that predates per-field versioning, carries a value with no attribution.

## Interfaces and dependencies

This subject has no HTTP surface of its own; every read and write reaches it through a neighbouring subject's handler or
service. The production writers are: `backend/src/models/work.rs` (`match_existing`, `create_stub`, `upgrade_stub`), the
Ingestion pipeline subject's entry point into this model, called from `backend/src/services/ingestion/orchestrator.rs`;
the same file's `rematch_on_isbn_change`, consumed by the enrichment orchestrator's ISBN re-check and, in `metadata.rs`,
by `accept_manifestation`'s accepted-ISBN-version path and `update_book_metadata`'s touched-ISBN path;
`backend/src/routes/metadata.rs`, the manual review and edit surface, which also writes `work_authors` and the
vocabulary junctions through `apply_contributors_patch` and `apply_vocabulary_patch`; `backend/src/services/enrichment/`
(`orchestrator.rs`, `queue.rs`, `field_lock.rs`), which applies accepted canonical values;
`backend/src/services/writeback/` (`orchestrator.rs`, `queue.rs`), which updates `file_path` and `current_file_hash`
after a successful on-disk rewrite; and `backend/src/services/metadata/draft.rs`, which writes the `metadata_versions`
rows the pointer columns name. The production readers include `backend/src/routes/library/mod.rs` and `filters.rs`, and
the shared `backend/src/routes/sort_spec.rs` and `cursor.rs` modules they import (also used by
`backend/src/routes/shelves/mod.rs`), `backend/src/routes/series/mod.rs`, `backend/src/routes/opds/` (the OPDS catalogue
subject), and `backend/src/routes/suggest.rs` and `backend/src/routes/library/search.rs` (vocabulary and text search).

The migration grants `SELECT`, `INSERT`, `UPDATE` and `DELETE` on every table in this model to `reverie_app` and
`reverie_ingestion`, and `SELECT` alone to `reverie_readonly`; the Design "Row-level security and database context" is
the authority on that grant boundary and on the policies that further narrow `manifestations` and its three vocabulary
junctions.

## Data and state

[`backend/schema.sql`](../../../../backend/schema.sql), the `pg_dump --schema-only` of a freshly migrated database that
CI rebuilds and diffs, is the authoritative column-by-column reference for this model; this section states only what
changes runtime behaviour or governs application logic. Every identifier in the schema (table, column, enum type, and
enum value alike) is `snake_case`.

Every entity table in this model (`works`, `authors`, `manifestations`, `series`, `genres`, `moods`, `tags`) has a
surrogate `id` column defaulting to `uuidv7()`, a PostgreSQL 18 core function (the dev and CI image is `postgres:18`,
per `docker/compose.dev.yml`), so those ids sort approximately by creation time; this is a repository-wide convention
for entity tables, not particular to this model. The dump installs only three extensions: `pg_trgm`, `pgcrypto` and
`unaccent`. The junction tables (`work_authors`, `series_works`, `omnibus_contents`, and the three vocabulary junctions)
carry no `id` column at all and use a composite natural key instead, the same split the schema applies elsewhere
(`shelf_items`, `reading_state`, `field_locks`, and the external-identifier and external-rating tables follow the
identical pattern). Every first-party `TIMESTAMPTZ` column in this model carries a decode-range `CHECK` constraint
(`>= 0001-01-01`, `< 10000-01-01`), the same convention the migration applies throughout the schema.

`manifestations_pages_positive` rejects a non-positive `pages` value at the database, independent of any
application-level check upstream. `manifestation_external_identifiers_external_id_check` and its `work_`-prefixed
counterpart constrain the external-identifier format, but those two tables belong to the Identifier and rating registry
subject, not this one; they are named here only because their FK targets (`manifestations`, `works`) are this model's
tables.

`ingestion_file_hash` is set once, on the row's initial `INSERT`, and no code path in `backend/src` updates it
afterwards; that holds only because no writer exists, not because any schema constraint or runtime guard enforces it. A
test in the writeback orchestrator, `ingestion_file_hash_immutable_across_writeback_chain`, checks it.
`current_file_hash` equals `ingestion_file_hash` until the Writeback pipeline subject's first successful rewrite of the
file, after which that pipeline is the column's sole writer.

## Runtime behaviour

**Creating a work and its first manifestation** must resolve a foreign-key cycle: `manifestations.work_id` references
`works.id`, and every `metadata_versions` row references `manifestations.id`, but a work's canonical columns are
themselves wired to `metadata_versions` rows once they exist. `backend/src/models/work.rs` resolves the cycle in three
steps: `create_stub` inserts an empty-placeholder `works` row (`title = ''`, `sort_title = ''`) so the manifestation
insert's `work_id` foreign key has a target before any metadata is known; the manifestation and its `metadata_versions`
draft rows are then written (by the Ingestion pipeline subject, not this model); and `upgrade_stub` runs a single
`UPDATE` against the stub, setting `title`/`sort_title`/`subtitle`/`description`/`language` and their version pointers
together with the `work_authors` and `series_works` rows the extracted metadata implies. A work therefore always exists,
even mid-ingestion, but can hold empty-string title and `sort_title` briefly; the sequencing that guarantees the upgrade
follows belongs to the Ingestion pipeline subject, not this model.

**A `works` delete cascades** to its `manifestations`, `work_authors`, `series_works`, and `work_external_identifiers`
rows, and to `omnibus_contents` rows where it is `contained_work_id`; a `manifestations` row that names it as
`suspected_duplicate_work_id` has that pointer `SET NULL` instead of being deleted or blocked. A cascaded
`manifestations` delete in turn cascades to that row's own `manifestation_genres`/`moods`/`tags`, `metadata_versions`,
`manifestation_external_identifiers`, `manifestation_external_ratings`, `field_locks`, `omnibus_contents` rows where it
is `omnibus_manifestation_id`, `reading_state`, `reading_sessions`, `shelf_items`, and `writeback_jobs` rows; the last
five belong to neighbouring subjects and are named here only as cascade targets, not described further.

**A `metadata_versions` delete degrades its pointers silently.** Every `*_version_id` and `source_version_id` column
this model owns is `ON DELETE SET NULL`, so deleting the journal row a value was attributed to leaves the value itself
untouched on `works` or `manifestations` and simply clears which row it came from; no error is raised and no value is
lost, only its attribution.

**First-observation-wins deduplication.** `find_or_create_author` and the series equivalent in `work.rs` both use
`INSERT ... ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id`. Because the conflict target is the
exact `name`, `EXCLUDED.name` is always identical to the stored value on a repeat observation; the `DO UPDATE` exists
only to make `RETURNING` return the existing row's `id` on a conflict, not to change anything; `sort_name`, absent from
the `SET` list entirely, stays at whatever the first insert supplied even if a later observation would compute it
differently. The vocabulary tables' upsert, `insert_vocabulary_rows` in `metadata.rs`, conflicts on `lower(name)`
instead: an incoming name that differs only in casing from an existing term still matches, and its
`ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name` then does change the stored row, so the display casing
from the most recent write replaces the term's stored form. The `work_authors` insert in `work.rs`, and the
`insert_role_rows` helper behind `apply_contributors_patch` in `metadata.rs`, use a third pattern,
`ON CONFLICT (work_id, author_id, role) DO NOTHING`: a duplicate role-author pairing on a work is silently skipped
rather than erroring or updating.

**The unique constraints on `manifestations` are a database-level backstop**, independent of whatever dedup check an
upstream caller already ran: a second `INSERT` naming a `file_path` or `ingestion_file_hash` that already exists in the
table is rejected by the constraint before the row can be created, and the resulting `sqlx::Error` is the caller's, the
Ingestion pipeline subject's, to interpret.

## Failure and recovery

A row can never exist in a junction table (`work_authors`, the three vocabulary junctions, `omnibus_contents`) without a
live parent on both sides: every one of those foreign keys is `ON DELETE CASCADE`, so the schema itself prevents an
orphaned membership row rather than relying on application cleanup. `series.parent_id` is the one parent/child
relationship in this model that instead orphans rather than cascades, by design (see "Structure"); it is not the model's
only `SET NULL` column: `suspected_duplicate_work_id` and every version-pointer column behave the same way, described
above.

A `metadata_versions` deletion never fails a constraint on this side: because every pointer into it is `SET NULL`, there
is no foreign-key error to recover from, only the silent attribution loss described above. A duplicate `file_path` or
`ingestion_file_hash` insert does fail a constraint (`unique_violation`); this model guarantees the row is never created
twice, but does not itself define what the caller does with the resulting error; that recovery path belongs to the
Ingestion pipeline subject.

`manifestations_pages_positive` and the `TIMESTAMPTZ` decode-range checks reject an out-of-range value at the database
as a `CHECK` violation, a second layer behind whatever validation a calling handler already applied; a value that
reaches this model having skipped that handler-level validation is still refused here.

## Security and operations

Of the tables this model owns, only `manifestations` and its three vocabulary junctions carry row-level security;
`works`, `authors`, `work_authors`, `series`, `series_works` and `omnibus_contents` carry none and are granted for full
DML to both `reverie_app` and `reverie_ingestion`. (`work_external_identifiers`, a `works`-adjacent table belonging to
the Identifier and rating registry subject rather than this one, does carry row-level security; it is not a
counterexample to the statement above, which is scoped to this model's own tables.) Practically, that means the
catalogue data proper (a work's title, its authors, its series membership, the vocabulary terms that exist as rows) is
global and shared across every account once at least one visible manifestation links to it; only which manifestations an
account can see is per-user or per-role, through the mechanism the Design "Row-level security and database context"
owns. The three vocabulary junctions are the one place vocabulary data becomes visibility-scoped: their `SELECT`
policies inherit the linked `manifestations` row's own visibility with no further predicate, while their `INSERT`,
`UPDATE` and `DELETE` policies add an independent check that the caller holds the `admin` or `adult` role, regardless of
the manifestation's visibility.

Because the shared catalogue tables carry no row-level security, the only defence against an under-privileged write is
the handler that reaches them. Every route handler in `backend/src/routes/metadata.rs` that writes to this model
(`accept_manifestation`, `reject_manifestation`, `revert_manifestation`, `lock_field`, `unlock_field`, and
`update_book_metadata`, which reaches `work_authors` via `apply_contributors_patch` and the vocabulary junctions via
`apply_vocabulary_patch`) calls `CurrentUser::require_not_child` before any write; the mechanism that check draws on is
the Design "Authorization axes", not restated here. The Ingestion pipeline, Enrichment pipeline and Writeback pipeline
writers named in "Interfaces and dependencies" reach this model over their own dedicated connection pools rather than a
per-request credential, so no caller-supplied role or scope gates them; the pool and grant boundary that confines those
pools to the catalogue and pipeline tables is again the Design "Row-level security and database context".

This subject has no operational surface of its own to run, restart, or scale: it is schema, read and written entirely
through the neighbouring subjects named above.
