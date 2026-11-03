//! Work matching + creation.
//!
//! The flow is split so the orchestrator can honour the ingest
//! invariant (every canonical field has a matching `metadata_versions` row):
//!
//! 1. ISBN candidates preserve separate works; `match_existing` resolves
//!    title and author matches only when no ISBN candidate exists.
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

/// Outcome of `rematch_on_isbn_change`.
/// Consumed by the enrichment orchestrator.
#[derive(Debug, PartialEq, Eq)]
pub enum RematchOutcome {
    /// No matching ISBN row elsewhere; any obsolete candidate pointer is cleared.
    NoOp,
    /// A potential duplicate is recorded without changing work membership.
    Suspected {
        /// Work id of the suspected duplicate.
        matched_work: Uuid,
    },
}

/// Return an attachable work or an ISBN candidate, preserving candidate independence.
/// Candidate lookup locks matching rows but never writes.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the candidate lookup queries.
pub async fn match_existing(
    conn: &mut PgConnection,
    metadata: &ExtractedMetadata,
) -> Result<(Option<Uuid>, Option<Uuid>), sqlx::Error> {
    let candidate = isbn_candidate(
        conn,
        metadata.isbn_10.as_deref(),
        metadata.isbn_13.as_deref(),
        None,
    )
    .await?;
    if candidate.is_some() {
        return Ok((None, candidate));
    }
    if let Some(title) = &metadata.title
        && let Some(first_author) = metadata.first_author()
    {
        let hit = sqlx::query_scalar!(
            "SELECT w.id FROM works w \
             JOIN work_authors wa ON wa.work_id = w.id \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.role = 'author' \
               AND similarity(w.title, $1) > 0.6 \
               AND similarity(a.name, $2) > 0.6 \
             ORDER BY similarity(w.title, $1) DESC \
             LIMIT 1",
            title,
            &first_author.name,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if hit.is_some() {
            return Ok((hit, None));
        }
    }

    Ok((None, None))
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
///
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
/// match-or-create flow without draft wiring.
///
/// Retained for call sites that
/// don't participate in the ingest-invariant flow (primarily the existing
/// tests).
///
/// # Errors
///
/// Returns [`sqlx::Error`] if the transaction cannot begin or commit, or from
/// any of [`match_existing`], [`create_stub`], or [`upgrade_stub`].
#[cfg(test)]
pub async fn find_or_create(
    pool: &PgPool,
    metadata: &ExtractedMetadata,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let (matched, _) = match_existing(&mut tx, metadata).await?;
    if let Some(id) = matched {
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

/// Find one possible duplicate from supplied ISBNs and comparison-only equivalents.
///
/// # Errors
/// Returns database errors from the bounded, locked candidate lookup.
pub(crate) async fn isbn_candidate(
    conn: &mut PgConnection,
    isbn_10: Option<&str>,
    isbn_13: Option<&str>,
    excluded_work: Option<Uuid>,
) -> Result<Option<Uuid>, sqlx::Error> {
    let mut tens = Vec::new();
    let mut thirteens = Vec::new();
    if let Some(raw) = isbn_10
        && let Ok(value) = raw.parse::<isbn::Isbn10>()
    {
        tens.push(value.to_string());
        thirteens.push(isbn::Isbn13::from(value).to_string());
    }
    if let Some(raw) = isbn_13
        && let Ok(value) = raw.parse::<isbn::Isbn13>()
    {
        let compact = value.to_string();
        if compact.starts_with("978") || compact.starts_with("979") {
            thirteens.push(compact);
            if let Ok(ten) = isbn::Isbn10::try_from(value) {
                tens.push(ten.to_string());
            }
        }
    }
    if tens.is_empty() && thirteens.is_empty() {
        return Ok(None);
    }
    sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         WHERE (m.isbn_10 = ANY($1) OR m.isbn_13 = ANY($2)) \
           AND ($3::uuid IS NULL OR m.work_id != $3) \
         ORDER BY m.work_id, m.id LIMIT 1 FOR UPDATE",
        &tens,
        &thirteens,
        excluded_work,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Refresh the possible-duplicate pointer without changing work membership.
///
/// # Errors
/// Returns errors from manifestation locking, candidate lookup or pointer update.
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
    let candidate = isbn_candidate(
        conn,
        row.isbn_10.as_deref(),
        row.isbn_13.as_deref(),
        Some(row.work_id),
    )
    .await?;
    sqlx::query!(
        "UPDATE manifestations SET suspected_duplicate_work_id = $1 WHERE id = $2",
        candidate,
        manifestation_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(candidate.map_or(RematchOutcome::NoOp, |matched_work| {
        RematchOutcome::Suspected { matched_work }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    // REV-REQ-0073 AC1.
    #[sqlx::test(migrations = "./migrations")]
    async fn isbn_contract_candidates(pool: PgPool) {
        use crate::models::manifestation_format::ManifestationFormat;
        use crate::models::validation_status::ValidationStatus;
        use crate::services::files::LibraryLocation;
        use crate::services::ingestion::{ManifestationMeta, commit_ingest, copier::CopyResult};
        let pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let mut metadata = test_metadata(&format!("Candidate {marker}"), "Candidate Author");
        let existing = find_or_create(&pool, &metadata).await.unwrap();
        let old_m = insert_manifestation(
            &pool,
            existing,
            Some("9780306406157"),
            &format!("{marker}-existing"),
        )
        .await;
        metadata.isbn_10 = Some("0306406152".into());
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let location = LibraryLocation {
            library_id,
            path: format!("candidate-{marker}.epub").parse().unwrap(),
        };
        let copied = CopyResult {
            dest_path: location.path.as_str().into(),
            sha256: marker.clone(),
            file_size: 1000,
            identity: (1, 1),
        };
        let (independent, manifestation) = commit_ingest(
            &pool,
            &Some(metadata),
            &std::collections::HashMap::new(),
            &location,
            &copied,
            ManifestationMeta {
                format: ManifestationFormat::Epub,
                validation_status: ValidationStatus::Clean,
                accessibility_metadata: &None,
                has_embedded_cover: Some(false),
                current_hash: &copied.sha256,
                current_size: 1000,
            },
        )
        .await
        .unwrap();
        assert_ne!(independent, existing);
        let row = sqlx::query!("SELECT work_id, suspected_duplicate_work_id, isbn_10, isbn_13, isbn_10_version_id, isbn_13_version_id FROM manifestations WHERE id = $1", manifestation)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.work_id, independent);
        assert_eq!(row.suspected_duplicate_work_id, Some(existing));
        assert_eq!(row.isbn_10.as_deref(), Some("0306406152"));
        assert!(row.isbn_10_version_id.is_some());
        assert!(row.isbn_13.is_none() && row.isbn_13_version_id.is_none());
        sqlx::query!(
            "UPDATE manifestations SET isbn_10 = $1 WHERE id = $2",
            "0306406152",
            old_m
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let outcome = rematch_on_isbn_change(&mut tx, manifestation)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            outcome,
            RematchOutcome::Suspected {
                matched_work: existing
            }
        );
        let work = sqlx::query_scalar!(
            "SELECT work_id FROM manifestations WHERE id = $1",
            manifestation
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(work, independent);
        let work = sqlx::query_scalar!("SELECT work_id FROM manifestations WHERE id = $1", old_m)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(work, existing);
        let surviving = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM works WHERE id = $1",
            independent
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(surviving, 1);
    }

    use crate::services::metadata::extractor::ExtractedCreator;
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
            isbn_10: None,
            isbn_13: None,
            subjects: vec![],
            series: None,
            inversion: None,
            confidence: 0.5,
        }
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
    /// Helper: insert a manifestation row for a given work/ISBN combo.
    /// Returns the manifestation id.
    async fn insert_manifestation(
        pool: &PgPool,
        work_id: Uuid,
        isbn_13: Option<&str>,
        file_marker: &str,
    ) -> Uuid {
        let path = format!("fixtures/rematch-{file_marker}.epub");
        let hash = format!("hash-{file_marker}");
        sqlx::query_scalar!(
            "WITH inserted AS (INSERT INTO manifestations \
               (library_id, work_id, isbn_13, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, $2, 'epub'::manifestation_format, $3, $4, $4, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
            work_id,
            isbn_13,
            path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

}
