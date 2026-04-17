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

    let rows: Vec<MetadataRowRaw> = sqlx::query_as(
        "SELECT mv.id, mv.field_name, mv.source, mv.new_value, mv.status::text, \
                mv.confidence_score, mv.match_type, mv.observation_count \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         WHERE m.work_id = $1 \
         ORDER BY mv.last_seen_at DESC",
    )
    .bind(work_id)
    .fetch_all(&state.pool)
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

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row: Option<(Uuid, String, Value, Uuid)> = sqlx::query_as(
        "SELECT mv.id, mv.field_name, mv.new_value, m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
         FOR UPDATE OF m",
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

    let rows = sqlx::query(
        "UPDATE metadata_versions \
         SET status = 'rejected', resolved_by = $1, resolved_at = now() \
         WHERE id = $2 AND manifestation_id = $3",
    )
    .bind(current_user.user_id)
    .bind(payload.version_id)
    .bind(manifestation_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::OK)
}

async fn revert_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<RevertPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Load work_id FOR UPDATE so we serialise concurrent revert calls.
    let work_id: Option<Uuid> =
        sqlx::query_scalar("SELECT work_id FROM manifestations WHERE id = $1 FOR UPDATE")
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
    if s.len() >= 10 {
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
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

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
}
