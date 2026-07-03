//! Work matching + creation.
//!
//! After Step 7 the flow is split so the orchestrator can honour the ingest
//! invariant (every canonical field has a matching `metadata_versions` row):
//!
//! 1. `match_existing` — pure match; returns `Some(work_id)` on ISBN or
//!    title+author fuzzy match, else `None`.
//! 2. `create_stub` — inserts an empty-placeholder work so the manifestation FK
//!    is satisfied before any drafts are written.
//! 3. `upgrade_stub` — runs an `UPDATE` against the stub with the metadata and wires
//!    `works.{title,description,language}_version_id` + `work_authors.source_version_id`
//!    from the draft IDs returned by `metadata::draft::write_drafts`.
//!
//! `find_or_create` (test-only, `#[cfg(test)]`) remains as a
//! transaction-wrapped convenience for call sites that don't need the
//! ingest-invariant split (notably the existing tests).
//!
//! Deviation from plan task 4: the plan's single `find_or_create(tx, meta, draft_ids)`
//! signature cannot be implemented without breaking the FK cycle
//! (`manifestations.work_id` → works.id, `metadata_versions.manifestation_id` →
//! manifestations.id, `draft_ids` requires existing drafts). The split preserves
//! the plan's behaviour (matched paths leave pointers untouched; create paths
//! wire all pointers from `draft_ids`).

use sqlx::PgConnection;
#[cfg(test)]
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::metadata::draft::DraftIds;
use crate::services::metadata::extractor::ExtractedMetadata;

/// Outcome of `rematch_on_isbn_change` — Step 7 task 6.
/// Consumed by the enrichment orchestrator (Step 7 task 21).
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub enum RematchOutcome {
    /// No matching ISBN row elsewhere; the manifestation is left untouched.
    NoOp,
    /// The current work was a stub safe to retire; its sole manifestation
    /// has been moved to the matched work and the stub deleted.
    AutoMerged {
        /// Stub work id that was deleted.
        from: Uuid,
        /// Surviving work id that absorbed the manifestation.
        to: Uuid,
    },
    /// A potential duplicate exists but cannot be auto-merged. Two
    /// conditions produce this outcome: (1) multiple distinct works match
    /// the ISBN (ambiguous merge target), or (2) exactly one match exists
    /// but the current work has other manifestations or `manual`-source
    /// drafts that block safe retirement.
    /// `manifestations.suspected_duplicate_work_id` has been set for
    /// operator review.
    Suspected {
        /// Work id of the suspected duplicate.
        matched_work: Uuid,
    },
}

