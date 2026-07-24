//! External-source identifier registry: work-level and manifestation-level
//! tables holding one `external_id` per `(entity, scheme)`.
//!
//! Schema lives in migration `20260724120000_external_identifiers_ratings`.
//! Unlike the ratings cache, identifiers are journal-integrated (each row
//! carries a `source_version_id` pointer into `metadata_versions`) and are
//! hand-editable through the metadata PATCH pipeline. Both tables enforce
//! their own row-level security so a direct read cannot bypass authorization.
//!
//! The canonical metadata field name for an identifier is
//! `identifiers.<level>.<scheme>`. Level is carried explicitly (see
//! `IdentifierLevel`) because it is not derivable from the scheme: Open
//! Library issues both work ids (`OL…W`) and edition ids (`OL…M`), which live
//! at different FRBR levels under the same `openlibrary` scheme.

use uuid::Uuid;

/// FRBR level an external identifier is attached to.
///
/// A work-level id (e.g. an Open Library work `OL…W`) lives in
/// `work_external_identifiers`; a manifestation/edition-level id (e.g. an
/// Open Library edition `OL…M`, or a Google Books volume) lives in
/// `manifestation_external_identifiers`. The level is the middle segment of
/// the canonical field name `identifiers.<level>.<scheme>` and routes both the
/// journal `field_name` and the target table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierLevel {
    /// Work-level id (e.g. an Open Library work `OL…W`).
    Work,
    /// Manifestation/edition-level id (e.g. an Open Library edition `OL…M`).
    Manifestation,
}

impl IdentifierLevel {
    /// The wire/field-name segment (`"work"` or `"manifestation"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Manifestation => "manifestation",
        }
    }

    /// Parse the level segment of a canonical identifier field name.
    pub fn from_segment(segment: &str) -> Option<Self> {
        match segment {
            "work" => Some(Self::Work),
            "manifestation" => Some(Self::Manifestation),
            _ => None,
        }
    }

    /// Canonical metadata field name `identifiers.<level>.<scheme>` for this
    /// level and scheme. This is the `field_name` written to
    /// `metadata_versions`, the key `policy::decide` dispatches on, and the
    /// addressing key in the PATCH payload.
    pub fn canonical_field(self, scheme: &str) -> String {
        format!("identifiers.{}.{scheme}", self.as_str())
    }
}

/// One work-level identifier row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WorkExternalIdentifier {
    /// Owning work.
    pub work_id: Uuid,
    /// Identifier scheme (`identifier_schemes.id`).
    pub scheme: String,
    /// The external id value on that scheme.
    pub external_id: String,
    /// Journal pointer to the `metadata_versions` row that set this value.
    pub source_version_id: Option<Uuid>,
}

/// One manifestation-level identifier row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ManifestationExternalIdentifier {
    /// Owning manifestation.
    pub manifestation_id: Uuid,
    /// Identifier scheme (`identifier_schemes.id`).
    pub scheme: String,
    /// The external id value on that scheme.
    pub external_id: String,
    /// Journal pointer to the `metadata_versions` row that set this value.
    pub source_version_id: Option<Uuid>,
}

/// Upsert a work-level identifier, replacing any existing value for the
/// `(work_id, scheme)` slot in place (one value per (entity, scheme)). Both the
/// manual PATCH path and enrichment reuse this.
pub async fn upsert_work_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    work_id: Uuid,
    scheme: &str,
    external_id: &str,
    source_version_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO work_external_identifiers \
             (work_id, scheme, external_id, source_version_id) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (work_id, scheme) \
         DO UPDATE SET external_id = EXCLUDED.external_id, \
                       source_version_id = EXCLUDED.source_version_id",
        work_id,
        scheme,
        external_id,
        source_version_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Upsert a manifestation-level identifier, replacing any existing value for
