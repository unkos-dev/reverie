//! Per-source aggregate ratings cache, keyed `(manifestation_id, source)`.
//!
//! Schema lives in migration `20260724120000_external_identifiers_ratings`.
//! Ratings are a refreshable multi-source cache, deliberately modeled unlike
//! identifiers: each provider is independently authoritative for its own
//! scale, so there is no single canonical value to
//! reconcile. Ratings therefore live OUTSIDE the journal/policy/lock engine
//! (no `source_version_id`, never staged, never written back) and are only
//! ever written by enrichment via `upsert_rating`. Users read them but
//! cannot write them (RLS default-deny on `reverie_app`).
//!
//! `source` FKs `rating_sources` (the rating-capable subset), so a provenance
//! source that cannot carry a rating (e.g. `manual`) is rejected at the DB.

use time::OffsetDateTime;
use uuid::Uuid;

/// One cached rating row for a `(manifestation, source)` pair.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ManifestationExternalRating {
    /// Owning manifestation (edition).
    pub manifestation_id: Uuid,
    /// Rating provider (`rating_sources.id`).
    pub source: String,
    /// The provider's score on its own scale (e.g. Google's 4.3 out of 5).
    pub rating: f32,
    /// The provider's maximum (e.g. 5). Stored per row because scales differ.
    pub rating_scale: f32,
    /// Number of reviews backing the score, if the provider reports it.
    pub review_count: i32,
    /// When this cache row was last refreshed.
    pub fetched_at: OffsetDateTime,
}