/// Try to match an existing work by ISBN-13 or title+author similarity (0.6).
/// Pure read; never writes.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the candidate lookup queries.
pub async fn match_existing(
    conn: &mut PgConnection,
    metadata: &ExtractedMetadata,
) -> Result<Option<Uuid>, sqlx::Error> {
    if let Some(isbn) = &metadata.isbn
        && let Some(isbn_13) = &isbn.isbn_13
    {
        let hit = sqlx::query_scalar!(
            "SELECT w.id FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.isbn_13 = $1 \
             LIMIT 1",
            isbn_13,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if hit.is_some() {
            return Ok(hit);
        }
    }

    if let Some(title) = &metadata.title
        && let Some(first_author) = metadata.creators.first()
    {
        let hit = sqlx::query_scalar!(
            "SELECT w.id FROM works w \
             JOIN work_authors wa ON wa.work_id = w.id \
             JOIN authors a ON a.id = wa.author_id \
             WHERE similarity(w.title, $1) > 0.6 \
               AND similarity(a.name, $2) > 0.6 \
             ORDER BY similarity(w.title, $1) DESC \
             LIMIT 1",
            title,
            &first_author.name,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if hit.is_some() {
            return Ok(hit);
        }
    }

    Ok(None)
}

/// Insert an empty-placeholder work used to satisfy the manifestation FK
/// before drafts are written. Upgrade via `upgrade_stub` after drafts exist.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `INSERT … RETURNING id`.
pub async fn create_stub(conn: &mut PgConnection) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id")
        .fetch_one(&mut *conn)
        .await
}

/// Upgrade a stub work (from `create_stub`) to the real thing:
/// set `title/sort_title/description/language` + canonical pointers,
/// create authors + `work_authors` rows with `source_version_id` wired.
/// Also creates the series row if present.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from any of the constituent statements
/// (work `UPDATE`, author find-or-create, `work_authors INSERT`,
/// series find-or-create, `series_works INSERT`).
pub async fn upgrade_stub(
    conn: &mut PgConnection,
    work_id: Uuid,
    metadata: &ExtractedMetadata,
    draft_ids: &DraftIds,
) -> Result<(), sqlx::Error> {
    let work_title = metadata.title.as_deref().unwrap_or("Unknown");
    let work_sort_title = metadata.sort_title.as_deref().unwrap_or(work_title);

    sqlx::query!(
        "UPDATE works SET \
            title = $1, \
            sort_title = $2, \
            subtitle = $3, \
            description = $4, \
            language = $5, \
            title_version_id = $6, \
            subtitle_version_id = $7, \
            description_version_id = $8, \
            language_version_id = $9 \
         WHERE id = $10",
        work_title,
        work_sort_title,
        metadata.subtitle.as_deref(),
        metadata.description.as_deref(),
        metadata.language.as_deref(),
        draft_ids.get("title").copied(),
        draft_ids.get("subtitle").copied(),
        draft_ids.get("description").copied(),
        draft_ids.get("language").copied(),
        work_id,
    )
    .execute(&mut *conn)
    .await?;

    for (i, creator) in metadata.creators.iter().enumerate() {
        let author_id = find_or_create_author(conn, &creator.name, &creator.sort_name).await?;
        let position = i32::try_from(i).unwrap_or(i32::MAX);
        let source_version_id = draft_ids
            .get(&format!("contributors.{}", creator.role))
            .copied();
        sqlx::query!(
            // sqlx macros have no built-in &str→enum mapping for `author_role`,
            // so $3 binds as text and the cast to the DB enum happens in SQL.
            "INSERT INTO work_authors (work_id, author_id, role, position, source_version_id) \
             VALUES ($1, $2, ($3::text)::author_role, $4, $5) \
             ON CONFLICT (work_id, author_id, role) DO NOTHING",
            work_id,
            author_id,
            &creator.role,
            position,
            source_version_id,
        )
        .execute(&mut *conn)
        .await?;
    }
    if !metadata.creators.is_empty() {
        refresh_first_author_sort(conn, work_id).await?;
    }

    if let Some(series) = &metadata.series {
        let series_id =
            find_or_create_series(conn, &series.name, &series.name.to_lowercase()).await?;
        sqlx::query!(
            // $3 bound as float8; postgres implicitly casts to NUMERIC for
            // the column. Avoids requiring sqlx's `bigdecimal` feature for
            // an Option<f64> source value.
            "INSERT INTO series_works (series_id, work_id, position) \
             VALUES ($1, $2, $3::float8)",
            series_id,
            work_id,
            series.position,
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

/// Convenience wrapper: opens its own transaction, runs the full
/// match-or-create flow without draft wiring. Retained for call sites that
/// don't participate in the ingest-invariant flow (primarily the existing
/// tests).
#[allow(dead_code)]
#[cfg(test)]
pub async fn find_or_create(
    pool: &PgPool,
    metadata: &ExtractedMetadata,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    if let Some(id) = match_existing(&mut tx, metadata).await? {
        tx.commit().await?;
        return Ok(id);
    }

    let stub_id = create_stub(&mut tx).await?;
    upgrade_stub(&mut tx, stub_id, metadata, &DraftIds::new()).await?;
    tx.commit().await?;
    Ok(stub_id)
}

/// Refresh `works.first_author_sort_name` from the current `work_authors` rows.
///
/// This is the ONLY writer of that column: every `work_authors` mutation site
/// (`upgrade_stub` and the contributors `PATCH` handler) calls it after its
/// writes so the denormalized author-sort index never drifts from the
/// underlying rows. Picks the lowest-`position` row with `role = 'author'`;
/// works with no author row get `NULL` (sorts last). The `UPDATE` also bumps
/// `works.updated_at` via the table's existing trigger.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE` statement.
pub async fn refresh_first_author_sort(
    conn: &mut PgConnection,
    work_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE works SET first_author_sort_name = (\
             SELECT a.sort_name FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author' \
             ORDER BY wa.position ASC LIMIT 1) \
         WHERE id = $1",
        work_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn find_or_create_author(
    conn: &mut PgConnection,
    name: &str,
    sort_name: &str,
) -> Result<Uuid, sqlx::Error> {
    // DO UPDATE SET name = EXCLUDED.name is a no-op trick to make RETURNING work
    // on the conflict path (DO NOTHING doesn't return the existing row).
    sqlx::query_scalar!(
        "INSERT INTO authors (name, sort_name) VALUES ($1, $2) \
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
        name,
        sort_name,
    )
    .fetch_one(&mut *conn)
    .await
}

async fn find_or_create_series(
    conn: &mut PgConnection,
    name: &str,
    sort_name: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ($1, $2) \
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
        name,
        sort_name,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Step 7 task 6 — re-check a manifestation's ISBN against other works.
///
/// Auto-merges the current work into a matched work when:
///   * exactly one other work holds the same ISBN,
///   * current work has no other manifestations, and
///   * current work has no `manual`-source drafts.
///
/// Otherwise, if any matches exist, sets `suspected_duplicate_work_id`.
///
/// Must be called inside the caller's transaction; uses `FOR UPDATE` on
/// candidate rows to avoid concurrent rematch races.
/// Consumed by the enrichment orchestrator (Step 7 task 21).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from any of: the manifestation lookup, the
/// `FOR UPDATE` candidate scan, the safety-condition counts, the
/// `manifestations.work_id` `UPDATE`, the stub `DELETE`, or the
/// `suspected_duplicate_work_id` `UPDATE`.
#[allow(dead_code)]
pub async fn rematch_on_isbn_change(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
) -> Result<RematchOutcome, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT work_id, isbn_10, isbn_13 FROM manifestations WHERE id = $1 FOR UPDATE",
        manifestation_id,
    )
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Ok(RematchOutcome::NoOp);
    };
    let current_work_id = row.work_id;
    let isbn_10 = row.isbn_10;
    let isbn_13 = row.isbn_13;

    if isbn_10.is_none() && isbn_13.is_none() {
        return Ok(RematchOutcome::NoOp);
    }

    // Postgres forbids DISTINCT with FOR UPDATE, so lock the manifestation
    // rows (not-distinct) and dedupe their work_ids in Rust. The FOR UPDATE
    // still guarantees no concurrent rematch can mutate the same matched
    // manifestation mid-flight.
    let raw_matches = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         WHERE m.work_id != $3 \
           AND ( \
               (m.isbn_13 = $1 AND $1 IS NOT NULL) OR \
               (m.isbn_10 = $2 AND $2 IS NOT NULL) \
           ) \
         FOR UPDATE",
        isbn_13,
        isbn_10,
        current_work_id,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut seen = std::collections::HashSet::new();
    let mut matches: Vec<Uuid> = Vec::new();
    for w in raw_matches {
        if seen.insert(w) {
            matches.push(w);
        }
    }

    if matches.is_empty() {
        return Ok(RematchOutcome::NoOp);
    }

    if matches.len() == 1 {
        let other_manifestations = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestations \
             WHERE work_id = $1 AND id != $2",
            current_work_id,
            manifestation_id,
        )
        .fetch_one(&mut *conn)
        .await?;

        let manual_drafts = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions mv \
             JOIN manifestations m ON m.id = mv.manifestation_id \
             WHERE m.work_id = $1 AND mv.source = 'manual'",
            current_work_id,
        )
        .fetch_one(&mut *conn)
        .await?;

        if other_manifestations == 0 && manual_drafts == 0 {
            let target_work_id = matches[0];
            sqlx::query!(
                "UPDATE manifestations SET work_id = $1 WHERE id = $2",
                target_work_id,
                manifestation_id,
            )
            .execute(&mut *conn)
            .await?;
            sqlx::query!("DELETE FROM works WHERE id = $1", current_work_id)
                .execute(&mut *conn)
                .await?;
            return Ok(RematchOutcome::AutoMerged {
                from: current_work_id,
                to: target_work_id,
            });
        }
    }

    let pick = matches[0];
    sqlx::query!(
        "UPDATE manifestations SET suspected_duplicate_work_id = $1 WHERE id = $2",
        pick,
        manifestation_id,
    )
    .execute(&mut *conn)
    .await?;

    Ok(RematchOutcome::Suspected { matched_work: pick })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::metadata::extractor::{ExtractedCreator, SeriesInfo};
    use crate::services::metadata::isbn::IsbnResult;
    use crate::test_support::db::ingestion_pool_for;

    fn test_metadata(title: &str, author: &str) -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some(title.into()),
            sort_title: Some(title.to_lowercase()),
            subtitle: None,
            description: None,
            language: Some("en".into()),
            creators: vec![ExtractedCreator {
                name: author.into(),
                sort_name: format!("{author}, Test"),
                role: "author".into(),
            }],
            unmapped_contributors: vec![],
            pages: None,
            publisher: None,
            pub_date: None,
            isbn: None,
            subjects: vec![],
            series: None,
            inversion: None,
            confidence: 0.5,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_new_work(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let meta = test_metadata("Test Book Alpha", "Test Author Alpha");
        let work_id = find_or_create(&pool, &meta).await.unwrap();

        let title = sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(title, "Test Book Alpha");

        let author_name = sqlx::query_scalar!(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             WHERE wa.work_id = $1",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_name, "Test Author Alpha");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_deduplicates_authors(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;

        let meta1 = test_metadata("Astronomy Fundamentals", "Shared Author Name");
        let meta2 = test_metadata("Renaissance Cooking Guide", "Shared Author Name");

        let work_id1 = find_or_create(&pool, &meta1).await.unwrap();
        let work_id2 = find_or_create(&pool, &meta2).await.unwrap();
        assert_ne!(work_id1, work_id2);

        let author_id1 = sqlx::query_scalar!(
            "SELECT author_id FROM work_authors WHERE work_id = $1 LIMIT 1",
            work_id1,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let author_id2 = sqlx::query_scalar!(
            "SELECT author_id FROM work_authors WHERE work_id = $1 LIMIT 1",
            work_id2,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_id1, author_id2, "same author should be reused");

        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM authors WHERE name = 'Shared Author Name'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_deduplicates_series(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;

        let mut meta1 = test_metadata("Series Book 1", "Series Author");
        meta1.series = Some(SeriesInfo {
            name: "Test Series Dedup".into(),
            position: Some(1.0),
        });
        let mut meta2 = test_metadata("Series Book 2", "Series Author");
        meta2.series = Some(SeriesInfo {
            name: "Test Series Dedup".into(),
            position: Some(2.0),
        });

        let _work_id1 = find_or_create(&pool, &meta1).await.unwrap();
        let _work_id2 = find_or_create(&pool, &meta2).await.unwrap();

        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM series WHERE name = 'Test Series Dedup'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_or_create_matches_by_isbn(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;

        let mut meta1 = test_metadata("ISBN Match Test", "ISBN Author");
        meta1.isbn = Some(IsbnResult {
            isbn_10: Some("0306406152".into()),
            isbn_13: Some("9780306406157".into()),
            valid: true,
        });
        let work_id1 = find_or_create(&pool, &meta1).await.unwrap();

        let isbn = "9780306406157";
        let path = format!("/tmp/test-isbn-{work_id1}.epub");
        sqlx::query!(
            "INSERT INTO manifestations \
             (work_id, isbn_13, format, file_path, ingestion_file_hash, current_file_hash, \
              file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, $2, 'epub'::manifestation_format, $3, 'testhash', 'testhash', 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status)",
            work_id1,
            isbn,
            path,
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut meta2 = test_metadata("Different Title", "Different Author");
        meta2.isbn = Some(IsbnResult {
            isbn_10: Some("0306406152".into()),
            isbn_13: Some("9780306406157".into()),
            valid: true,
        });
        let work_id2 = find_or_create(&pool, &meta2).await.unwrap();
        assert_eq!(work_id1, work_id2, "ISBN match should return existing work");
    }

    /// Helper: insert a `metadata_versions` row directly, for use as a
    /// `work_authors.source_version_id` FK target in `upgrade_stub` tests.
    async fn insert_journal_row(pool: &PgPool, manifestation_id: Uuid, field_name: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                 (manifestation_id, source, field_name, new_value, value_hash, \
                  match_type, confidence_score) \
             VALUES ($1, 'opf', $2, '[]'::jsonb, '\\x00'::bytea, 'title', 0.5) \
             RETURNING id",
            manifestation_id,
            field_name,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upgrade_stub_wires_per_role_source_version_id_and_sort_name(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_unused_work, manifestation_id) =
            crate::test_support::db::insert_work_and_manifestation(&pool, "role-wiring").await;
        let author_version_id =
            insert_journal_row(&pool, manifestation_id, "contributors.author").await;
        let editor_version_id =
            insert_journal_row(&pool, manifestation_id, "contributors.editor").await;

        let mut conn = pool.acquire().await.unwrap();
        let work_id = create_stub(&mut conn).await.unwrap();

        // Editor placed BEFORE author in document order: the sort column must
        // still resolve to the author, not whichever role sits at position 0.
        let meta = ExtractedMetadata {
            creators: vec![
                ExtractedCreator {
                    name: "Editor Person".into(),
                    sort_name: "Person, Editor".into(),
                    role: "editor".into(),
                },
                ExtractedCreator {
                    name: "Author Person".into(),
                    sort_name: "Person, Author".into(),
                    role: "author".into(),
                },
            ],
            ..test_metadata("Role Wiring Test", "unused")
        };
        let mut draft_ids = DraftIds::new();
        draft_ids.insert("contributors.author".into(), author_version_id);
        draft_ids.insert("contributors.editor".into(), editor_version_id);

        upgrade_stub(&mut conn, work_id, &meta, &draft_ids)
            .await
            .unwrap();
        drop(conn);

        let editor_source = sqlx::query_scalar!(
            "SELECT source_version_id FROM work_authors \
             WHERE work_id = $1 AND role = 'editor'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let author_source = sqlx::query_scalar!(
            "SELECT source_version_id FROM work_authors \
             WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(editor_source, Some(editor_version_id));
        assert_eq!(author_source, Some(author_version_id));
        assert_ne!(editor_source, author_source);

        let sort_name = sqlx::query_scalar!(
            "SELECT first_author_sort_name FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sort_name.as_deref(), Some("Person, Author"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upgrade_stub_authorless_metadata_leaves_sort_name_null(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_unused_work, manifestation_id) =
            crate::test_support::db::insert_work_and_manifestation(&pool, "authorless").await;
        let editor_version_id =
            insert_journal_row(&pool, manifestation_id, "contributors.editor").await;

        let mut conn = pool.acquire().await.unwrap();
        let work_id = create_stub(&mut conn).await.unwrap();

        let meta = ExtractedMetadata {
            creators: vec![ExtractedCreator {
                name: "Editor Only".into(),
                sort_name: "Only, Editor".into(),
                role: "editor".into(),
            }],
            ..test_metadata("Authorless Stub Test", "unused")
        };
        let mut draft_ids = DraftIds::new();
        draft_ids.insert("contributors.editor".into(), editor_version_id);

        upgrade_stub(&mut conn, work_id, &meta, &draft_ids)
            .await
            .unwrap();
        drop(conn);

        let sort_name = sqlx::query_scalar!(
            "SELECT first_author_sort_name FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(sort_name.is_none());
    }

    // ── Task 34: rematch_on_isbn_change integration tests ─────────────────

    /// Helper: insert a blank work directly (bypassing `find_or_create` so
    /// test titles don't collide via `pg_trgm` similarity).
    async fn insert_work(pool: &PgPool, title: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, lower($1)) RETURNING id",
            title,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Helper: insert a manifestation row for a given work/ISBN combo.
    /// Returns the manifestation id.
    async fn insert_manifestation(
        pool: &PgPool,
        work_id: Uuid,
        isbn_13: Option<&str>,
        file_marker: &str,
    ) -> Uuid {
        let path = format!("/tmp/rematch-{file_marker}.epub");
        let hash = format!("hash-{file_marker}");
        sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, isbn_13, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, $2, 'epub'::manifestation_format, $3, $4, $4, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            isbn_13,
            path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Stub work (no other manifestations, no manual drafts) with ISBN
    /// matching another work → auto-merge: manifestation moves, stub deleted.
    #[sqlx::test(migrations = "./migrations")]
    async fn rematch_auto_merge(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = "9780000000001";

        // Seed "real" work + manifestation with the target ISBN.
        let real_work_id = insert_work(&pool, &format!("Real-{marker}")).await;
        let _real_m =
            insert_manifestation(&pool, real_work_id, Some(isbn), &format!("{marker}-a")).await;

        // Seed "stub" work (different title, one manifestation) with the
        // same ISBN that will trigger rematch.
        let stub_work_id = insert_work(&pool, &format!("Stub-{marker}")).await;
        let stub_manifestation_id =
            insert_manifestation(&pool, stub_work_id, Some(isbn), &format!("{marker}-b")).await;

        // Run rematch inside a transaction (matches orchestrator usage).
        let mut tx = pool.begin().await.unwrap();
        let outcome = rematch_on_isbn_change(&mut tx, stub_manifestation_id)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome,
            RematchOutcome::AutoMerged {
                from: stub_work_id,
                to: real_work_id,
            },
            "expected auto-merge"
        );

        // Stub work must be deleted.
        let stub_exists = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM works WHERE id = $1",
            stub_work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stub_exists, 0, "stub work should be deleted");

        // Manifestation should now point at the real work.
        let new_work_id = sqlx::query_scalar!(
            "SELECT work_id FROM manifestations WHERE id = $1",
            stub_manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(new_work_id, real_work_id);
    }

    /// Stub work has 2 manifestations → suspected, not auto-merged.
    #[sqlx::test(migrations = "./migrations")]
    async fn rematch_suspected_when_multiple_manifestations(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = "9780000000002";

        let real_meta = test_metadata(&format!("Real MultiMani {marker}"), "RM Author");
        let real_work_id = find_or_create(&pool, &real_meta).await.unwrap();
        let _ = insert_manifestation(&pool, real_work_id, Some(isbn), &format!("{marker}-a")).await;

        let stub_meta = test_metadata(&format!("Stub MultiMani {marker}"), "SM Author");
        let stub_work_id = find_or_create(&pool, &stub_meta).await.unwrap();
        let target_m =
            insert_manifestation(&pool, stub_work_id, Some(isbn), &format!("{marker}-b")).await;
        // Add a second manifestation on the stub — should inhibit auto-merge.
        let _sibling_m =
            insert_manifestation(&pool, stub_work_id, None, &format!("{marker}-c")).await;

        let mut tx = pool.begin().await.unwrap();
        let outcome = rematch_on_isbn_change(&mut tx, target_m).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome,
            RematchOutcome::Suspected {
                matched_work: real_work_id
            },
            "expected Suspected outcome"
        );

        // Stub work must still exist.
        let stub_exists = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM works WHERE id = $1",
            stub_work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stub_exists, 1, "stub work should NOT be deleted");

        // suspected_duplicate_work_id should be set.
        let dup = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            target_m,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dup, Some(real_work_id));
    }

    /// Stub work has a manual-source draft → suspected, not auto-merged.
    #[sqlx::test(migrations = "./migrations")]
    async fn rematch_suspected_when_manual_draft_exists(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = "9780000000003";

        let real_work_id = insert_work(&pool, &format!("Real-MD-{marker}")).await;
        let _ = insert_manifestation(&pool, real_work_id, Some(isbn), &format!("{marker}-a")).await;

        let stub_work_id = insert_work(&pool, &format!("Stub-MD-{marker}")).await;
        let stub_m =
            insert_manifestation(&pool, stub_work_id, Some(isbn), &format!("{marker}-b")).await;

        // Add a manual-source draft on the stub.
        sqlx::query!(
            "INSERT INTO metadata_versions \
               (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
             VALUES ($1, 'manual', 'title', '\"User Override\"'::jsonb, \
                     digest('user-override', 'sha256'), 'isbn', 1.0)",
            stub_m,
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let outcome = rematch_on_isbn_change(&mut tx, stub_m).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            outcome,
            RematchOutcome::Suspected {
                matched_work: real_work_id
            },
            "expected Suspected because of manual draft"
        );

        let stub_exists = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM works WHERE id = $1",
            stub_work_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stub_exists, 1, "stub work should NOT be deleted");

        let dup = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            stub_m,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dup, Some(real_work_id));
    }

    /// No other work has the ISBN → `NoOp`.
    #[sqlx::test(migrations = "./migrations")]
    async fn rematch_noop_when_isbn_unique(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let isbn = "9780000000004";

        let work_id = insert_work(&pool, &format!("Solo-{marker}")).await;
        let m = insert_manifestation(&pool, work_id, Some(isbn), &format!("{marker}-solo")).await;

        let mut tx = pool.begin().await.unwrap();
        let outcome = rematch_on_isbn_change(&mut tx, m).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(outcome, RematchOutcome::NoOp);

        let dup = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            m,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(dup.is_none(), "no suspected pointer should be set");
    }
}
