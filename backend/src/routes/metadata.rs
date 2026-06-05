//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::work;
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::value_hash;
use crate::services::metadata::isbn;
use crate::state::AppState;

/// Build the metadata-review router.
///
/// Mounts the `GET` views over manifestation/work metadata and the
/// `POST` accept/reject/revert/lock/unlock mutators on `AppState`.
///
/// # Invariants
/// - Every handler requires an authenticated non-child user
///   (`CurrentUser::require_not_child`).
/// - Reads and writes acquire a connection via `db::acquire_with_rls`
///   so RLS policies see `app.current_user_id` for the caller.
/// - Write paths open a transaction, take `SELECT ... FOR UPDATE` on
///   the owning manifestation/work row, apply the change, and commit.
///
/// Why: the row-level lock serialises concurrent reviewers against the
/// same entity so accept/reject/revert can't race with each other or
/// with re-enrichment writes that mutate the same metadata row.
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
        .route("/api/books/{id}/metadata", patch(update_book_metadata))
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

async fn get_manifestation_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = load_versions(&mut tx, id).await?;
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

    let rows = sqlx::query!(
        "SELECT mv.id, mv.field_name, mv.source, \
                mv.new_value AS \"new_value!\", \
                mv.status::text AS \"status!\", \
                mv.confidence_score AS \"confidence_score!\", \
                mv.match_type, mv.observation_count \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         WHERE m.work_id = $1 \
         ORDER BY mv.last_seen_at DESC",
        work_id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let rows: Vec<MetadataRow> = rows
        .into_iter()
        .map(|r| MetadataRow {
            id: r.id,
            field_name: r.field_name,
            source: r.source,
            new_value: r.new_value,
            status: r.status,
            confidence_score: r.confidence_score,
            match_type: r.match_type,
            observation_count: r.observation_count,
        })
        .collect();
    Ok(axum::Json(rows))
}