/// Upsert a rating for a `(manifestation_id, source)` pair, refreshing the
/// score/scale/count and stamping `fetched_at = now()`. Idempotent: a re-run
/// with the same score updates the one row in place (no duplicate). Two
/// manifestations (editions) with the same source keep independent rows.
pub async fn upsert_rating(
    executor: impl sqlx::PgExecutor<'_>,
    manifestation_id: Uuid,
    source: &str,
    rating: f32,
    rating_scale: f32,
    review_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO manifestation_external_ratings \
             (manifestation_id, source, rating, rating_scale, review_count, fetched_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (manifestation_id, source) \
         DO UPDATE SET rating = EXCLUDED.rating, \
                       rating_scale = EXCLUDED.rating_scale, \
                       review_count = EXCLUDED.review_count, \
                       fetched_at = now()",
        manifestation_id,
        source,
        rating,
        rating_scale,
        review_count,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Remove the cached rating for a `(manifestation_id, source)` pair.
/// Used when a rating-capable provider record is re-fetched and no longer
/// reports a rating, so the cache reflects the provider's removal instead
/// of serving the old score indefinitely. Returns whether a row existed.
pub async fn delete_rating(
    executor: impl sqlx::PgExecutor<'_>,
    manifestation_id: Uuid,
    source: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM manifestation_external_ratings \
         WHERE manifestation_id = $1 AND source = $2",
        manifestation_id,
        source,
    )
    .execute(executor)
    .await?;
    Ok(done.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::acquire_with_rls;
    use crate::test_support::db::{
        add_to_shelf, app_pool_for, create_adult_and_basic_auth, create_child_user_and_basic_auth,
        create_shelf, ingestion_pool_for, insert_work_and_manifestation, readonly_pool_for,
    };
    use sqlx::PgPool;

    fn is_rls_denied(err: &sqlx::Error) -> bool {
        err_code(err).as_deref() == Some("42501")
    }

    fn err_code(err: &sqlx::Error) -> Option<std::borrow::Cow<'_, str>> {
        err.as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < f32::EPSILON
    }

    // ---- upsert idempotency + independence ----

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_updates_in_place(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-upsert").await;
        upsert_rating(&pool, m, "googlebooks", 4.0, 5.0, 100)
            .await
            .expect("first");
        upsert_rating(&pool, m, "googlebooks", 4.5, 5.0, 250)
            .await
            .expect("second");

        let rows = sqlx::query_as!(
            ManifestationExternalRating,
            "SELECT manifestation_id, source, rating, rating_scale, review_count, fetched_at \
             FROM manifestation_external_ratings WHERE manifestation_id = $1",
            m,
        )
        .fetch_all(&pool)
        .await
        .expect("select");
        assert_eq!(rows.len(), 1, "one row per (manifestation, source)");
        assert!(approx(rows[0].rating, 4.5), "rating updated in place");
        assert_eq!(rows[0].review_count, 250);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn two_editions_keep_independent_ratings(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w1, m1) = insert_work_and_manifestation(&ingestion, "rating-ed1").await;
        let (_w2, m2) = insert_work_and_manifestation(&ingestion, "rating-ed2").await;
        // Same provider, different volume ratings — no cross-edition clobber.
        upsert_rating(&pool, m1, "googlebooks", 3.0, 5.0, 10)
            .await
            .expect("ed1");
        upsert_rating(&pool, m2, "googlebooks", 5.0, 5.0, 20)
            .await
            .expect("ed2");

        let r1 = sqlx::query_scalar!(
            "SELECT rating FROM manifestation_external_ratings WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m1,
        )
        .fetch_one(&pool)
        .await
        .expect("r1");
        let r2 = sqlx::query_scalar!(
            "SELECT rating FROM manifestation_external_ratings WHERE manifestation_id = $1 AND source = 'googlebooks'",
            m2,
        )
        .fetch_one(&pool)
        .await
        .expect("r2");
        assert!(approx(r1, 3.0), "edition 1 keeps its rating");
        assert!(approx(r2, 5.0), "edition 2 keeps its rating");
    }

    // ---- FK + CHECK boundaries ----

    #[sqlx::test(migrations = "./migrations")]
    async fn source_fk_rejects_non_rating_source(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-fk").await;
        // `manual` exists in metadata_sources but is NOT rating-capable.
        let err = upsert_rating(&pool, m, "manual", 4.0, 5.0, 1)
            .await
            .expect_err("source=manual must be FK-rejected");
        assert_eq!(
            err_code(&err).as_deref(),
            Some("23503"),
            "foreign_key_violation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rating_out_of_range_rejected(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-range").await;
        assert!(
            upsert_rating(&pool, m, "googlebooks", 6.0, 5.0, 1)
                .await
                .is_err(),
            "rating above scale rejected"
        );
        assert!(
            upsert_rating(&pool, m, "googlebooks", -0.5, 5.0, 1)
                .await
                .is_err(),
            "negative rating rejected"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn negative_review_count_rejected(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-count").await;
        assert!(
            upsert_rating(&pool, m, "googlebooks", 4.0, 5.0, -1)
                .await
                .is_err(),
            "negative review_count rejected"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn nonpositive_scale_rejected(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-scale").await;
        assert!(
            upsert_rating(&pool, m, "googlebooks", 0.0, 0.0, 1)
                .await
                .is_err(),
            "rating_scale = 0 rejected"
        );
    }

    // ---- cascade ----

    #[sqlx::test(migrations = "./migrations")]
    async fn deleting_manifestation_cascades_ratings(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rating-cascade").await;
        upsert_rating(&pool, m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect("seed");
        sqlx::query!("DELETE FROM manifestations WHERE id = $1", m)
            .execute(&pool)
            .await
            .expect("delete manifestation");
        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM manifestation_external_ratings WHERE manifestation_id = $1",
            m,
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(remaining, 0, "ratings cascade-deleted with manifestation");
    }

    // ---- RLS ----

    #[sqlx::test(migrations = "./migrations")]
    async fn child_reads_only_shelf_visible_ratings(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "rat-vis").await;
        let (_hw, hidden_m) = insert_work_and_manifestation(&ingestion, "rat-hid").await;
        upsert_rating(&pool, visible_m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect("seed visible");
        upsert_rating(&pool, hidden_m, "googlebooks", 2.0, 5.0, 1)
            .await
            .expect("seed hidden");

        let (child_id, _) = create_child_user_and_basic_auth(&app, "rat-child").await;
        let shelf = create_shelf(&app, child_id, "kids").await;
        add_to_shelf(&app, shelf, visible_m).await;

        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let rows: Vec<Uuid> = sqlx::query_scalar!(
            "SELECT manifestation_id FROM manifestation_external_ratings ORDER BY manifestation_id",
        )
        .fetch_all(&mut *tx)
        .await
        .expect("child select");
        assert_eq!(
            rows,
            vec![visible_m],
            "child sees only shelf-visible rating"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rls_enabled_fresh_child_sees_no_rating_rows(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rat-enable").await;
        upsert_rating(&pool, m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect("seed");
        let (child_id, _) = create_child_user_and_basic_auth(&app, "rat-enable").await;
        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let count: i64 =
            sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM manifestation_external_ratings",)
                .fetch_one(&mut *tx)
                .await
                .expect("select");
        assert_eq!(count, 0, "RLS ENABLE hides ratings from a shelf-less child");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn user_cannot_write_ratings(pool: PgPool) {
        // Ratings are enrichment-write-only. No reverie_app write policy
        // exists → default-deny for adult/admin too.
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rat-nouser").await;
        let (adult_id, _) = create_adult_and_basic_auth(&app, "rat-nouser").await;
        let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
        let err = upsert_rating(&mut *tx, m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect_err("adult rating write denied");
        assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ingestion_can_upsert_ratings(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rat-ing").await;
        upsert_rating(&ingestion, m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect("ingestion upsert succeeds");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn readonly_can_read_ratings(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let readonly = readonly_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "rat-ro").await;
        upsert_rating(&pool, m, "googlebooks", 4.0, 5.0, 1)
            .await
            .expect("seed");
        let (adult_id, _) = create_adult_and_basic_auth(&app, "rat-ro").await;
        let mut tx = acquire_with_rls(&readonly, adult_id).await.expect("rls tx");
        let count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM manifestation_external_ratings WHERE manifestation_id = $1",
            m,
        )
        .fetch_one(&mut *tx)
        .await
        .expect("readonly select");
        assert_eq!(count, 1, "readonly (adult) reads visible ratings");
    }
}
