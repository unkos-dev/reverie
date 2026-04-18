//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::work;
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/manifestations/{id}/metadata",
            get(get_manifestation_metadata),
        )
        .route("/api/works/{id}/metadata", get(get_work_metadata))
        .route(
            "/api/manifestations/{id}/metadata/accept",
            post(accept_manifestation),
        )
        .route(
            "/api/manifestations/{id}/metadata/reject",
            post(reject_manifestation),
        )
        .route(
            "/api/manifestations/{id}/metadata/revert",
            post(revert_manifestation),
        )
        .route("/api/manifestations/{id}/metadata/lock", post(lock_field))
        .route(
            "/api/manifestations/{id}/metadata/unlock",
            post(unlock_field),
        )
}

#[derive(Debug, Serialize)]
struct MetadataRow {
    id: Uuid,
    field_name: String,
    source: String,
    new_value: Value,
    status: String,
    confidence_score: f32,
    match_type: String,
    observation_count: i32,
}

type MetadataRowRaw = (Uuid, String, String, Value, String, f32, String, i32);

fn raw_to_row(raw: MetadataRowRaw) -> MetadataRow {
    let (
        id,
        field_name,
        source,
        new_value,
        status,
        confidence_score,
        match_type,
        observation_count,
    ) = raw;
    MetadataRow {
        id,
        field_name,
        source,
        new_value,
        status,
        confidence_score,
        match_type,
        observation_count,
    }
}

async fn get_manifestation_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let rows = load_versions(&state.pool, id).await?;
    Ok(axum::Json(rows))
}

