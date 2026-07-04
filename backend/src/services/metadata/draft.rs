//! Write extracted OPF metadata fields as `metadata_versions` journal rows.
//!
//! After Step 7 the journal is keyed on `value_hash`: repeated observations of
//! the same logical value bump `observation_count` and `last_seen_at` via the
//! `(manifestation_id, source, field_name, value_hash)` unique constraint.
//!
//! Callers pass in an open transaction so the write is committed atomically
//! with the surrounding manifestation insert and canonical-pointer update.

use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use super::extractor::ExtractedMetadata;
use crate::services::enrichment::value_hash;

/// Map of `field_name -> metadata_versions.id` for every row written in this call.
pub type DraftIds = HashMap<String, Uuid>;

/// Write every non-`None` extracted field as an `opf`-source journal row.
///
/// `conn` must be an open connection or transaction; this function does not
/// begin or commit. On a repeat observation of the same `(manifestation_id,
/// field_name, value_hash)` the existing row's `observation_count` and
/// `last_seen_at` are bumped and its `id` is returned unchanged.
///
/// # Errors
///
/// Returns `sqlx::Error` if any individual `INSERT … ON CONFLICT` statement
/// fails (e.g. constraint violation, connection loss, or serialisation failure).
/// Partial writes within an uncommitted transaction are visible to the caller
/// and will be rolled back if the caller rolls back.
#[expect(
    clippy::too_many_lines,
    reason = "write_drafts inserts a draft row per canonical metadata axis; the per-axis cases are mechanical and cannot meaningfully be split without changing the function's transaction contract"
)]
pub async fn write_drafts(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    metadata: &ExtractedMetadata,
) -> Result<DraftIds, sqlx::Error> {
    let confidence = metadata.confidence;
    let mut out: DraftIds = HashMap::new();

    if let Some(title) = &metadata.title {
        let id = insert_draft(
            conn,
            manifestation_id,
            "title",
            &json_string(title),
            confidence,
            "title",
        )
        .await?;
        out.insert("title".into(), id);
    }
    if let Some(subtitle) = &metadata.subtitle {
        let id = insert_draft(
            conn,
            manifestation_id,
            "subtitle",
            &json_string(subtitle),
            confidence,
            "title",
        )
        .await?;
        out.insert("subtitle".into(), id);
    }
    if let Some(desc) = &metadata.description {
        let id = insert_draft(
            conn,
            manifestation_id,
            "description",
            &json_string(desc),
            confidence,
            "title",
        )
        .await?;
        out.insert("description".into(), id);
    }
    if let Some(pub_name) = &metadata.publisher {
        let id = insert_draft(
            conn,
            manifestation_id,
            "publisher",
            &json_string(pub_name),
            confidence,
            "title",
        )
        .await?;
        out.insert("publisher".into(), id);
    }
    if let Some(d) = &metadata.pub_date {
        let id = insert_draft(
            conn,
            manifestation_id,
            "pub_date",
            &json_string(&d.to_string()),
            confidence,
            "title",
        )
        .await?;
        out.insert("pub_date".into(), id);
    }
    if let Some(lang) = &metadata.language {
        let id = insert_draft(
            conn,
            manifestation_id,
            "language",
            &json_string(lang),
            confidence,
            "title",
        )
        .await?;
        out.insert("language".into(), id);
    }
    if let Some(isbn) = &metadata.isbn {
        let match_type = if isbn.valid { "isbn" } else { "title" };
        if let Some(v) = &isbn.isbn_10 {
            let id = insert_draft(
                conn,
                manifestation_id,
                "isbn_10",
                &json_string(v),
                confidence,
                match_type,
            )
            .await?;
            out.insert("isbn_10".into(), id);
        }
        if let Some(v) = &isbn.isbn_13 {
            let id = insert_draft(
                conn,
                manifestation_id,
                "isbn_13",
                &json_string(v),
                confidence,
                match_type,
            )
            .await?;
            out.insert("isbn_13".into(), id);
        }
    }
    if let Some(pages) = metadata.pages {
        let id = insert_draft(
            conn,
            manifestation_id,
            "pages",
            &serde_json::Value::from(pages),
            confidence,
            "title",
        )
        .await?;
        out.insert("pages".into(), id);
    }
    // One journal row per declared role: locking/reviewing an editor's name
    // must never touch the author row alongside it.
    for role in ["author", "editor", "translator", "narrator"] {
        let names: Vec<&str> = metadata
            .creators
            .iter()
            .filter(|c| c.role == role)
            .map(|c| c.name.as_str())
            .collect();
        if names.is_empty() {
            continue;
        }
        let key = format!("contributors.{role}");
        let val = serde_json::to_value(&names).unwrap_or_default();
        let id = insert_draft(conn, manifestation_id, &key, &val, confidence, "title").await?;
        out.insert(key, id);
    }
    if !metadata.unmapped_contributors.is_empty() {
        let val = serde_json::to_value(&metadata.unmapped_contributors).unwrap_or_default();
        // Not inserted into `out`: contributors.other has no apply arm, so no
        // work_authors row ever points back at it.
        insert_draft(
            conn,
            manifestation_id,
            "contributors.other",
            &val,
            confidence,
            "title",
        )
        .await?;
    }
    if !metadata.subjects.is_empty() {
        let val = serde_json::to_value(&metadata.subjects).unwrap_or_default();
        let id = insert_draft(
            conn,
            manifestation_id,
            "subjects",
            &val,
            confidence,
            "title",
        )
        .await?;
        out.insert("subjects".into(), id);
    }
    if let Some(series) = &metadata.series {
        let val = serde_json::to_value(series).unwrap_or_default();
        let id = insert_draft(conn, manifestation_id, "series", &val, confidence, "title").await?;
        out.insert("series".into(), id);
    }

    Ok(out)
}

