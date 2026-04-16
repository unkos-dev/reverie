//! Work matching: find existing Work by ISBN or title+author similarity,
//! or create a new one with Author and Series records.

use sqlx::PgPool;
use uuid::Uuid;

use crate::services::metadata::extractor::ExtractedMetadata;

/// Find an existing Work or create a new one. Returns the work_id.
///
/// Matching cascade (in a single transaction):
/// 1. ISBN match: if metadata has valid isbn_13, look up via manifestations table.
/// 2. Title+author fuzzy match via pg_trgm similarity (threshold 0.6).
/// 3. Create new Work + Author(s) + work_authors + optional Series.
pub async fn find_or_create(
    pool: &PgPool,
    metadata: &ExtractedMetadata,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Step 1: ISBN match
    if let Some(ref isbn) = metadata.isbn
        && let Some(ref isbn_13) = isbn.isbn_13
    {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT w.id FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.isbn_13 = $1 \
             LIMIT 1",
        )
        .bind(isbn_13)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(work_id) = existing {
            tx.commit().await?;
            return Ok(work_id);
        }
    }

    // Step 2: Title+author fuzzy match
    if let Some(ref title) = metadata.title
        && let Some(first_author) = metadata.creators.first()
    {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT w.id FROM works w \
             JOIN work_authors wa ON wa.work_id = w.id \
             JOIN authors a ON a.id = wa.author_id \
             WHERE similarity(w.title, $1) > 0.6 \
               AND similarity(a.name, $2) > 0.6 \
             ORDER BY similarity(w.title, $1) DESC \
             LIMIT 1",
        )
        .bind(title)
        .bind(&first_author.name)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(work_id) = existing {
            tx.commit().await?;
            return Ok(work_id);
        }
    }

    // Step 3: Create new Work
    let work_title = metadata.title.as_deref().unwrap_or("Unknown");
    let work_sort_title = metadata.sort_title.as_deref().unwrap_or(work_title);
    let work_id: Uuid = sqlx::query_scalar(
        "INSERT INTO works (title, sort_title, description, language) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(work_title)
    .bind(work_sort_title)
    .bind(metadata.description.as_deref())
    .bind(metadata.language.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    // Create or find authors and link via work_authors
    for (i, creator) in metadata.creators.iter().enumerate() {
        let author_id: Uuid =
            find_or_create_author(&mut tx, &creator.name, &creator.sort_name).await?;

        sqlx::query(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, $3::author_role, $4) \
             ON CONFLICT (work_id, author_id, role) DO NOTHING",
        )
        .bind(work_id)
        .bind(author_id)
        .bind(&creator.role)
        .bind(i as i32)
        .execute(&mut *tx)
        .await?;
    }

    // Step 4: Series linking
    if let Some(ref series) = metadata.series {
        let series_id: Uuid =
            find_or_create_series(&mut tx, &series.name, &series.name.to_lowercase()).await?;

        sqlx::query(
            "INSERT INTO series_works (series_id, work_id, position) \
             VALUES ($1, $2, $3)",
        )
        .bind(series_id)
        .bind(work_id)
        .bind(series.position)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(work_id)
}

/// Find an existing author by name or create a new one.
/// Uses ON CONFLICT to safely handle concurrent inserts (UNIQUE on authors.name).
async fn find_or_create_author(
    conn: &mut sqlx::PgConnection,
    name: &str,
    sort_name: &str,
) -> Result<Uuid, sqlx::Error> {
    // DO UPDATE SET name = EXCLUDED.name is a no-op trick to make RETURNING work
    // on the conflict path (DO NOTHING doesn't return the existing row).
    sqlx::query_scalar(
        "INSERT INTO authors (name, sort_name) VALUES ($1, $2) \
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
    )
    .bind(name)
    .bind(sort_name)
    .fetch_one(&mut *conn)
    .await
}

/// Find an existing series by name or create a new one.
/// Uses ON CONFLICT to safely handle concurrent inserts (UNIQUE on series.name).
async fn find_or_create_series(
    conn: &mut sqlx::PgConnection,
    name: &str,
    sort_name: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO series (name, sort_name) VALUES ($1, $2) \
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
    )
    .bind(name)
    .bind(sort_name)
    .fetch_one(&mut *conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::metadata::extractor::{ExtractedCreator, SeriesInfo};
    use crate::services::metadata::isbn::IsbnResult;

    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        })
    }

    fn test_metadata(title: &str, author: &str) -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some(title.into()),
            sort_title: Some(title.to_lowercase()),
            description: None,
            language: Some("en".into()),
            creators: vec![ExtractedCreator {
                name: author.into(),
                sort_name: format!("{author}, Test"),
                role: "author".into(),
            }],
            publisher: None,
            pub_date: None,
            isbn: None,
            subjects: vec![],
            series: None,
            inversion: None,
            confidence: 0.5,
        }
    }

    /// Clean up work + cascading records created during a test.
    async fn cleanup_work(pool: &PgPool, work_id: Uuid) {
        // work_authors and series_works cascade from works ON DELETE CASCADE
        // but authors and series rows are independent — clean them explicitly
        let author_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT author_id FROM work_authors WHERE work_id = $1")
                .bind(work_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        let series_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT series_id FROM series_works WHERE work_id = $1")
                .bind(work_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        // Delete manifestations first (FK to works)
        let _ = sqlx::query("DELETE FROM manifestations WHERE work_id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
        // Delete work (cascades work_authors, series_works)
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
        // Clean orphaned authors
        for aid in author_ids {
            let _ = sqlx::query("DELETE FROM authors WHERE id = $1")
                .bind(aid)
                .execute(pool)
                .await;
        }
        // Clean orphaned series
        for sid in series_ids {
            let _ = sqlx::query("DELETE FROM series WHERE id = $1")
                .bind(sid)
                .execute(pool)
                .await;
        }
    }

    #[tokio::test]
    #[ignore] // requires PostgreSQL with migrations applied
    async fn find_or_create_new_work() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        let meta = test_metadata("Test Book Alpha", "Test Author Alpha");
        let work_id = find_or_create(&pool, &meta).await.unwrap();

        // Verify work exists
        let title: String = sqlx::query_scalar("SELECT title FROM works WHERE id = $1")
            .bind(work_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(title, "Test Book Alpha");

        // Verify author linked
        let author_name: String = sqlx::query_scalar(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             WHERE wa.work_id = $1",
        )
        .bind(work_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_name, "Test Author Alpha");

        cleanup_work(&pool, work_id).await;
    }

    #[tokio::test]
    #[ignore]
    async fn find_or_create_deduplicates_authors() {
        let pool = PgPool::connect(&db_url()).await.unwrap();

        // Use maximally distinct titles so pg_trgm similarity stays well below 0.6.
        // "Book One By Same" / "Book Two By Same" share too many trigrams and
        // were incorrectly treated as the same work.
        let meta1 = test_metadata("Astronomy Fundamentals", "Shared Author Name");
        let meta2 = test_metadata("Renaissance Cooking Guide", "Shared Author Name");

        let work_id1 = find_or_create(&pool, &meta1).await.unwrap();
        let work_id2 = find_or_create(&pool, &meta2).await.unwrap();
        assert_ne!(work_id1, work_id2);

        // Both works should reference the SAME author row
        let author_id1: Uuid =
            sqlx::query_scalar("SELECT author_id FROM work_authors WHERE work_id = $1 LIMIT 1")
                .bind(work_id1)
                .fetch_one(&pool)
                .await
                .unwrap();
        let author_id2: Uuid =
            sqlx::query_scalar("SELECT author_id FROM work_authors WHERE work_id = $1 LIMIT 1")
                .bind(work_id2)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(author_id1, author_id2, "same author should be reused");

        // Only one author row should exist for this name
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM authors WHERE name = 'Shared Author Name'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);

        cleanup_work(&pool, work_id1).await;
        cleanup_work(&pool, work_id2).await;
    }

    #[tokio::test]
    #[ignore]
    async fn find_or_create_deduplicates_series() {
        let pool = PgPool::connect(&db_url()).await.unwrap();

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

        let work_id1 = find_or_create(&pool, &meta1).await.unwrap();
        let work_id2 = find_or_create(&pool, &meta2).await.unwrap();

        // Only one series row should exist
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM series WHERE name = 'Test Series Dedup'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);

        cleanup_work(&pool, work_id1).await;
        cleanup_work(&pool, work_id2).await;
    }

    #[tokio::test]
    #[ignore]
    async fn find_or_create_matches_by_isbn() {
        let pool = PgPool::connect(&db_url()).await.unwrap();

        // Create first work with ISBN
        let mut meta1 = test_metadata("ISBN Match Test", "ISBN Author");
        meta1.isbn = Some(IsbnResult {
            isbn_10: Some("0306406152".into()),
            isbn_13: Some("9780306406157".into()),
            valid: true,
        });
        let work_id1 = find_or_create(&pool, &meta1).await.unwrap();

        // Insert a manifestation so the ISBN join works
        let _ = sqlx::query(
            "INSERT INTO manifestations \
             (work_id, isbn_13, format, file_path, file_hash, file_size_bytes, \
              ingestion_status, validation_status) \
             VALUES ($1, $2, 'epub'::manifestation_format, $3, 'testhash', 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status)",
        )
        .bind(work_id1)
        .bind("9780306406157")
        .bind(format!("/tmp/test-isbn-{work_id1}.epub"))
        .execute(&pool)
        .await
        .unwrap();

        // Second call with same ISBN should return existing work
        let mut meta2 = test_metadata("Different Title", "Different Author");
        meta2.isbn = Some(IsbnResult {
            isbn_10: Some("0306406152".into()),
            isbn_13: Some("9780306406157".into()),
            valid: true,
        });
        let work_id2 = find_or_create(&pool, &meta2).await.unwrap();
        assert_eq!(work_id1, work_id2, "ISBN match should return existing work");

        cleanup_work(&pool, work_id1).await;
    }
}