/// the `(manifestation_id, scheme)` slot in place (one value per (entity, scheme)).
pub async fn upsert_manifestation_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    manifestation_id: Uuid,
    scheme: &str,
    external_id: &str,
    source_version_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO manifestation_external_identifiers \
             (manifestation_id, scheme, external_id, source_version_id) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (manifestation_id, scheme) \
         DO UPDATE SET external_id = EXCLUDED.external_id, \
                       source_version_id = EXCLUDED.source_version_id",
        manifestation_id,
        scheme,
        external_id,
        source_version_id,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Fetch the current work-level `external_id` for a `(work_id, scheme)` slot,
/// if any. Used to load registry state into the enrichment snapshot and to
/// resolve the current value on a manual clear.
pub async fn get_work_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    work_id: Uuid,
    scheme: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT external_id FROM work_external_identifiers \
         WHERE work_id = $1 AND scheme = $2",
        work_id,
        scheme,
    )
    .fetch_optional(executor)
    .await
}

/// Fetch the current manifestation-level `external_id` for a
/// `(manifestation_id, scheme)` slot, if any.
pub async fn get_manifestation_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    manifestation_id: Uuid,
    scheme: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT external_id FROM manifestation_external_identifiers \
         WHERE manifestation_id = $1 AND scheme = $2",
        manifestation_id,
        scheme,
    )
    .fetch_optional(executor)
    .await
}

/// Delete a work-level identifier slot. Returns whether a row was removed.
pub async fn delete_work_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    work_id: Uuid,
    scheme: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM work_external_identifiers WHERE work_id = $1 AND scheme = $2",
        work_id,
        scheme,
    )
    .execute(executor)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Delete a manifestation-level identifier slot. Returns whether a row was