async fn load_versions(
    tx: &mut sqlx::PgConnection,
    manifestation_id: Uuid,
) -> Result<Vec<MetadataRow>, AppError> {
    let rows = sqlx::query!(
        "SELECT id, field_name, source, \
                new_value AS \"new_value!\", \
                status::text AS \"status!\", \
                confidence_score AS \"confidence_score!\", \
                match_type, observation_count \
         FROM metadata_versions \
         WHERE manifestation_id = $1 \
         ORDER BY last_seen_at DESC",
        manifestation_id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(rows
        .into_iter()
        .map(|r| MetadataRow {
            id: r.id,
            field_name: r.field_name,
            source: r.source,
            new_value: r.new_value,
            status: r.status,
            confidence_score: r.confidence_score,
            match_type: r.match_type,
            observation_count: r.observation_count,
        })
        .collect())
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

    let row = sqlx::query!(
        "SELECT mv.id, mv.field_name, \
                mv.new_value AS \"new_value!\", \
                m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         JOIN works w ON w.id = m.work_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
         FOR UPDATE OF m, w",
        payload.version_id,
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.ok_or(AppError::NotFound)?;
    let version_id = row.id;
    let field_name = row.field_name;
    let new_value = row.new_value;
    let work_id = row.work_id;

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

    let rows = sqlx::query!(
        "UPDATE metadata_versions \
         SET status = 'rejected', resolved_by = $1, resolved_at = now() \
         WHERE id = $2 AND manifestation_id = $3",
        current_user.user_id,
        payload.version_id,
        manifestation_id,
    )
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
    let work_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let work_id = work_id.ok_or(AppError::NotFound)?;

    match payload.version_id {
        Some(vid) => {
            let new_value: Option<Value> = sqlx::query_scalar!(
                "SELECT new_value AS \"new_value!\" FROM metadata_versions \
                 WHERE id = $1 AND manifestation_id = $2",
                vid,
                manifestation_id,
            )
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
//
// Field-dispatch with one `sqlx::query!` per supported field; macro-form
// expansion pushed this just past clippy's 100-line threshold.
#[allow(clippy::too_many_lines)]
async fn apply_version(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    value: &Value,
    version_id: Uuid,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    // Refuse to promote a null-valued audit row to canonical. Manual
    // clears (PATCH `{field}: null`) write a `'null'::jsonb` audit row
    // with `status = 'pending'` so operators can revert to them; the
    // Versions tab filters those rows out, but `accept_manifestation`
    // and `revert_manifestation` accept any pending row by id. Without
    // this guard, `value.as_str()` returns `None`, `value.to_string()`
    // yields the literal `"null"`, and non-date canonical columns get
    // corrupted with the string `"null"`. Callers wanting to clear a
    // field must go through `clear_field` (PATCH null or revert with
    // `version_id = null`), not /accept.
    if value.is_null() {
        return Err(AppError::Validation(
            "cannot accept a null-value audit row; use revert/clear instead".into(),
        ));
    }
    let str_val = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    match field {
        "title" => {
            sqlx::query!(
                "UPDATE works \
                 SET title = $1, sort_title = lower($1), title_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                work_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "description" => {
            sqlx::query!(
                "UPDATE works SET description = $1, description_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                work_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "language" => {
            sqlx::query!(
                "UPDATE works SET language = $1, language_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                work_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "publisher" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET publisher = $1, publisher_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "isbn_10" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET isbn_10 = $1, isbn_10_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "isbn_13" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET isbn_13 = $1, isbn_13_version_id = $2 \
                 WHERE id = $3",
                str_val,
                version_id,
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "pub_date" => {
            let date = parse_iso_date(&str_val)
                .map_err(|e| AppError::Validation(format!("invalid pub_date: {e}")))?;
            sqlx::query!(
                "UPDATE manifestations \
                 SET pub_date = $1, pub_date_version_id = $2 \
                 WHERE id = $3",
                date,
                version_id,
                manifestation_id,
            )
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
    enqueue_writeback(tx, manifestation_id, field).await?;
    Ok(())
}

/// Insert a writeback job in the caller's tx.  Shared by `apply_version`
/// (accept + revert-to-version) and `clear_field` (revert-to-null) so the
/// pointer mutation and writeback enqueue commit together.
async fn enqueue_writeback(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: &str,
) -> Result<(), AppError> {
    let reason = if field == "cover" || field == "cover_url" {
        "cover"
    } else {
        "metadata"
    };
    sqlx::query!(
        "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2)",
        manifestation_id,
        reason,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn clear_field(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    match field {
        "title" => {
            return Err(AppError::Validation(
                "cannot clear title — revert to a specific version instead".into(),
            ));
        }
        "description" => {
            sqlx::query!(
                "UPDATE works SET description = NULL, description_version_id = NULL \
                 WHERE id = $1",
                work_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "language" => {
            sqlx::query!(
                "UPDATE works SET language = NULL, language_version_id = NULL \
                 WHERE id = $1",
                work_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "publisher" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET publisher = NULL, publisher_version_id = NULL \
                 WHERE id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "pub_date" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET pub_date = NULL, pub_date_version_id = NULL \
                 WHERE id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "isbn_10" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET isbn_10 = NULL, isbn_10_version_id = NULL \
                 WHERE id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        "isbn_13" => {
            sqlx::query!(
                "UPDATE manifestations \
                 SET isbn_13 = NULL, isbn_13_version_id = NULL \
                 WHERE id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    }
    enqueue_writeback(tx, manifestation_id, field).await?;
    Ok(())
}

// ── manual metadata edit (RFC 7396 JSON Merge Patch) ──────────────────────

/// Per-field sparse update body for `PATCH /api/books/{id}/metadata`.
///
/// Encodes RFC 7396 JSON Merge Patch semantics via
/// `serde_with::rust::double_option` (per
/// `adr/2026-05-22-backend-aux-crates.md`):
/// * absent key → `None`         → leave field unchanged.
/// * key = `null` → `Some(None)` → clear the canonical value.
/// * key = value → `Some(Some(v))` → set the canonical value.
///
/// `cover` is not yet operator-editable from this endpoint — cover
/// promotion happens via a separate file-upload path.
#[allow(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding — outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Default, Deserialize)]
struct UpdateMetadataFields {
    #[serde(default, with = "::serde_with::rust::double_option")]
    title: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    description: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    language: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    publisher: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub_date: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    isbn_10: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    isbn_13: Option<Option<String>>,
}

/// Outer envelope for `PATCH /api/books/{id}/metadata`. The `fields`
/// wrapper keeps the door open for future top-level keys (eg. `tags`,
/// `series_position`) without forcing a body-shape migration.
#[derive(Debug, Deserialize)]
struct UpdateMetadataRequest {
    fields: UpdateMetadataFields,
}

impl UpdateMetadataFields {
    /// Collect every populated entry as `(field_name, optional value)`
    /// — `None` means "clear this field", `Some(_)` means "set to this".
    ///
    /// `None` for the outer Option means the key was absent from the
    /// JSON body and is therefore not iterated; that case never appears
    /// in the returned vec.
    #[allow(
        clippy::option_option,
        reason = "see UpdateMetadataFields struct attribute"
    )]
    fn populated(self) -> Vec<(&'static str, Option<String>)> {
        let mut out: Vec<(&'static str, Option<String>)> = Vec::new();
        if let Some(v) = self.title {
            out.push(("title", v));
        }
        if let Some(v) = self.description {
            out.push(("description", v));
        }
        if let Some(v) = self.language {
            out.push(("language", v));
        }
        if let Some(v) = self.publisher {
            out.push(("publisher", v));
        }
        if let Some(v) = self.pub_date {
            out.push(("pub_date", v));
        }
        if let Some(v) = self.isbn_10 {
            out.push(("isbn_10", v));
        }
        if let Some(v) = self.isbn_13 {
            out.push(("isbn_13", v));
        }
        out
    }
}

/// `PATCH /api/books/{id}/metadata` — manual operator edit.
///
/// RFC 7396 JSON Merge Patch shape. For each touched field the handler
/// inserts a `metadata_versions` row with `source = 'manual'` and then
/// either promotes that row to canonical via [`apply_version`] (when a
/// value is supplied) or clears the canonical column via [`clear_field`]
/// (when the value is `null`). The pending AI/OPF drafts on the same
/// field stay `status = 'pending'` — the operator can revert to one of
/// them later.
///
/// # Errors
/// - [`AppError::Validation`] when the body has no populated fields,
///   when an ISBN/date value fails parsing, or when the operator tries
///   to clear `title` (canonical title is `NOT NULL` on `works`).
/// - [`AppError::NotFound`] when the manifestation is missing or hidden
///   by RLS for the current user (existence-not-leaked).
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Internal`] on database errors.
#[allow(clippy::too_many_lines)]
async fn update_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    body: Result<axum::Json<UpdateMetadataRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    let fields = req.fields.populated();
    if fields.is_empty() {
        return Err(AppError::Validation("no fields".into()));
    }

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock the manifestation + parent work for the duration of the tx.
    // Mirrors `revert_manifestation` — serialises concurrent edits and
    // gates RLS-hidden rows to 404 (the join through `manifestations`
    // applies the RLS predicate).
    let work_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let work_id = work_id.ok_or(AppError::NotFound)?;

    let mut touched_isbn = false;
    for (field, maybe_value) in fields {
        // ISBN edits — set OR clear — change the matching surface.
        // Rematch is read-only on rejection so it's safe to gate on
        // the column name alone without inspecting the new value.
        if field == "isbn_10" || field == "isbn_13" {
            touched_isbn = true;
        }
        if let Some(value) = maybe_value {
            // Reject malformed ISBNs (wrong length, bad check digit,
            // non-numeric) and normalise valid ones to digits-only so the
            // stored value matches the ingestion surface and rematch's
            // exact-equality join can find ingested twins. Guard lives here,
            // not in `apply_version`, so accept/revert paths keep accepting
            // historical pre-validation/pre-normalisation values.
            let value = match field {
                "isbn_10" => isbn::checked_isbn10(&value)
                    .ok_or_else(|| AppError::Validation("invalid isbn_10".into()))?,
                "isbn_13" => isbn::checked_isbn13(&value)
                    .ok_or_else(|| AppError::Validation("invalid isbn_13".into()))?,
                _ => value,
            };
            let json = serde_json::Value::String(value);
            let version_id = insert_manual_version(
                &mut tx,
                manifestation_id,
                current_user.user_id,
                field,
                &json,
            )
            .await?;
            apply_version(&mut tx, field, &json, version_id, manifestation_id, work_id).await?;
        } else {
            // Audit-trail row: source='manual', new_value=null,
            // resolved_by = caller. `clear_field` does NOT wire a
            // canonical pointer to this row — by design — so it lives
            // in the journal as an orphan record of the clear action,
            // but `resolved_by` makes the action accountable.
            insert_manual_version(
                &mut tx,
                manifestation_id,
                current_user.user_id,
                field,
                &serde_json::Value::Null,
            )
            .await?;
            clear_field(&mut tx, field, manifestation_id, work_id).await?;
        }
    }

    if touched_isbn {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Insert one `metadata_versions` row with `source='manual'`, returning
/// its id. `value_hash` is the canonical SHA-256 of the serialised
/// value — same shape as the enrichment pipeline emits, so duplicate
/// manual saves of the same value collide on the existing
/// `(manifestation, source, field, value_hash)` unique. We surface
/// that collision as a no-op by bumping `last_seen_at` /
/// `observation_count` on the existing row and re-recording who
/// touched it last.
///
/// `resolved_by` + `resolved_at` here denote the *author* of the
/// manual entry, not a resolution action — the row stays
/// `status = 'pending'` after insert so the accept/reject machinery
/// can still operate on it. The accountability use of these columns
/// is project-local and documented in the manual-edit ADR.
async fn insert_manual_version(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: &str,
    value: &Value,
) -> Result<Uuid, AppError> {
    // Hash via the shared canonical normaliser (same call the enrichment
    // pipeline uses) so whitespace-equivalent values — e.g. a publisher
    // with stray leading/trailing space — produce an identical `value_hash`
    // regardless of entry path, restoring journal dedup.
    let hash = value_hash::value_hash(field, value);

    // `status` is reset to `pending` on conflict so a previously-
    // rejected manual entry of the same value is re-offered for
    // promotion rather than left stranded as `rejected`. Without this
    // reset, `apply_version` could write a canonical pointer to a row
    // still flagged rejected, leaving the journal internally
    // inconsistent.
    let id: Uuid = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, \
             match_type, confidence_score, status, resolved_by, resolved_at) \
         VALUES ($1, 'manual', $2, $3, $4, 'manual', 1.0, \
                 'pending'::metadata_review_status, $5, now()) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1, \
                       status = 'pending'::metadata_review_status, \
                       resolved_by = $5, \
                       resolved_at = now() \
         RETURNING id",
        manifestation_id,
        field,
        value,
        hash,
        user_id,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(id)
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
        let hash = format!("hash-{}", Uuid::new_v4()).into_bytes();
        sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, \
                 match_type, confidence_score, status) \
             VALUES ($1, 'openlibrary', $2, $3, $4, 'isbn', 0.96, 'pending'::metadata_review_status) \
             RETURNING id",
            manifestation_id,
            field,
            value,
            hash,
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert metadata_versions")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_admin_writes_canonical_title(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
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

        let title: String = sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch title");
        assert_eq!(title, new_title, "canonical title not written");

        let pointer: Option<Uuid> =
            sqlx::query_scalar!("SELECT title_version_id FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title_version_id");
        assert_eq!(pointer, Some(version_id), "version pointer not wired");

        // Accept must have enqueued exactly one writeback_jobs row.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 1,
            "accept must enqueue exactly one writeback job; got {job_count}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reject_admin_marks_version_rejected(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
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

        let row = sqlx::query!(
            "SELECT status::text AS status, resolved_by FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch version");
        assert_eq!(row.status.as_deref(), Some("rejected"));
        assert_eq!(
            row.resolved_by,
            Some(admin_id),
            "resolved_by should record admin id"
        );

        // Reject does NOT change the canonical pointer, so it must NOT
        // enqueue a writeback job.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 0,
            "reject must NOT enqueue writeback; got {job_count}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_admin_clears_field_to_null(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Pre-set a description on the work + a version pointer; revert with
        // version_id=null should clear both.
        let initial = format!("To Be Cleared {marker}");
        let version_id =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            initial,
            version_id,
            work_id,
        )
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

        let row = sqlx::query!(
            "SELECT description, description_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert_eq!(row.description, None, "description should be cleared");
        assert_eq!(
            row.description_version_id, None,
            "version pointer should be cleared"
        );

        // Revert must enqueue exactly one writeback_jobs row — the OPF
        // still needs the field cleared on disk.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 1,
            "revert must enqueue exactly one writeback job; got {job_count}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn double_accept_enqueues_two_jobs(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let v1 = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("First {marker}")),
        )
        .await;
        let v2 = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("Second {marker}")),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        for vid in [v1, v2] {
            let response = server
                .post(&format!("/api/manifestations/{m_id}/metadata/accept"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"version_id": vid}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }

        // Emitter does NOT dedup; worker does.  Two accepts → two rows.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(job_count, 2, "two accepts must enqueue two rows (no dedup)");
    }

    // ── PATCH /api/books/{id}/metadata (manual edit, RFC 7396) ───────────

    #[tokio::test]
    async fn patch_book_metadata_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .patch(&format!("/api/books/{id}/metadata"))
            .json(&serde_json::json!({"fields": {"title": "X"}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_sets_title_and_writes_canonical(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let new_title = format!("Manual Title {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": new_title}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        let row = sqlx::query!(
            "SELECT title, title_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert_eq!(row.title, new_title, "canonical title not written");

        let version_id = row.title_version_id.expect("title_version_id wired");
        let v = sqlx::query!(
            "SELECT source, status::text AS \"status!\", new_value AS \"new_value!\" \
             FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch version");
        assert_eq!(v.source, "manual");
        assert_eq!(v.status, "pending");
        assert_eq!(v.new_value, serde_json::json!(new_title));

        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(job_count, 1, "manual edit must enqueue one writeback");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_clears_description_with_null(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let initial = format!("Existing prose {marker}");
        let seed_v =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            initial,
            seed_v,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"description": serde_json::Value::Null}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        let row = sqlx::query!(
            "SELECT description, description_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert!(row.description.is_none(), "description should be cleared");
        assert!(
            row.description_version_id.is_none(),
            "version pointer should be cleared"
        );

        // Audit-trail row recorded with new_value = JSON null.
        let null_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" \
             FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' \
               AND field_name = 'description' AND new_value = 'null'::jsonb",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch null count");
        assert_eq!(null_count, 1, "audit-trail row missing for clear action");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_with_empty_body_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.text();
        assert!(
            body.contains("no fields"),
            "expected 'no fields' detail, got: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_clear_title_with_null_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let title_before: String =
            sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title before");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": serde_json::Value::Null}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "title is NOT NULL on works — clearing it must be rejected"
        );
        let body = response.text();
        assert!(
            body.contains("cannot clear title"),
            "expected 'cannot clear title' detail, got: {body}"
        );

        // Rejection happens before commit — the canonical title is intact
        // and the rolled-back tx leaves no manual audit row behind.
        let title_after: String =
            sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title after");
        assert_eq!(
            title_after, title_before,
            "title must be unchanged after a rejected clear"
        );
        let manual_title_rows: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" \
             FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'title'",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch manual title row count");
        assert_eq!(
            manual_title_rows, 0,
            "rejected clear must not persist an audit-trail row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_child_account_forbidden(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_child_id, basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "patch").await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": "Nope"}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_leaves_sibling_pending_drafts_alone(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // AI draft sitting pending on `title` BEFORE the manual edit.
        let ai_draft = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!("AI Draft Title"),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": "Operator-chosen"}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

        // AI draft stays pending — operator can later revert to it.
        let status: Option<String> = sqlx::query_scalar!(
            "SELECT status::text FROM metadata_versions WHERE id = $1",
            ai_draft,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch ai draft status");
        assert_eq!(
            status.as_deref(),
            Some("pending"),
            "sibling AI draft was mutated by the manual edit"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_returns_404_for_missing_manifestation(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let fake = Uuid::new_v4();
        let response = server
            .patch(&format!("/api/books/{fake}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": "ghost"}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn_set_triggers_rematch(pool: sqlx::PgPool) {
        // Two manifestations sharing an ISBN — patching the second
        // with that ISBN must wire `suspected_duplicate_work_id` on it.
        // Auto-merge is skipped because the PATCH inserts a manual
        // metadata draft on the current work (the orchestrator guards
        // against merging works that already carry manual drafts).
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_a).await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_b).await;

        let isbn = "9780306406157";
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            isbn,
            m_a,
        )
        .execute(&ing_pool)
        .await
        .expect("seed isbn_a");

        let response = server
            .patch(&format!("/api/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_13": isbn}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        // Rematch wired the suspected_duplicate_work_id pointer on m_b
        // back to work_a. The flag firing is the proof the route
        // correctly invoked rematch_on_isbn_change — without the route-
        // level wiring, this column stays NULL.
        let suspected: Option<Uuid> = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            m_b,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch suspected");
        assert_eq!(
            suspected,
            Some(work_a),
            "ISBN PATCH did not trigger rematch — suspected_duplicate_work_id is {suspected:?}, expected Some({work_a})"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn13_bad_check_digit_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // 13 digits, correct length, wrong check digit (valid form ends 7).
        let response = server
            .patch(&format!("/api/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_13": "9780306406150"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn10_wrong_length_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let response = server
            .patch(&format!("/api/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_10": "12345"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn13_non_numeric_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // 13 chars, but a letter where a digit must be.
        let response = server
            .patch(&format!("/api/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_13": "978030640615X"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_valid_isbn10_accepted(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Valid ISBN-10 (check digit 2) must pass the new guard.
        let response = server
            .patch(&format!("/api/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_10": "0306406152"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_hyphenated_isbn13_stored_normalized(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let response = server
            .patch(&format!("/api/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_13": "978-0-306-40615-7"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        // Stored canonical value is digits-only, matching the ingestion
        // surface so rematch exact-equality can find ingested twins.
        let stored: Option<String> =
            sqlx::query_scalar!("SELECT isbn_13 FROM manifestations WHERE id = $1", m)
                .fetch_one(&ing_pool)
                .await
                .expect("fetch isbn_13");
        assert_eq!(stored.as_deref(), Some("9780306406157"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_dashed_isbn_rematches_undashed_twin(pool: sqlx::PgPool) {
        // m_a carries an ingested (digits-only) ISBN; PATCHing m_b with the
        // same ISBN in dashed form must normalise on write so rematch's
        // exact-equality join wires the duplicate pointer.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_a).await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_b).await;

        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            "9780306406157",
            m_a,
        )
        .execute(&ing_pool)
        .await
        .expect("seed isbn_a");

        let response = server
            .patch(&format!("/api/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"isbn_13": "978-0-306-40615-7"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        let suspected: Option<Uuid> = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            m_b,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch suspected");
        assert_eq!(
            suspected,
            Some(work_a),
            "dashed PATCH did not rematch undashed twin — suspected_duplicate_work_id is {suspected:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn backfill_normalises_dashed_isbns(pool: sqlx::PgPool) {
        // Exercises the backfill statements from
        // 20260603032915_normalise_existing_isbns.up.sql against rows that
        // predate ISBN normalisation: dashed isbn_13 + spaced/lowercase
        // isbn_10, a `urn:isbn:` OPF prefix, and a leading-whitespace +
        // prefix value (the case where a missing trim would leave the prefix
        // literal and silently break rematch). The migration itself only
        // proves it applies to an empty table; this proves the transform
        // collapses real divergent data, including the prefix and whitespace
        // forms `normalise()` strips. The backfill SQL below is kept verbatim
        // in sync with the migration file.
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let prefix_marker = Uuid::new_v4().simple().to_string();
        let (_work2, m_prefix) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &prefix_marker).await;
        let ws_marker = Uuid::new_v4().simple().to_string();
        let (_work3, m_ws) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &ws_marker).await;

        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1, isbn_10 = $2 WHERE id = $3",
            "978-0-306-40615-7",
            "0-8044-2957-x",
            m,
        )
        .execute(&ing_pool)
        .await
        .expect("seed dashed isbns");
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            "urn:isbn:9780306406157",
            m_prefix,
        )
        .execute(&ing_pool)
        .await
        .expect("seed prefixed isbn");
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            " urn:isbn:9780306406157",
            m_ws,
        )
        .execute(&ing_pool)
        .await
        .expect("seed whitespace-prefixed isbn");

        sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = upper(regexp_replace(regexp_replace(regexp_replace(isbn_10, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g')) \
             WHERE isbn_10 IS NOT NULL \
               AND isbn_10 <> upper(regexp_replace(regexp_replace(regexp_replace(isbn_10, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g'))"
        )
        .execute(&ing_pool)
        .await
        .expect("backfill isbn_10");
        sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = regexp_replace(regexp_replace(regexp_replace(isbn_13, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g') \
             WHERE isbn_13 IS NOT NULL \
               AND isbn_13 <> regexp_replace(regexp_replace(regexp_replace(isbn_13, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g')"
        )
        .execute(&ing_pool)
        .await
        .expect("backfill isbn_13");

        let row = sqlx::query!(
            "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
            m
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch normalised");
        assert_eq!(row.isbn_13.as_deref(), Some("9780306406157"));
        assert_eq!(row.isbn_10.as_deref(), Some("080442957X"));

        let prefix_row = sqlx::query!("SELECT isbn_13 FROM manifestations WHERE id = $1", m_prefix)
            .fetch_one(&ing_pool)
            .await
            .expect("fetch prefix normalised");
        assert_eq!(prefix_row.isbn_13.as_deref(), Some("9780306406157"));

        let ws_row = sqlx::query!("SELECT isbn_13 FROM manifestations WHERE id = $1", m_ws)
            .fetch_one(&ing_pool)
            .await
            .expect("fetch whitespace-prefix normalised");
        assert_eq!(ws_row.isbn_13.as_deref(), Some("9780306406157"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_null_clear_does_not_surface_as_pending_draft(pool: sqlx::PgPool) {
        // Regression: PATCH-clear inserts an audit row with
        // new_value='null'::jsonb. Without the filter in
        // load_pending_versions, that row would surface in
        // BookDetail.metadata_versions as a draft with "(null)" value
        // and Accept/Reject buttons.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let initial = format!("seed prose {marker}");
        let seed_v =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            initial,
            seed_v,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"fields": {"description": serde_json::Value::Null}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

        // Now GET the detail and confirm the audit row is absent.
        let detail = server
            .get(&format!("/api/books/{m_id}"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(detail.status_code(), StatusCode::OK);
        let body: serde_json::Value = detail.json();
        let versions = body
            .get("metadata_versions")
            .and_then(|v| v.as_array())
            .expect("metadata_versions array present");
        for v in versions {
            assert_ne!(
                v.get("new_value"),
                Some(&serde_json::Value::Null),
                "null-value audit row leaked into Versions tab: {v}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_rejects_null_value_audit_row(pool: sqlx::PgPool) {
        // Regression for the null-accept corruption path: PATCH-clear
        // inserts a `new_value = 'null'::jsonb` audit row; the Versions
        // tab filters it, but POST /accept used to fetch any row by id
        // and run `apply_version`, which fell through to writing the
        // literal string "null" into the canonical column. The handler
        // must reject that promotion with 422.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let seed = format!("seed prose {marker}");
        let seed_v = insert_version(&ing_pool, m_id, "description", serde_json::json!(&seed)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            seed,
            seed_v,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        // PATCH description -> null. Creates a `new_value = 'null'`
        // audit row + clears the canonical column.
        let r = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"fields": {"description": serde_json::Value::Null}}))
            .await;
        assert_eq!(r.status_code(), StatusCode::NO_CONTENT);

        // Fetch the null-value row id directly. The Versions tab filter
        // hides it, so we read it from the table.
        let null_row_id: Uuid = sqlx::query_scalar!(
            "SELECT id FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'description' \
             AND new_value = 'null'::jsonb AND source = 'manual' \
             ORDER BY created_at DESC LIMIT 1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("null audit row exists");

        let accept = server
            .post(&format!("/api/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": null_row_id}))
            .await;
        assert_eq!(
            accept.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "accept of null-value row must 422; got body = {}",
            accept.text()
        );

        // Canonical column stays NULL (the PATCH-clear committed it).
        // No literal-"null" string corruption.
        let description: Option<String> =
            sqlx::query_scalar!("SELECT description FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch description");
        assert_eq!(
            description, None,
            "canonical description must stay NULL, not be overwritten with literal \"null\""
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reuses_conflicting_row_and_resets_status_to_pending(pool: sqlx::PgPool) {
        // Operator rejects their own manual entry, then re-saves the
        // same value. The conflict path must reset status back to
        // 'pending' so apply_version's canonical-pointer write doesn't
        // leave the row in 'rejected' state.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let value = format!("Manual title {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        // First save.
        let r1 = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"fields": {"title": value.clone()}}))
            .await;
        assert_eq!(r1.status_code(), StatusCode::NO_CONTENT);

        // Mark that manual row rejected directly (simulates an operator
        // rejecting their own manual entry via the per-row Reject button).
        let v1: Uuid = sqlx::query_scalar!(
            "SELECT title_version_id AS \"id?\" FROM works WHERE id = $1",
            work_id
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch v1")
        .expect("title_version_id wired by prior PATCH");
        sqlx::query!(
            "UPDATE metadata_versions SET status = 'rejected', resolved_at = now() WHERE id = $1",
            v1,
        )
        .execute(&app_pool)
        .await
        .expect("mark rejected");

        // Second save of the same value — should reset to pending.
        let r2 = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"title": value}}))
            .await;
        assert_eq!(r2.status_code(), StatusCode::NO_CONTENT);

        let status: String = sqlx::query_scalar!(
            "SELECT status::text AS \"status!\" FROM metadata_versions WHERE id = $1",
            v1,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch status");
        assert_eq!(status, "pending", "conflict path must reset status");
    }

    // ── Publisher hash convergence (debt: publisher-hash-divergence) ──────
    //
    // The manual edit path must hash via the same canonical normaliser as
    // the enrichment pipeline so whitespace-equivalent publishers dedup
    // identically regardless of entry path.

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_publisher_hash_matches_enrichment_canonical(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"fields": {"publisher": "  Acme Press  "}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "body = {}",
            response.text()
        );

        // The stored manual hash must equal the enrichment pipeline's
        // canonical hash for the trimmed-equivalent value — byte for byte.
        let stored: Vec<u8> = sqlx::query_scalar!(
            "SELECT value_hash AS \"value_hash!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'publisher'",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch stored publisher hash");

        let canonical = crate::services::enrichment::value_hash::value_hash(
            "publisher",
            &serde_json::json!("Acme Press"),
        );
        assert_eq!(
            stored, canonical,
            "manual publisher hash must equal enrichment canonical hash for whitespace-equivalent values"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_publisher_whitespace_variants_dedup(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        // Same logical publisher, divergent leading/trailing whitespace.
        for publisher in ["Penguin Random House", "  Penguin Random House  "] {
            let response = server
                .patch(&format!("/api/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"fields": {"publisher": publisher}}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::NO_CONTENT,
                "body = {}",
                response.text()
            );
        }

        let rows = sqlx::query!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'publisher'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch manual publisher rows");

        assert_eq!(
            rows.len(),
            1,
            "whitespace-variant publishers must collapse to one journal row; got {}",
            rows.len()
        );
        assert_eq!(
            rows[0].observation_count, 2,
            "second save must bump observation_count via ON CONFLICT, not insert a new row"
        );
    }
}