/// Upsert a single journal row. On repeat observation of the same value,
/// bump `observation_count` and `last_seen_at` and return the existing row id.
async fn insert_draft(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    field_name: &str,
    new_value: &serde_json::Value,
    confidence: f32,
    match_type: &str,
) -> Result<Uuid, sqlx::Error> {
    let hash = value_hash::value_hash(field_name, new_value);
    let id = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
             (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
         VALUES ($1, 'opf', $2, $3, $4, $5, $6) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1 \
         RETURNING id",
        manifestation_id,
        field_name,
        new_value,
        &hash,
        match_type,
        confidence,
    )
    .fetch_one(conn)
    .await?;
    Ok(id)
}

fn json_string(s: &str) -> serde_json::Value {
    serde_json::Value::String(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::metadata::extractor::{ExtractedCreator, ExtractedMetadata, SeriesInfo};
    use crate::services::metadata::isbn::IsbnResult;
    use crate::test_support::db::ingestion_pool_for;
    use sqlx::PgPool;

    async fn setup_manifestation(pool: &PgPool) -> (Uuid, Uuid) {
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('draft_test', 'draft_test') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let file_path = format!("/tmp/draft-test-{work_id}.epub");
        let hash = format!("hash-{work_id}");
        let manifestation_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
             (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
              file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 100, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            file_path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap();

        (work_id, manifestation_id)
    }

    fn sample_metadata() -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some("Draft Test Title".into()),
            sort_title: Some("draft test title".into()),
            subtitle: Some("Draft Test Subtitle".into()),
            description: Some("A description".into()),
            language: Some("en".into()),
            creators: vec![
                ExtractedCreator {
                    name: "Test Writer".into(),
                    sort_name: "Writer, Test".into(),
                    role: "author".into(),
                },
                ExtractedCreator {
                    name: "Test Editor".into(),
                    sort_name: "Editor, Test".into(),
                    role: "editor".into(),
                },
            ],
            unmapped_contributors: vec![
                crate::services::metadata::extractor::UnmappedContributor {
                    name: "Test Illustrator".into(),
                    code: Some("ill".into()),
                },
            ],
            pages: Some(353),
            publisher: Some("Test Publisher".into()),
            pub_date: None,
            isbn: Some(IsbnResult {
                isbn_10: None,
                isbn_13: Some("9780306406157".into()),
                valid: true,
            }),
            subjects: vec!["Fiction".into()],
            series: Some(SeriesInfo {
                name: "Test Series".into(),
                position: Some(1.0),
            }),
            inversion: None,
            confidence: 0.7,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn write_drafts_creates_journal_rows_with_ids(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, manifestation_id) = setup_manifestation(&pool).await;

        let metadata = sample_metadata();

        let mut conn = pool.acquire().await.unwrap();
        let ids = write_drafts(&mut conn, manifestation_id, &metadata)
            .await
            .unwrap();
        drop(conn);

        assert!(ids.contains_key("title"));
        assert!(ids.contains_key("subtitle"));
        assert!(ids.contains_key("isbn_13"));
        assert!(ids.contains_key("pages"));
        assert!(ids.contains_key("contributors.author"));
        assert!(ids.contains_key("contributors.editor"));
        assert!(
            !ids.contains_key("contributors.other"),
            "contributors.other has no apply arm and must not appear in DraftIds"
        );
        assert_eq!(ids.len(), 11, "expected 11 field ids, got {}", ids.len());

        let isbn_match_type = sqlx::query_scalar!(
            "SELECT match_type FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'isbn_13'",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(isbn_match_type, "isbn");

        let author_names: serde_json::Value = sqlx::query_scalar!(
            "SELECT new_value FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.author'",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_names, serde_json::json!(["Test Writer"]));

        let other_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.other'",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            other_count, 1,
            "contributors.other row is written to the journal even though it's not tracked in DraftIds"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn repeat_observation_bumps_count(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, manifestation_id) = setup_manifestation(&pool).await;

        let metadata = sample_metadata();

        let mut conn = pool.acquire().await.unwrap();
        let first = write_drafts(&mut conn, manifestation_id, &metadata)
            .await
            .unwrap();
        let second = write_drafts(&mut conn, manifestation_id, &metadata)
            .await
            .unwrap();
        drop(conn);

        assert_eq!(first.get("title"), second.get("title"));
        assert_eq!(
            first.get("contributors.author"),
            second.get("contributors.author")
        );

        let count = sqlx::query_scalar!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 2);

        let author_count = sqlx::query_scalar!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.author'",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_count, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn tx_rollback_leaves_no_rows(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, manifestation_id) = setup_manifestation(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        write_drafts(&mut tx, manifestation_id, &sample_metadata())
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions WHERE manifestation_id = $1",
            manifestation_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 0);
    }
}