/// removed.
pub async fn delete_manifestation_identifier(
    executor: impl sqlx::PgExecutor<'_>,
    manifestation_id: Uuid,
    scheme: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM manifestation_external_identifiers \
         WHERE manifestation_id = $1 AND scheme = $2",
        manifestation_id,
        scheme,
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
        // 42501 = insufficient_privilege: the clean RLS policy miss.
        err_code(err).as_deref() == Some("42501")
    }

    fn err_code(err: &sqlx::Error) -> Option<std::borrow::Cow<'_, str>> {
        err.as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
    }

    // ---- level helper ----

    #[test]
    fn canonical_field_carries_level_and_scheme() {
        assert_eq!(
            IdentifierLevel::Work.canonical_field("openlibrary"),
            "identifiers.work.openlibrary"
        );
        assert_eq!(
            IdentifierLevel::Manifestation.canonical_field("googlebooks"),
            "identifiers.manifestation.googlebooks"
        );
        assert_eq!(
            IdentifierLevel::from_segment("work"),
            Some(IdentifierLevel::Work)
        );
        assert_eq!(IdentifierLevel::from_segment("edition"), None);
    }

    // ---- single-slot replacement + constraints (owner pool bypasses RLS) ----

    #[sqlx::test(migrations = "./migrations")]
    async fn manifestation_upsert_replaces_in_place(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "midd-replace").await;

        upsert_manifestation_identifier(&pool, m, "googlebooks", "zyTZAAAAYAAJ", None)
            .await
            .expect("first upsert");
        upsert_manifestation_identifier(&pool, m, "googlebooks", "NEWvalue123", None)
            .await
            .expect("replacing upsert");

        let rows: Vec<String> = sqlx::query_scalar!(
            "SELECT external_id FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'googlebooks'",
            m,
        )
        .fetch_all(&pool)
        .await
        .expect("select");
        assert_eq!(
            rows,
            vec!["NEWvalue123".to_string()],
            "single slot, replaced"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn work_upsert_replaces_in_place(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (w, _m) = insert_work_and_manifestation(&ingestion, "widd-replace").await;

        upsert_work_identifier(&pool, w, "openlibrary", "OL111W", None)
            .await
            .expect("first upsert");
        upsert_work_identifier(&pool, w, "openlibrary", "OL222W", None)
            .await
            .expect("replacing upsert");

        let got = get_work_identifier(&pool, w, "openlibrary")
            .await
            .expect("get");
        assert_eq!(got, Some("OL222W".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scheme_fk_rejects_non_scheme_value(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "scheme-fk").await;
        // `manual` is a provenance source, not an identifier scheme.
        let err = upsert_manifestation_identifier(&pool, m, "manual", "whatever", None)
            .await
            .expect_err("scheme=manual must be FK-rejected");
        assert_eq!(
            err_code(&err).as_deref(),
            Some("23503"),
            "foreign_key_violation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn external_id_check_rejects_unsafe_values(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "extid-unsafe").await;
        for bad in ["has/slash", "has?query", "has space", "café", ""] {
            let res = upsert_manifestation_identifier(&pool, m, "asin", bad, None).await;
            assert!(
                res.is_err(),
                "external_id {bad:?} must violate the charset CHECK"
            );
        }
        // A well-formed ASIN is accepted.
        upsert_manifestation_identifier(&pool, m, "asin", "B004GXAX8C", None)
            .await
            .expect("valid ASIN accepted");
    }

    // ---- cascade on entity deletion ----

    #[sqlx::test(migrations = "./migrations")]
    async fn deleting_manifestation_cascades_its_identifiers(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "midd-cascade").await;
        upsert_manifestation_identifier(&pool, m, "googlebooks", "vol123", None)
            .await
            .expect("upsert");

        sqlx::query!("DELETE FROM manifestations WHERE id = $1", m)
            .execute(&pool)
            .await
            .expect("delete manifestation");

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1",
            m,
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(remaining, 0, "manifestation identifiers cascade-deleted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn deleting_work_cascades_its_identifiers(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (w, m) = insert_work_and_manifestation(&ingestion, "widd-cascade").await;
        upsert_work_identifier(&pool, w, "openlibrary", "OL999W", None)
            .await
            .expect("upsert");
        // Remove the manifestation first (its FK to the work is also cascade,
        // but delete it explicitly so the work delete is unambiguous).
        sqlx::query!("DELETE FROM manifestations WHERE id = $1", m)
            .execute(&pool)
            .await
            .expect("delete manifestation");
        sqlx::query!("DELETE FROM works WHERE id = $1", w)
            .execute(&pool)
            .await
            .expect("delete work");

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM work_external_identifiers WHERE work_id = $1",
            w,
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(remaining, 0, "work identifiers cascade-deleted");
    }

    // ---- RLS: reads ----

    #[sqlx::test(migrations = "./migrations")]
    async fn child_reads_only_shelf_visible_manifestation_identifiers(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "child-vis").await;
        let (_hw, hidden_m) = insert_work_and_manifestation(&ingestion, "child-hid").await;
        upsert_manifestation_identifier(&pool, visible_m, "googlebooks", "visID", None)
            .await
            .expect("seed visible");
        upsert_manifestation_identifier(&pool, hidden_m, "googlebooks", "hidID", None)
            .await
            .expect("seed hidden");

        let (child_id, _) = create_child_user_and_basic_auth(&app, "child-read").await;
        let shelf = create_shelf(&app, child_id, "kids").await;
        add_to_shelf(&app, shelf, visible_m).await;

        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let rows: Vec<Uuid> = sqlx::query_scalar!(
            "SELECT manifestation_id FROM manifestation_external_identifiers \
             ORDER BY manifestation_id",
        )
        .fetch_all(&mut *tx)
        .await
        .expect("child select");
        assert_eq!(
            rows,
            vec![visible_m],
            "child sees only the shelf-visible manifestation's identifier"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn vis_work_child_cannot_read_other_work_identifier(pool: PgPool) {
        // Regression guard for the work-scope predicate: the child MUST have a
        // visible manifestation, otherwise a shadow-broken visibility predicate
        // (outer work_id bound to the inner manifestations.work_id) would also
        // return empty and mask the bug. The separate work is non-visible but
        // carries a work-level identifier; the child must read zero rows for it.
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "vw-vis").await;
        let (other_w, _om) = insert_work_and_manifestation(&ingestion, "vw-other").await;
        upsert_work_identifier(&pool, other_w, "openlibrary", "OL777W", None)
            .await
            .expect("seed other-work identifier");

        let (child_id, _) = create_child_user_and_basic_auth(&app, "vw-child").await;
        let shelf = create_shelf(&app, child_id, "kids").await;
        add_to_shelf(&app, shelf, visible_m).await;

        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM work_external_identifiers WHERE work_id = $1",
            other_w,
        )
        .fetch_one(&mut *tx)
        .await
        .expect("child select");
        assert_eq!(
            count, 0,
            "child must not read a non-visible work's identifier"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rls_enabled_fresh_child_sees_no_identifier_rows(pool: PgPool) {
        // Asserts ENABLE is in force: a child with no shelf sees zero rows even
        // though the row exists and grants permit SELECT.
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "enable-check").await;
        upsert_manifestation_identifier(&pool, m, "googlebooks", "hiddenX", None)
            .await
            .expect("seed");

        let (child_id, _) = create_child_user_and_basic_auth(&app, "enable-child").await;
        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM manifestation_external_identifiers",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("select");
        assert_eq!(
            count, 0,
            "RLS ENABLE hides all rows from a shelf-less child"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn adult_reads_all_identifiers(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "adult-read").await;
        upsert_manifestation_identifier(&pool, m, "googlebooks", "adultsee", None)
            .await
            .expect("seed");
        let (adult_id, _) = create_adult_and_basic_auth(&app, "adult-read").await;
        let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
        let count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM manifestation_external_identifiers WHERE manifestation_id = $1",
            m,
        )
        .fetch_one(&mut *tx)
        .await
        .expect("select");
        assert_eq!(count, 1, "adult sees all identifiers");
    }

    // ---- RLS: writes ----

    #[sqlx::test(migrations = "./migrations")]
    async fn adult_can_insert_identifier(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "adult-write").await;
        let (adult_id, _) = create_adult_and_basic_auth(&app, "adult-write").await;
        let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
        upsert_manifestation_identifier(&mut *tx, m, "googlebooks", "adultwrote", None)
            .await
            .expect("adult insert succeeds");
        tx.commit().await.expect("commit");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn child_cannot_insert_identifier(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "child-write").await;
        let (child_id, _) = create_child_user_and_basic_auth(&app, "child-write").await;
        // Even a shelf-visible manifestation: the child is not admin/adult, so
        // no write policy matches → default-deny.
        let shelf = create_shelf(&app, child_id, "kids").await;
        add_to_shelf(&app, shelf, m).await;
        let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
        let err = upsert_manifestation_identifier(&mut *tx, m, "googlebooks", "childtry", None)
            .await
            .expect_err("child insert denied");
        assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn readonly_cannot_insert_identifier(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let readonly = readonly_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "ro-write").await;
        let (adult_id, _) = create_adult_and_basic_auth(&app, "ro-write").await;
        let mut tx = acquire_with_rls(&readonly, adult_id).await.expect("rls tx");
        let err = upsert_manifestation_identifier(&mut *tx, m, "googlebooks", "rotry", None)
            .await
            .expect_err("readonly insert denied");
        // readonly holds no INSERT grant → 42501 insufficient_privilege.
        assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ingestion_can_upsert_identifier(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "ing-write").await;
        // ingestion has USING(true) WITH CHECK(true) and needs no GUC.
        upsert_manifestation_identifier(&ingestion, m, "googlebooks", "ingwrote", None)
            .await
            .expect("ingestion upsert succeeds");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unset_guc_write_is_denied_not_errored(pool: PgPool) {
        // A reverie_app write with no app.current_user_id set must be a clean
        // policy miss (42501), NOT a query error from casting an unset GUC. We
        // deliberately do NOT call acquire_with_rls, so the GUC is unset on
        // this connection.
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w, m) = insert_work_and_manifestation(&ingestion, "unset-guc").await;
        let err = upsert_manifestation_identifier(&app, m, "googlebooks", "noguctry", None)
            .await
            .expect_err("insert with unset GUC denied");
        assert!(
            is_rls_denied(&err),
            "unset GUC must fail closed (42501), not raise a cast error; got {err:?}"
        );
    }
}