async fn get_work_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(work_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows: Vec<MetadataRowRaw> = sqlx::query_as(
        "SELECT mv.id, mv.field_name, mv.source, mv.new_value, mv.status::text, \
                mv.confidence_score, mv.match_type, mv.observation_count \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         WHERE m.work_id = $1 \
         ORDER BY mv.last_seen_at DESC",
    )
    .bind(work_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let rows: Vec<MetadataRow> = rows.into_iter().map(raw_to_row).collect();
    Ok(axum::Json(rows))
}

async fn load_versions(
    pool: &sqlx::PgPool,
    manifestation_id: Uuid,
) -> Result<Vec<MetadataRow>, AppError> {
    let rows: Vec<MetadataRowRaw> = sqlx::query_as(
        "SELECT id, field_name, source, new_value, status::text, \
                confidence_score, match_type, observation_count \
         FROM metadata_versions \
         WHERE manifestation_id = $1 \
         ORDER BY last_seen_at DESC",
    )
    .bind(manifestation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(rows.into_iter().map(raw_to_row).collect())
}

// ── accept / reject / revert / lock ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct VersionPayload {
    version_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct RevertPayload {
    field_name: String,
    /// `null` clears the canonical pointer AND the canonical column.
    version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct LockPayload {
    field_name: String,
    entity_type: String,
}

async fn accept_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row: Option<(Uuid, String, Value, Uuid)> = sqlx::query_as(
        "SELECT mv.id, mv.field_name, mv.new_value, m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         JOIN works w ON w.id = m.work_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
         FOR UPDATE OF m, w",
    )
    .bind(payload.version_id)
    .bind(manifestation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (version_id, field_name, new_value, work_id) = row.ok_or(AppError::NotFound)?;

    apply_version(
        &mut tx,
        &field_name,
        &new_value,
        version_id,
        manifestation_id,
        work_id,
    )
    .await?;

    // Accepted ISBN changes can trigger auto-merge; match orchestrator behaviour.
    if field_name == "isbn_10" || field_name == "isbn_13" {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

async fn reject_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows = sqlx::query(
        "UPDATE metadata_versions \
         SET status = 'rejected', resolved_by = $1, resolved_at = now() \
         WHERE id = $2 AND manifestation_id = $3",
    )
    .bind(current_user.user_id)
    .bind(payload.version_id)
    .bind(manifestation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

async fn revert_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<RevertPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock both the manifestation row and its work row so concurrent
    // accept/revert calls on sibling manifestations of the same work
    // serialise on `works.{title,description,language}` updates.
    let work_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
    )
    .bind(manifestation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let work_id = work_id.ok_or(AppError::NotFound)?;

    match payload.version_id {
        Some(vid) => {
            let new_value: Option<Value> = sqlx::query_scalar(
                "SELECT new_value FROM metadata_versions \
                 WHERE id = $1 AND manifestation_id = $2",
            )
            .bind(vid)
            .bind(manifestation_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            let new_value = new_value.ok_or(AppError::NotFound)?;
            apply_version(
                &mut tx,
                &payload.field_name,
                &new_value,
                vid,
                manifestation_id,
                work_id,
            )
            .await?;
        }
        None => {
            clear_field(&mut tx, &payload.field_name, manifestation_id, work_id).await?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

async fn lock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    field_lock::lock(
        &state.pool,
        manifestation_id,
        entity,
        &payload.field_name,
        current_user.user_id,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::CREATED)
}

async fn unlock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let removed = field_lock::unlock(&state.pool, manifestation_id, entity, &payload.field_name)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !removed {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::OK)
}

fn parse_entity(s: &str) -> Result<EntityType, AppError> {
    match s {
        "work" => Ok(EntityType::Work),
        "manifestation" => Ok(EntityType::Manifestation),
        other => Err(AppError::Validation(format!(
            "invalid entity_type '{other}'; expected 'work' or 'manifestation'"
        ))),
    }
}

/// Apply a specific version to its canonical column + pointer.
/// Reused by `/accept` and `/revert`.
async fn apply_version(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    value: &Value,
    version_id: Uuid,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    let str_val = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    match field {
        "title" => {
            exec(
                tx,
                "UPDATE works SET title = $1, sort_title = lower($1), title_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                work_id,
            )
            .await?
        }
        "description" => {
            exec(
                tx,
                "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                work_id,
            )
            .await?
        }
        "language" => {
            exec(
                tx,
                "UPDATE works SET language = $1, language_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                work_id,
            )
            .await?
        }
        "publisher" => {
            exec(
                tx,
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                manifestation_id,
            )
            .await?
        }
        "isbn_10" => {
            exec(
                tx,
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                manifestation_id,
            )
            .await?
        }
        "isbn_13" => {
            exec(
                tx,
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
                &str_val,
                version_id,
                manifestation_id,
            )
            .await?
        }
        "pub_date" => {
            let date = parse_iso_date(&str_val).map_err(|e| {
                AppError::Validation(format!("invalid pub_date: {e}"))
            })?;
            sqlx::query(
                "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
            )
            .bind(date)
            .bind(version_id)
            .bind(manifestation_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auto-apply field '{other}' (list/complex fields must be accepted via their dedicated routes)"
            )));
        }
    }
    Ok(())
}

async fn clear_field(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    let sql = match field {
        "title" => {
            return Err(AppError::Validation(
                "cannot clear title — revert to a specific version instead".into(),
            ));
        }
        "description" => Some(("works", "description", "description_version_id", work_id)),
        "language" => Some(("works", "language", "language_version_id", work_id)),
        "publisher" => Some((
            "manifestations",
            "publisher",
            "publisher_version_id",
            manifestation_id,
        )),
        "pub_date" => Some((
            "manifestations",
            "pub_date",
            "pub_date_version_id",
            manifestation_id,
        )),
        "isbn_10" => Some((
            "manifestations",
            "isbn_10",
            "isbn_10_version_id",
            manifestation_id,
        )),
        "isbn_13" => Some((
            "manifestations",
            "isbn_13",
            "isbn_13_version_id",
            manifestation_id,
        )),
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    };
    let Some((table, col, vid_col, row_id)) = sql else {
        return Ok(());
    };
    let q = format!("UPDATE {table} SET {col} = NULL, {vid_col} = NULL WHERE id = $1");
    sqlx::query(&q)
        .bind(row_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn exec(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    value: &str,
    version_id: Uuid,
    row_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(sql)
        .bind(value)
        .bind(version_id)
        .bind(row_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

fn parse_iso_date(s: &str) -> Result<time::Date, time::error::Parse> {
    use time::format_description::well_known::Iso8601;
    // `s.len()` is in bytes; user-submitted strings can contain multi-byte
    // UTF-8 codepoints. `is_char_boundary` keeps the slice valid.
    if s.len() >= 10 && s.is_char_boundary(10) {
        time::Date::parse(&s[..10], &Iso8601::DATE)
    } else {
        let padded = match s.len() {
            4 => format!("{s}-01-01"),
            7 => format!("{s}-01"),
            _ => s.to_string(),
        };
        time::Date::parse(&padded, &Iso8601::DATE)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_iso_date;
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

    #[test]
    fn parse_iso_date_rejects_multibyte_garbage_without_panicking() {
        // 3-byte codepoint pushes byte-10 mid-character; pre-fix this panicked.
        let s = "2024-01-€€€garbage";
        assert!(parse_iso_date(s).is_err());
    }

    #[test]
    fn parse_iso_date_accepts_well_formed_iso() {
        assert!(parse_iso_date("2024-01-15").is_ok());
        assert!(parse_iso_date("2024-01-15T00:00:00Z").is_ok());
    }

    #[tokio::test]
    async fn get_manifestation_metadata_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .get(&format!("/api/manifestations/{id}/metadata"))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn accept_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let vid = Uuid::new_v4();
        let response = server
            .post(&format!("/api/manifestations/{id}/metadata/accept"))
            .json(&serde_json::json!({"version_id": vid}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    // ── Admin-authenticated success tests (X3) ────────────────────────────
    //
    // These exercise the C2 fix: route handlers now open their tx via
    // `acquire_with_rls`, so the manifestations RLS policies see a real
    // `app.current_user_id` and an admin user satisfies the
    // `role IN ('admin','adult')` clause.  Without C2 these tests would
    // 404 on the initial SELECT.

    use axum::http::header::AUTHORIZATION;

    /// Insert a `metadata_versions` row for `(manifestation_id, field_name)`
    /// via the ingestion pool, returning its id.
    async fn insert_version(
        ingestion_pool: &sqlx::PgPool,
        manifestation_id: Uuid,
        field: &str,
        value: serde_json::Value,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, \
                 match_type, confidence_score, status) \
             VALUES ($1, 'openlibrary', $2, $3, $4, 'isbn', 0.96, 'pending'::metadata_review_status) \
             RETURNING id",
        )
        .bind(manifestation_id)
        .bind(field)
        .bind(&value)
        .bind(format!("hash-{}", Uuid::new_v4()).into_bytes())
        .fetch_one(ingestion_pool)
        .await
        .expect("insert metadata_versions")
    }

    #[tokio::test]
    #[ignore] // requires running postgres + applied migrations
    async fn accept_admin_writes_canonical_title() {
        let app_pool = test_support::db::app_pool().await;
        let ing_pool = test_support::db::ingestion_pool().await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let new_title = format!("Canon Title {marker}");
        let version_id =
            insert_version(&ing_pool, m_id, "title", serde_json::json!(new_title)).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let title: String = sqlx::query_scalar("SELECT title FROM works WHERE id = $1")
            .bind(work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch title");
        assert_eq!(title, new_title, "canonical title not written");

        let pointer: Option<Uuid> =
            sqlx::query_scalar("SELECT title_version_id FROM works WHERE id = $1")
                .bind(work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title_version_id");
        assert_eq!(pointer, Some(version_id), "version pointer not wired");

        test_support::db::cleanup_work(&ing_pool, work_id).await;
        test_support::db::cleanup_user(&app_pool, admin_id).await;
    }

    #[tokio::test]
    #[ignore]
    async fn reject_admin_marks_version_rejected() {
        let app_pool = test_support::db::app_pool().await;
        let ing_pool = test_support::db::ingestion_pool().await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("Reject Me {marker}")),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/manifestations/{m_id}/metadata/reject"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let row: (String, Option<Uuid>) =
            sqlx::query_as("SELECT status::text, resolved_by FROM metadata_versions WHERE id = $1")
                .bind(version_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch version");
        assert_eq!(row.0, "rejected");
        assert_eq!(row.1, Some(admin_id), "resolved_by should record admin id");

        test_support::db::cleanup_work(&ing_pool, work_id).await;
        test_support::db::cleanup_user(&app_pool, admin_id).await;
    }

    #[tokio::test]
    #[ignore]
    async fn revert_admin_clears_field_to_null() {
        let app_pool = test_support::db::app_pool().await;
        let ing_pool = test_support::db::ingestion_pool().await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Pre-set a description on the work + a version pointer; revert with
        // version_id=null should clear both.
        let initial = format!("To Be Cleared {marker}");
        let version_id =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query("UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3")
            .bind(&initial)
            .bind(version_id)
            .bind(work_id)
            .execute(&ing_pool)
            .await
            .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "description",
                "version_id": serde_json::Value::Null,
            }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let row: (Option<String>, Option<Uuid>) =
            sqlx::query_as("SELECT description, description_version_id FROM works WHERE id = $1")
                .bind(work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch work");
        assert_eq!(row.0, None, "description should be cleared");
        assert_eq!(row.1, None, "version pointer should be cleared");

        test_support::db::cleanup_work(&ing_pool, work_id).await;
        test_support::db::cleanup_user(&app_pool, admin_id).await;
    }
}
