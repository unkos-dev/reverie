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

/// Write every non-None extracted field as an `opf`-source journal row.
///
/// * `conn` — open connection or transaction; this function does not begin or commit.
/// * Returns the version IDs so the caller can wire canonical pointer columns.
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
    if !metadata.creators.is_empty() {
        let val = serde_json::to_value(&metadata.creators).unwrap_or_default();
        let id = insert_draft(
            conn,
            manifestation_id,
            "creators",
            &val,
            confidence,
            "title",
        )
        .await?;
        out.insert("creators".into(), id);
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
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO metadata_versions \
             (manifestation_id, source, field_name, new_value, value_hash, match_type, confidence_score) \
         VALUES ($1, 'opf', $2, $3, $4, $5, $6) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1 \
         RETURNING id",
    )
    .bind(manifestation_id)
    .bind(field_name)
    .bind(new_value)
    .bind(&hash)
    .bind(match_type)
    .bind(confidence)
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
    use sqlx::{Connection, PgConnection, PgPool};

    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        })
    }

    async fn setup_manifestation(pool: &PgPool) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ('draft_test', 'draft_test') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let manifestation_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
             (work_id, format, file_path, file_hash, file_size_bytes, \
              ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, 100, \
                     'complete'::ingestion_status, 'valid'::validation_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(format!("/tmp/draft-test-{work_id}.epub"))
        .bind(format!("hash-{work_id}"))
        .fetch_one(pool)
        .await
        .unwrap();

        (work_id, manifestation_id)
    }

    async fn cleanup(pool: &PgPool, work_id: Uuid, manifestation_id: Uuid) {
        let _ = sqlx::query("DELETE FROM metadata_versions WHERE manifestation_id = $1")
            .bind(manifestation_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM manifestations WHERE id = $1")
            .bind(manifestation_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
    }

    fn sample_metadata() -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some("Draft Test Title".into()),
            sort_title: Some("draft test title".into()),
            description: Some("A description".into()),
            language: Some("en".into()),
            creators: vec![ExtractedCreator {
                name: "Test Writer".into(),
                sort_name: "Writer, Test".into(),
                role: "author".into(),
            }],
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

    #[tokio::test]
    #[ignore]
    async fn write_drafts_creates_journal_rows_with_ids() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        let (work_id, manifestation_id) = setup_manifestation(&pool).await;

        let metadata = sample_metadata();

        let mut conn = pool.acquire().await.unwrap();
        let ids = write_drafts(&mut conn, manifestation_id, &metadata)
            .await
            .unwrap();
        drop(conn);

        assert!(ids.contains_key("title"));
        assert!(ids.contains_key("isbn_13"));
        assert!(ids.contains_key("creators"));
        assert_eq!(ids.len(), 8, "expected 8 field ids, got {}", ids.len());

        let isbn_match_type: String = sqlx::query_scalar(
            "SELECT match_type FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'isbn_13'",
        )
        .bind(manifestation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(isbn_match_type, "isbn");

        cleanup(&pool, work_id, manifestation_id).await;
    }

    #[tokio::test]
    #[ignore]
    async fn repeat_observation_bumps_count() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        let (work_id, manifestation_id) = setup_manifestation(&pool).await;

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

        let count: i32 = sqlx::query_scalar(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'title'",
        )
        .bind(manifestation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 2);

        cleanup(&pool, work_id, manifestation_id).await;
    }

    #[tokio::test]
    #[ignore]
    async fn tx_rollback_leaves_no_rows() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        let (work_id, manifestation_id) = setup_manifestation(&pool).await;

        let mut conn = PgConnection::connect(&db_url()).await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        write_drafts(&mut tx, manifestation_id, &sample_metadata())
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions WHERE manifestation_id = $1",
        )
        .bind(manifestation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 0);

        cleanup(&pool, work_id, manifestation_id).await;
    }
}
