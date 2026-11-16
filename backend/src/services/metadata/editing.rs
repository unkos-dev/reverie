//! Canonical metadata edits, review actions and matched snapshots.
//!
//! THREAT: shared operations retain caller scope, catalogue visibility and field-lock checks.

pub mod undo;

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::models::external_identifier::{
    IdentifierLevel, delete_manifestation_identifier, delete_work_identifier,
    upsert_manifestation_identifier, upsert_work_identifier,
};
use crate::models::{content_rating::ContentRating, work};
use crate::routes::etag::{Conditional, MetadataRevision, StrongEntityTag};
use crate::routes::library::{
    load_genres_for_manifestations, load_moods_for_manifestations, load_tags_for_manifestations,
};
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::value_hash;
use crate::services::metadata::external_id;
use crate::services::reader::{self, RelationKey, RelationKind, RelationPage};
use crate::{db, error::AppError, state::AppState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

pub struct SaveResult {
    pub snapshot: MetadataSnapshot,
    pub etag: axum::http::HeaderValue,
    pub request_id: Option<Uuid>,
    pub operations: Vec<undo::UndoOperation>,
}

// ── accept / reject / revert / lock ────────────────────────────────────────

/// Body for accept / reject: the targeted `metadata_versions` row.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VersionPayload {
    /// Metadata version row to act on.
    #[schema(max_length = 45)]
    pub version_id: Uuid,
}

/// Body for revert.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RevertPayload {
    /// Canonical field to revert (e.g. `title`).
    #[schema(max_length = 38)]
    pub field_name: String,
    /// Version to restore; `null` clears the canonical pointer AND the
    /// canonical column.
    #[schema(max_length = 45)]
    pub version_id: Option<Uuid>,
}

/// Body for lock / unlock.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LockPayload {
    /// Field to lock or unlock (e.g. `title`).
    pub field_name: String,
    /// Entity the lock applies to: `work` or `manifestation`.
    #[schema(schema_with = lock_entity_type_schema)]
    pub entity_type: String,
}

fn lock_entity_type_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .description(Some(
            "Entity the lock applies to: `work` or `manifestation`.",
        ))
        .enum_values(Some(["work", "manifestation"]))
        .examples(["manifestation"])
        .build()
}

/// Promote a pending
/// metadata version to canonical.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the version row does not belong to the
///   manifestation, is already rejected, or is RLS-hidden.
/// - [`AppError::Validation`] when the stored value fails field parsing.
/// - [`AppError::Internal`] on database errors.
pub async fn accept_manifestation(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    payload: VersionPayload,
    selection: Option<&SnapshotSelection>,
) -> Result<Option<MetadataSnapshot>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    lock_version_review(&mut tx, payload.version_id).await?;

    let row = sqlx::query!(
        "SELECT mv.id, mv.field_name, \
                mv.new_value AS \"new_value!\", \
                m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         JOIN works w ON w.id = m.work_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
           AND mv.status = 'pending'::metadata_review_status \
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

    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        std::slice::from_ref(&field_name),
    )
    .await?;

    // Accept promotes the version to canonical; the caller has no use for
    // the prior pointer (that's the revert/undo surface's concern).
    apply_version(
        &mut tx,
        &field_name,
        &new_value,
        version_id,
        manifestation_id,
        work_id,
    )
    .await?;

    if field_name == "isbn_10" || field_name == "isbn_13" {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    let snapshot = match selection {
        Some(selection) => {
            Some(project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?)
        }
        None => None,
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(snapshot)
}

const VERSION_REVIEW_LOCK_NAMESPACE: i32 = 0x4d45_5441;

async fn lock_version_review(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
) -> Result<(), AppError> {
    let key = version_id.to_string();
    sqlx::query!(
        "SELECT pg_advisory_xact_lock($1, hashtext($2))",
        VERSION_REVIEW_LOCK_NAMESPACE,
        key,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// `POST /api/v1/manifestations/{id}/metadata/reject` — mark a pending
/// metadata version as rejected.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the version row does not belong to the
///   manifestation or is RLS-hidden.
/// - [`AppError::Internal`] on database errors.
pub async fn reject_manifestation(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    payload: VersionPayload,
    selection: Option<&SnapshotSelection>,
) -> Result<Option<MetadataSnapshot>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    lock_version_review(&mut tx, payload.version_id).await?;

    let work_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    work_id.ok_or(AppError::NotFound)?;

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

    let snapshot = match selection {
        Some(selection) => {
            Some(project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?)
        }
        None => None,
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(snapshot)
}

/// Restore a prior
/// version as canonical, or clear the field entirely.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation or version is missing
///   or RLS-hidden.
/// - [`AppError::Validation`] when clearing a non-clearable field (e.g.
///   `title`) or the stored value fails parsing.
/// - [`AppError::Internal`] on database errors.
pub async fn revert_manifestation(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    payload: RevertPayload,
    selection: Option<&SnapshotSelection>,
) -> Result<Option<MetadataSnapshot>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
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

    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        std::slice::from_ref(&payload.field_name),
    )
    .await?;

    match payload.version_id {
        Some(vid) => {
            // Sibling manifestations of the same work may legitimately own
            // the version being restored for work-scoped fields; see
            // `is_work_scoped_field`.
            let is_work_scoped = is_work_scoped_field(&payload.field_name);
            let new_value: Option<Value> = sqlx::query_scalar!(
                "SELECT mv.new_value AS \"new_value!\" \
                 FROM metadata_versions mv \
                 JOIN manifestations vm ON vm.id = mv.manifestation_id \
                 WHERE mv.id = $1 \
                   AND (vm.id = $2 OR ($3::bool AND vm.work_id = $4))",
                vid,
                manifestation_id,
                is_work_scoped,
                work_id,
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            let new_value = new_value.ok_or(AppError::NotFound)?;
            // Revert-to-version restores canonical state; the review UI
            // does not surface the prior pointer from this endpoint today.
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

    if matches!(payload.field_name.as_str(), "isbn_10" | "isbn_13") {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
    }

    let snapshot = match selection {
        Some(selection) => {
            Some(project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?)
        }
        None => None,
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(snapshot)
}

/// Lock a metadata field
/// against manual canonical and automated enrichment writes.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
pub async fn lock_field(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    payload: LockPayload,
    selection: Option<&SnapshotSelection>,
) -> Result<Option<MetadataSnapshot>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    lock_visible_manifestation(&mut tx, manifestation_id).await?;
    field_lock::lock_tx(
        &mut tx,
        manifestation_id,
        entity,
        &payload.field_name,
        current_user.user_id,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let snapshot = match selection {
        Some(selection) => {
            Some(project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?)
        }
        None => None,
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(snapshot)
}

/// `POST /api/v1/manifestations/{id}/metadata/unlock` — remove a field
/// lock.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user, or when no matching lock exists.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
pub async fn unlock_field(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    payload: LockPayload,
    selection: Option<&SnapshotSelection>,
) -> Result<Option<MetadataSnapshot>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    lock_visible_manifestation(&mut tx, manifestation_id).await?;
    let removed = field_lock::unlock_tx(&mut tx, manifestation_id, entity, &payload.field_name)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !removed {
        return Err(AppError::NotFound);
    }
    let snapshot = match selection {
        Some(selection) => {
            Some(project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?)
        }
        None => None,
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(snapshot)
}

/// Resolve a manifestation through the caller's RLS context and hold its row
/// lock while a field lock mutation runs.
pub async fn lock_visible_manifestation(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    let visible = sqlx::query_scalar!(
        "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE",
        manifestation_id,
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    visible.ok_or(AppError::NotFound).map(|_| ())
}

/// Whether `field_name`'s canonical pointer lives on the shared `works`
/// row rather than on a specific `manifestations` row.
///
/// `title`/`subtitle`/`description`/`language` and every `contributors.*`
/// role write through to `works` (see `apply_version`), so a version
/// journaled under any manifestation of that work is a valid revert
/// target. `genres`/`moods`/`tags` are per-manifestation vocabulary
/// junctions despite also hanging off the work's editions, so they stay
/// out of this set and use the strict same-manifestation match.
/// `identifiers.work.*` values live in `work_external_identifiers`, keyed
/// by the shared work, so they are work-scoped too;
/// `identifiers.manifestation.*` values are per-edition and are not.
fn is_work_scoped_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "title" | "subtitle" | "description" | "language"
    ) || field_name.starts_with("contributors.")
        || field_name.starts_with("identifiers.work.")
}

async fn require_unlocked_fields(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    fields: &[String],
) -> Result<(), AppError> {
    // THREAT: The caller holds the edition row lock so lock changes cannot race this decision.
    let (work_fields, manifestation_fields): (Vec<_>, Vec<_>) = fields
        .iter()
        .cloned()
        .partition(|field| is_work_scoped_field(field));
    let locked =
        field_lock::locked_fields_tx(tx, manifestation_id, &work_fields, &manifestation_fields)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
    if locked.is_empty() {
        return Ok(());
    }
    tracing::info!(%user_id, %manifestation_id, "metadata write refused by field locks");
    Err(AppError::FieldLocked(locked))
}

/// Split a canonical `identifiers.<level>.<scheme>` field name into its level
/// and scheme, rejecting a malformed level segment or a scheme unknown at
/// that level. Shared by the PATCH map handler, `apply_version`, and
/// `clear_field` so every path addresses the registry identically.
fn parse_identifier_field(field: &str) -> Result<(IdentifierLevel, &str), AppError> {
    let rest = field
        .strip_prefix("identifiers.")
        .ok_or_else(|| AppError::Validation(format!("'{field}' is not an identifier field")))?;
    let (level_segment, scheme) = rest.split_once('.').ok_or_else(|| {
        AppError::Validation(format!(
            "identifier field '{field}' must be identifiers.<level>.<scheme>"
        ))
    })?;
    let level = IdentifierLevel::from_segment(level_segment).ok_or_else(|| {
        AppError::Validation(format!(
            "identifier level must be 'work' or 'manifestation', got '{level_segment}'"
        ))
    })?;
    external_id::validate_scheme_level(level, scheme).map_err(AppError::Validation)?;
    Ok((level, scheme))
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

fn normalise_isbn_field(field: &str, value: &Value) -> Result<String, AppError> {
    let invalid = || AppError::FieldValidation {
        detail: format!("invalid {field}"),
        errors: BTreeMap::from([(field.into(), format!("invalid {field}"))]),
    };
    let raw = value.as_str().ok_or_else(invalid)?;
    let parsed = if field == "isbn_10" {
        raw.parse::<isbn::Isbn10>().map(|value| value.to_string())
    } else {
        raw.parse::<isbn::Isbn13>().and_then(|value| {
            let compact = value.to_string();
            if compact.starts_with("978") || compact.starts_with("979") {
                Ok(compact)
            } else {
                Err(isbn::IsbnError::InvalidGroup)
            }
        })
    };
    parsed.map_err(|_| invalid())
}

/// Apply a specific version to its canonical column + pointer.
/// Reused by `/accept` and `/revert`.
//
// Field-dispatch with one `sqlx::query!` per supported field; macro-form
// expansion pushed this just past clippy's 100-line threshold.
#[expect(
    clippy::too_many_lines,
    reason = "field dispatch with one sqlx::query! per supported field; macro-form expansion pushes this past the 100-line threshold"
)]
pub async fn apply_version(
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
    let str_val = if matches!(field, "isbn_10" | "isbn_13") {
        normalise_isbn_field(field, value)?
    } else {
        value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned)
    };
    let update = match field {
        "title" => Some(sqlx::query!(
            "UPDATE works \
             SET title = $1, sort_title = lower($1), title_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "description" => Some(sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "language" => Some(sqlx::query!(
            "UPDATE works SET language = $1, language_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "subtitle" => Some(sqlx::query!(
            "UPDATE works SET subtitle = $1, subtitle_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "pages" => {
            let pages = value
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| AppError::Validation("pages must be a positive integer".into()))?;
            Some(sqlx::query!(
                "UPDATE manifestations SET pages = $1, pages_version_id = $2 \
                 WHERE id = $3",
                pages,
                version_id,
                manifestation_id,
            ))
        }
        "publisher" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET publisher = $1, publisher_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "isbn_10" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = $1, isbn_10_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "isbn_13" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = $1, isbn_13_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "pub_date" => {
            let date = crate::services::metadata::publication_date::normalise(&str_val)
                .and_then(|value| value.ok_or(crate::services::metadata::publication_date::PublicationDateError::Malformed))
                .map_err(|error| {
                    let message = format!("invalid publication date: {error}");
                    AppError::FieldValidation {
                        detail: message.clone(),
                        errors: std::collections::BTreeMap::from([("pub_date".into(), message)]),
                    }
                })?;
            Some(sqlx::query!(
                "UPDATE manifestations \
                 SET pub_date = $1, pub_date_version_id = $2 \
                 WHERE id = $3",
                date,
                version_id,
                manifestation_id,
            ))
        }
        "content_rating" => {
            let rating: ContentRating = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(
                    "content_rating must be one of everyone, teen, mature, adult, explicit".into(),
                )
            })?;
            Some(sqlx::query!(
                "UPDATE manifestations \
                 SET content_rating = $1, content_rating_version_id = $2 \
                 WHERE id = $3",
                rating as ContentRating,
                version_id,
                manifestation_id,
            ))
        }
        "genres" | "moods" | "tags" => {
            let vocab = VocabularyField::from_field_name(field).ok_or_else(|| {
                AppError::Validation(format!("unsupported vocabulary field '{field}'"))
            })?;
            let names: Vec<String> = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(format!("{field} must be an array of strings"))
            })?;
            let trimmed = validated_vocabulary_terms(field, &names)?;
            delete_vocabulary_rows(tx, manifestation_id, vocab).await?;
            insert_vocabulary_rows(tx, manifestation_id, vocab, &trimmed, version_id).await?;
            None
        }
        _ if field.starts_with("contributors.") => {
            let role = &field["contributors.".len()..];
            if !matches!(role, "author" | "editor" | "translator") {
                return Err(AppError::Validation(format!(
                    "unsupported contributor role '{role}'"
                )));
            }
            let names: Vec<String> = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(format!("{field} must be an array of strings"))
            })?;
            let trimmed = validated_role_names(role, &names)?;
            delete_role_rows(tx, work_id, role).await?;
            insert_role_rows(tx, work_id, role, &trimmed, version_id).await?;
            if role == "author" {
                let post_author_count: i64 = sqlx::query_scalar!(
                    "SELECT COUNT(*) AS \"count!\" FROM work_authors \
                     WHERE work_id = $1 AND role = 'author'",
                    work_id,
                )
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                if post_author_count == 0 {
                    return Err(AppError::Validation(
                        "a work must retain at least one author".into(),
                    ));
                }
                work::refresh_first_author_sort(tx, work_id)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
            }
            None
        }
        _ if field.starts_with("identifiers.") => {
            let (level, scheme) = parse_identifier_field(field)?;
            let raw = value
                .as_str()
                .ok_or_else(|| AppError::Validation(format!("{field} must be a string")))?;
            // Re-run the typed parser here (not only at the PATCH boundary)
            // so accept/revert of a journaled value cannot promote a
            // malformed identifier into the registry.
            let canonical =
                external_id::parse_external_id(level, scheme, raw).map_err(AppError::Validation)?;
            match level {
                IdentifierLevel::Work => {
                    upsert_work_identifier(
                        &mut **tx,
                        work_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                }
                IdentifierLevel::Manifestation => {
                    upsert_manifestation_identifier(
                        &mut **tx,
                        manifestation_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                }
            }
            None
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auto-apply field '{other}' (list/complex fields must be accepted via their dedicated routes)"
            )));
        }
    };
    if let Some(update) = update {
        update
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    // External identifiers are never written back to the source file, so an
    // identifier apply (manual set, accept, or revert) must not enqueue an
    // OPF writeback job.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
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
        "INSERT INTO writeback_jobs (manifestation_id, reason, field_name) VALUES ($1, $2, $3)",
        manifestation_id,
        reason,
        field,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Clear a field's canonical value and pointer. Reused by `/revert` (with
/// `version_id = null`) and the manual PATCH clear path.
///
/// Returns the pointer that was canonical before the clear (`None` when the
/// field was already unset, or when the field kind has no single-pointer
/// concept).
#[expect(
    clippy::too_many_lines,
    reason = "field dispatch with one sqlx::query! per supported field; macro-form expansion pushes this past the 100-line threshold"
)]
pub async fn clear_field(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    let update = match field {
        "title" => {
            return Err(AppError::Validation(
                "cannot clear title — revert to a specific version instead".into(),
            ));
        }
        "description" => Some(sqlx::query!(
            "UPDATE works SET description = NULL, description_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "language" => Some(sqlx::query!(
            "UPDATE works SET language = NULL, language_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "subtitle" => Some(sqlx::query!(
            "UPDATE works SET subtitle = NULL, subtitle_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "pages" => Some(sqlx::query!(
            "UPDATE manifestations SET pages = NULL, pages_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "publisher" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET publisher = NULL, publisher_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "pub_date" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET pub_date = NULL, pub_date_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "isbn_10" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = NULL, isbn_10_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "isbn_13" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = NULL, isbn_13_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "content_rating" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET content_rating = NULL, content_rating_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "genres" | "moods" | "tags" => {
            let vocab = VocabularyField::from_field_name(field).ok_or_else(|| {
                AppError::Validation(format!("unsupported vocabulary field '{field}'"))
            })?;
            delete_vocabulary_rows(tx, manifestation_id, vocab).await?;
            None
        }
        _ if field.starts_with("contributors.") => {
            let role = &field["contributors.".len()..];
            match role {
                // Mirrors the title arm above: clearing the sole author role
                // would leave the work with zero authors, which the schema's
                // consumers (sort, display) do not expect.
                "author" => {
                    return Err(AppError::Validation(
                        "cannot clear contributors.author: a work must retain at least one author"
                            .into(),
                    ));
                }
                "editor" | "translator" => {
                    delete_role_rows(tx, work_id, role).await?;
                }
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported contributor role '{other}'"
                    )));
                }
            }
            None
        }
        _ if field.starts_with("identifiers.") => {
            let (level, scheme) = parse_identifier_field(field)?;
            match level {
                IdentifierLevel::Work => {
                    delete_work_identifier(&mut **tx, work_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                }
                IdentifierLevel::Manifestation => {
                    delete_manifestation_identifier(&mut **tx, manifestation_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                }
            }
            None
        }
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    };
    if let Some(update) = update {
        update
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    // Identifier clears mirror identifier applies: no OPF writeback.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
    Ok(())
}

/// Maximum length (in `char`s) for a manually-entered contributor name.
const MAX_CONTRIBUTOR_NAME_CHARS: usize = 500;

/// Maximum number of names accepted per contributor role in one patch.
/// Bounds the per-name insert loop that runs under the handler's row lock.
const MAX_CONTRIBUTORS_PER_ROLE: usize = 100;

/// Validate one role's submitted names: bounded count, trimmed, non-empty,
/// length-capped, duplicate-free. Returns the trimmed list.
fn validated_role_names(role: &str, names: &[String]) -> Result<Vec<String>, AppError> {
    if names.len() > MAX_CONTRIBUTORS_PER_ROLE {
        return Err(AppError::Validation(format!(
            "{role} list exceeds {MAX_CONTRIBUTORS_PER_ROLE} names"
        )));
    }
    let mut trimmed: Vec<String> = Vec::with_capacity(names.len());
    for raw in names {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AppError::Validation(format!(
                "{role} name must not be empty"
            )));
        }
        if t.chars().count() > MAX_CONTRIBUTOR_NAME_CHARS {
            return Err(AppError::Validation(format!(
                "{role} name exceeds {MAX_CONTRIBUTOR_NAME_CHARS} characters"
            )));
        }
        trimmed.push(t.to_string());
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for t in &trimmed {
        if !seen.insert(t.as_str()) {
            return Err(AppError::Validation(format!("duplicate {role} name '{t}'")));
        }
    }
    Ok(trimmed)
}

async fn delete_role_rows(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM work_authors WHERE work_id = $1 AND role = ($2::text)::author_role",
        work_id,
        role,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn insert_role_rows(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    role: &str,
    names: &[String],
    version_id: Uuid,
) -> Result<(), AppError> {
    for (i, name) in names.iter().enumerate() {
        let sort_name = crate::services::metadata::extractor::generate_sort_name(name);
        let author_id = work::find_or_create_author(tx, name, &sort_name)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let position = i32::try_from(i).unwrap_or(i32::MAX);
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position, source_version_id) \
             VALUES ($1, $2, ($3::text)::author_role, $4, $5) \
             ON CONFLICT (work_id, author_id, role) DO NOTHING",
            work_id,
            author_id,
            role,
            position,
            version_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

/// Apply a `contributors` merge-patch: per role, replace/clear/reorder
/// `work_authors` rows and journal the change under `contributors.<role>`.
///
/// `patch = None` means the top-level `"contributors": null` was sent —
/// pure RFC 7396 member removal, treated as clearing all three editable
/// roles. An empty array is equivalent to `null` for a given role.
///
/// # Errors
/// - [`AppError::Validation`] on an empty/duplicate/over-length name, or
///   when clearing/reassigning authors would leave a previously-authored
///   work with zero authors (editor/translator-only edits on an
///   authorless stub are legal; the invariant only fires when the
///   `author` key was touched).
/// - [`AppError::Internal`] on database errors.
async fn apply_contributors_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    patch: Option<ContributorsPatch>,
) -> Result<(), AppError> {
    let (author, editor, translator) = match patch {
        None => (Some(None), Some(None), Some(None)),
        Some(p) => (p.author, p.editor, p.translator),
    };

    // Captured before any mutation so the last-author guard below compares
    // against the pre-patch state, not a role this same call already cleared.
    let pre_author_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) AS \"count!\" FROM work_authors WHERE work_id = $1 AND role = 'author'",
        work_id,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut author_touched = false;
    let mut touched = false;

    for (role, maybe_names) in [
        ("author", author),
        ("editor", editor),
        ("translator", translator),
    ] {
        let Some(names) = maybe_names else {
            continue; // absent from the patch: this role is untouched.
        };
        touched = true;
        if role == "author" {
            author_touched = true;
        }
        // Empty array ≡ null: both clear the role.
        let names = names.filter(|v| !v.is_empty());

        let key = format!("contributors.{role}");
        match names {
            None => {
                insert_manual_version(tx, manifestation_id, user_id, &key, &Value::Null).await?;
                delete_role_rows(tx, work_id, role).await?;
            }
            Some(names) => {
                let trimmed = validated_role_names(role, &names)?;
                let json =
                    serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
                let version_id =
                    insert_manual_version(tx, manifestation_id, user_id, &key, &json).await?;
                delete_role_rows(tx, work_id, role).await?;
                insert_role_rows(tx, work_id, role, &trimmed, version_id).await?;
            }
        }
    }

    if author_touched {
        let post_author_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if post_author_count == 0 && pre_author_count > 0 {
            return Err(AppError::Validation(
                "a work must retain at least one author".into(),
            ));
        }
    }

    // The denormalized sort column derives from the author role only, so
    // editor/translator-only patches skip the refresh (and its updated_at
    // trigger bump).
    if author_touched {
        work::refresh_first_author_sort(tx, work_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    // The library path renders from the primary author, so a contributor
    // change must enqueue writeback like every scalar field in this handler.
    if touched {
        enqueue_writeback(tx, manifestation_id, "contributors").await?;
    }
    Ok(())
}

// ── vocabulary junction fields (genres / moods / tags) ────────────────────

/// Maximum length (in `char`s) for a manually-entered vocabulary term.
const MAX_VOCABULARY_TERM_CHARS: usize = 100;

/// Maximum number of terms accepted per vocabulary field in one patch.
const MAX_VOCABULARY_TERMS_PER_FIELD: usize = 50;

/// Vocabulary junction targets editable from the manual PATCH surface and
/// the accept/revert machinery. Each maps to a vocabulary table plus a
/// junction table mirroring `manifestation_tags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VocabularyField {
    Genres,
    Moods,
    Tags,
}

impl VocabularyField {
    pub const fn field_name(self) -> &'static str {
        match self {
            Self::Genres => "genres",
            Self::Moods => "moods",
            Self::Tags => "tags",
        }
    }

    fn from_field_name(field: &str) -> Option<Self> {
        match field {
            "genres" => Some(Self::Genres),
            "moods" => Some(Self::Moods),
            "tags" => Some(Self::Tags),
            _ => None,
        }
    }
}

/// Validate one vocabulary field's submitted terms: bounded count, trimmed,
/// non-empty, length-capped, duplicate-free. Duplicates are checked
/// case-insensitively because the vocabulary tables are unique on
/// `lower(name)`. Returns the trimmed list in submission order.
fn validated_vocabulary_terms(field: &str, names: &[String]) -> Result<Vec<String>, AppError> {
    if names.len() > MAX_VOCABULARY_TERMS_PER_FIELD {
        return Err(AppError::Validation(format!(
            "{field} list exceeds {MAX_VOCABULARY_TERMS_PER_FIELD} terms"
        )));
    }
    let mut trimmed: Vec<String> = Vec::with_capacity(names.len());
    for raw in names {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AppError::Validation(format!(
                "{field} term must not be empty"
            )));
        }
        if t.chars().count() > MAX_VOCABULARY_TERM_CHARS {
            return Err(AppError::Validation(format!(
                "{field} term exceeds {MAX_VOCABULARY_TERM_CHARS} characters"
            )));
        }
        trimmed.push(t.to_string());
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in &trimmed {
        if !seen.insert(t.to_lowercase()) {
            return Err(AppError::Validation(format!(
                "duplicate {field} term '{t}'"
            )));
        }
    }
    Ok(trimmed)
}

async fn delete_vocabulary_rows(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: VocabularyField,
) -> Result<(), AppError> {
    match field {
        VocabularyField::Genres => {
            sqlx::query!(
                "DELETE FROM manifestation_genres WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Moods => {
            sqlx::query!(
                "DELETE FROM manifestation_moods WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Tags => {
            sqlx::query!(
                "DELETE FROM manifestation_tags WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
    }
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Find-or-create every term and link it to the manifestation, carrying the
/// journal pointer onto each junction row. Set-based: one statement per call.
/// The `ON CONFLICT ((lower(name))) DO UPDATE` leg returns ids for existing
/// terms too, preserving their stored display spelling.
///
/// The `DISTINCT ON (lower(name)) .. ORDER BY lower(name)` source does two
/// jobs: it guards the single-statement upsert against case collisions the
/// Rust-side validation missed (Postgres `lower()` can fold pairs that
/// `str::to_lowercase` keeps distinct, and upserting the same row twice in
/// one statement is an error), and it fixes the row-lock acquisition order
/// so concurrent patches upserting overlapping terms cannot deadlock.
pub async fn insert_vocabulary_rows(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: VocabularyField,
    names: &[String],
    version_id: Uuid,
) -> Result<(), AppError> {
    match field {
        VocabularyField::Genres => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO genres (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = genres.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_genres (manifestation_id, genre_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, genre_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Moods => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO moods (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = moods.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_moods (manifestation_id, mood_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, mood_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Tags => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO tags (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = tags.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_tags (manifestation_id, tag_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, tag_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
    }
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Apply one vocabulary field's merge-patch: journal the full replacement
/// list under the field name and rebuild the junction rows with the new
/// journal pointer.
///
/// `None` (top-level `null`) and an empty array both clear the field; the
/// clear is journaled as a null-valued accountability row, mirroring the
/// scalar clear path.
///
/// # Errors
/// - [`AppError::Validation`] on an empty/duplicate/over-length term or an
///   over-long list.
/// - [`AppError::Internal`] on database errors.
async fn apply_vocabulary_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: VocabularyField,
    maybe_names: Option<Vec<String>>,
) -> Result<(), AppError> {
    let key = field.field_name();
    // Empty array ≡ null: both clear the field.
    let names = maybe_names.filter(|v| !v.is_empty());
    match names {
        None => {
            insert_manual_version(tx, manifestation_id, user_id, key, &Value::Null).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
        }
        Some(names) => {
            let trimmed = validated_vocabulary_terms(key, &names)?;
            let json = serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
            let version_id =
                insert_manual_version(tx, manifestation_id, user_id, key, &json).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
            insert_vocabulary_rows(tx, manifestation_id, field, &trimmed, version_id).await?;
        }
    }
    enqueue_writeback(tx, manifestation_id, key).await?;
    Ok(())
}

// ── manual metadata edit (RFC 7396 JSON Merge Patch) ──────────────────────

/// Per-field sparse update body for `PATCH /api/v1/books/{id}/metadata`.
///
/// Encodes RFC 7396 JSON Merge Patch semantics via
/// `serde_with::rust::double_option` (per
/// `docs/adr/0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md`):
/// * absent key → `None`         → leave field unchanged.
/// * key = `null` → `Some(None)` → clear the canonical value.
/// * key = value → `Some(Some(v))` → set the canonical value.
///
/// `cover` is not yet operator-editable from this endpoint — cover
/// promotion happens via a separate file-upload path.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding — outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateMetadataFields {
    /// Client-generated save identifier for resolution; an already recorded identifier is refused.
    #[schema(max_length = 45)]
    pub request_id: Option<Uuid>,
    /// New title. Absent = unchanged; `null` is rejected (canonical title
    /// is NOT NULL).
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub title: Option<Option<String>>,
    /// New description. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    /// New language. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub language: Option<Option<String>>,
    /// New publisher. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub publisher: Option<Option<String>>,
    /// New publication text at its supplied precision. Absent = unchanged;
    /// `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub pub_date: Option<Option<String>>,
    /// New ISBN-10. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub isbn_10: Option<Option<String>>,
    /// New ISBN-13. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub isbn_13: Option<Option<String>>,
    /// New subtitle. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub subtitle: Option<Option<String>>,
    /// New page count. Absent = unchanged; `null` clears; must be positive.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<i32>, minimum = 1)]
    pub pages: Option<Option<i32>>,
    /// New content rating tier. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ContentRating>)]
    pub content_rating: Option<Option<ContentRating>>,
    /// Full replacement genre list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    pub genres: Option<Option<Vec<String>>>,
    /// Full replacement mood list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    pub moods: Option<Option<Vec<String>>>,
    /// Full replacement tag list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    pub tags: Option<Option<Vec<String>>>,
    /// Per-role contributor replace/clear. Absent = unchanged; `null` clears
    /// author, editor, and translator together (pure RFC 7396 member removal).
    /// Handled separately from the other fields — not part of [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ContributorsPatch>)]
    pub contributors: Option<Option<ContributorsPatch>>,
    /// Work-level external identifiers keyed by scheme (e.g.
    /// `{"openlibrary": "OL45804W"}`). Each entry independently sets its
    /// scheme's single slot; a `null` entry clears it. Absent (or `null`)
    /// leaves all work-level identifiers unchanged. Handled separately from
    /// the scalar fields — not part of [`Self::populated`].
    #[serde(default)]
    #[schema(schema_with = update_metadata_work_identifiers_schema)]
    pub work_identifiers: Option<BTreeMap<String, Option<String>>>,
    /// Manifestation-level external identifiers keyed by scheme (e.g.
    /// `{"googlebooks": "zyTZAAAAYAAJ"}`). Same per-entry set/clear semantics
    /// as `work_identifiers`.
    #[serde(default)]
    #[schema(schema_with = update_metadata_manifestation_identifiers_schema)]
    pub manifestation_identifiers: Option<BTreeMap<String, Option<String>>>,
}

/// utoipa 5.5's `#[schema(...)]` derive attribute has no `max_properties`
/// (`openapi::schema::Object` does), so the bounded identifier/response map
/// fields below build their schema by hand via `schema_with`.
fn nullable_string_value_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::String, Type::Null]))
        .build()
}

fn string_property_names() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .build()
}

fn update_metadata_work_identifiers_schema() -> utoipa::openapi::schema::Object {
    update_metadata_identifiers_schema(
        "Work-level external identifiers keyed by scheme (e.g.\n\
             `{\"openlibrary\": \"OL45804W\"}`). Each entry independently sets its\n\
             scheme's single slot; a `null` entry clears it. Absent (or `null`)\n\
             leaves all work-level identifiers unchanged. Handled separately from\n\
             the scalar fields — not part of [`Self::populated`].",
    )
}

fn update_metadata_manifestation_identifiers_schema() -> utoipa::openapi::schema::Object {
    update_metadata_identifiers_schema(
        "Manifestation-level external identifiers keyed by scheme (e.g.\n\
             `{\"googlebooks\": \"zyTZAAAAYAAJ\"}`). Same per-entry set/clear semantics\n\
             as `work_identifiers`.",
    )
}

fn update_metadata_identifiers_schema(description: &str) -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .description(Some(description))
        .additional_properties(Some(utoipa::openapi::Schema::Object(
            nullable_string_value_schema(),
        )))
        .property_names(Some(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .max_length(Some(12))
                .build(),
        ))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

/// Per-role contributor replace/clear, nested under `UpdateMetadataFields::contributors`.
///
/// Each role independently follows RFC 7396 double-`Option` semantics:
/// absent = untouched, `null` (or an empty array) clears the role, a
/// non-empty array replaces it wholesale in the given order. `narrator` is
/// deliberately not accepted here (reserved, not yet operator-writable);
/// any unrecognised key 422s via `deny_unknown_fields`.
#[expect(
    clippy::option_option,
    reason = "see UpdateMetadataFields struct attribute"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ContributorsPatch {
    /// Ordered author names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    pub author: Option<Option<Vec<String>>>,
    /// Ordered editor names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    pub editor: Option<Option<Vec<String>>>,
    /// Ordered translator names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    pub translator: Option<Option<Vec<String>>>,
}

/// Response body for `PATCH /api/v1/books/{id}/metadata`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UpdateMetadataResponse {
    /// Save identifier for session-bound outcome resolution; absent for non-browser credentials.
    pub request_id: Option<Uuid>,
    /// Independent exact field operations issued to the authenticated browser session.
    #[schema(max_items = 144)]
    pub operations: Vec<undo::UndoOperation>,
    /// Complete canonical metadata paired with the response `ETag`.
    pub values: BookMetadata,
}

impl UpdateMetadataFields {
    /// Collect every populated entry as `(field_name, optional value)`
    /// — `None` means "clear this field", `Some(_)` means "set to this".
    ///
    /// `None` for the outer Option means the key was absent from the
    /// JSON body and is therefore not iterated; that case never appears
    /// in the returned vec.
    ///
    /// `pages` widens this to `Value` (rather than adding a parallel
    /// int-typed path) so every scalar field, string or numeric, flows
    /// through the same apply/clear plumbing.
    fn populated(self) -> Vec<(&'static str, Option<Value>)> {
        let mut out: Vec<(&'static str, Option<Value>)> = Vec::new();
        if let Some(v) = self.title {
            out.push(("title", v.map(Value::String)));
        }
        if let Some(v) = self.description {
            out.push(("description", v.map(Value::String)));
        }
        if let Some(v) = self.language {
            out.push(("language", v.map(Value::String)));
        }
        if let Some(v) = self.publisher {
            out.push(("publisher", v.map(Value::String)));
        }
        if let Some(v) = self.pub_date {
            out.push(("pub_date", v.map(Value::String)));
        }
        if let Some(v) = self.isbn_10 {
            out.push(("isbn_10", v.map(Value::String)));
        }
        if let Some(v) = self.isbn_13 {
            out.push(("isbn_13", v.map(Value::String)));
        }
        if let Some(v) = self.subtitle {
            out.push(("subtitle", v.map(Value::String)));
        }
        if let Some(v) = self.pages {
            out.push(("pages", v.map(Value::from)));
        }
        if let Some(v) = self.content_rating {
            out.push((
                "content_rating",
                v.map(|r| Value::String(r.as_str().to_owned())),
            ));
        }
        out
    }
}

/// Contributor names by role, mirroring [`ContributorsPatch`]'s roles for
/// the matched-GET response.
#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
pub struct MetadataContributors {
    /// Ordered author names.
    pub author: Vec<String>,
    /// Ordered editor names.
    pub editor: Vec<String>,
    /// Ordered translator names.
    pub translator: Vec<String>,
}

/// Response body for `GET /api/v1/books/{id}/metadata` — exactly the span
/// [`UpdateMetadataFields`] can write, field for field.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BookMetadata {
    /// Canonical title (work-level, `NOT NULL`).
    pub title: String,
    /// Subtitle; `null` when unset.
    pub subtitle: Option<String>,
    /// Description; `null` when unset.
    pub description: Option<String>,
    /// Language; `null` when unset.
    pub language: Option<String>,
    /// Publisher; `null` when unset.
    pub publisher: Option<String>,
    /// Publication text at its supplied precision; `null` when unset.
    pub pub_date: Option<String>,
    /// ISBN-10; `null` when unset.
    pub isbn_10: Option<String>,
    /// ISBN-13; `null` when unset.
    pub isbn_13: Option<String>,
    /// Whether the saved ISBN pair contains valid, non-equivalent numbers.
    pub isbn_mismatch: bool,
    /// Page count; `null` when unset.
    pub pages: Option<i32>,
    /// Content rating tier; `null` when unset.
    pub content_rating: Option<ContentRating>,
    /// Genre names, alphabetical; empty when none are set.
    pub genres: Vec<String>,
    /// Mood names, alphabetical; empty when none are set.
    pub moods: Vec<String>,
    /// Tag names, alphabetical; empty when none are set.
    pub tags: Vec<String>,
    /// Author/editor/translator names, each in stored position order.
    pub contributors: MetadataContributors,
    /// Work-level external identifiers keyed by scheme.
    #[schema(schema_with = book_metadata_work_identifiers_schema)]
    pub work_identifiers: BTreeMap<String, String>,
    /// Manifestation-level external identifiers keyed by scheme.
    #[schema(schema_with = book_metadata_manifestation_identifiers_schema)]
    pub manifestation_identifiers: BTreeMap<String, String>,
}

fn book_metadata_work_identifiers_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some("Work-level external identifiers keyed by scheme."))
        .additional_properties(Some(ObjectBuilder::new().schema_type(Type::String)))
        .property_names(Some(string_property_names()))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

fn book_metadata_manifestation_identifiers_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some(
            "Manifestation-level external identifiers keyed by scheme.",
        ))
        .additional_properties(Some(ObjectBuilder::new().schema_type(Type::String)))
        .property_names(Some(string_property_names()))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SnapshotPage {
    pub kind: RelationKind,
    pub first: i32,
    pub after: Option<String>,
}
impl SnapshotPage {
    pub fn key(&self, revision: &MetadataRevision) -> RelationKey {
        RelationKey {
            kind: self.kind,
            first: self.first,
            after: self.after.clone(),
            parent: match self.kind {
                RelationKind::Authors
                | RelationKind::Contributors
                | RelationKind::WorkIdentifiers => revision.work_id,
                _ => revision.manifestation_id,
            },
        }
    }
}
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SnapshotKey {
    pub manifestation_id: Uuid,
    pub pages: Vec<SnapshotPage>,
}
pub enum SnapshotSelection {
    Complete,
    Selected(Vec<SnapshotPage>),
}
pub struct MetadataSnapshot {
    pub values: BookMetadata,
    pub revision: MetadataRevision,
    pub pages: HashMap<RelationKey, RelationPage>,
}
#[derive(Clone)]
struct SnapshotCore {
    work_id: Uuid,
    manifestation_id: Uuid,
    work_revision: i64,
    manifestation_revision: i64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    language: Option<String>,
    publisher: Option<String>,
    pub_date: Option<String>,
    isbn_10: Option<String>,
    isbn_13: Option<String>,
    pages: Option<i32>,
    content_rating: Option<ContentRating>,
}
async fn snapshot_cores(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<SnapshotCore>, AppError> {
    sqlx::query_as!(
        SnapshotCore,
        r#"
        SELECT m.id AS "manifestation_id!", w.id             AS "work_id!",
               w.metadata_revision AS "work_revision!",
               m.metadata_revision AS "manifestation_revision!",
               w.title          AS "title!",
               w.subtitle       AS subtitle,
               w.description    AS description,
               w.language       AS language,
               m.publisher      AS publisher,
               m.pub_date       AS pub_date,
               m.isbn_10        AS isbn_10,
               m.isbn_13        AS isbn_13,
               m.pages          AS pages,
               m.content_rating AS "content_rating: ContentRating"
          FROM manifestations m
          JOIN works w ON w.id = m.work_id
         WHERE m.id = ANY($1)
        "#,
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}
fn core_snapshot(core: SnapshotCore) -> MetadataSnapshot {
    MetadataSnapshot {
        revision: MetadataRevision {
            work_id: core.work_id,
            manifestation_id: core.manifestation_id,
            work_revision: core.work_revision,
            manifestation_revision: core.manifestation_revision,
        },
        values: BookMetadata {
            title: core.title,
            subtitle: core.subtitle,
            description: core.description,
            language: core.language,
            publisher: core.publisher,
            pub_date: core.pub_date,
            isbn_mismatch: match (core.isbn_10.as_deref(), core.isbn_13.as_deref()) {
                (Some(ten), Some(thirteen)) => match (
                    ten.parse::<isbn::Isbn10>(),
                    thirteen.parse::<isbn::Isbn13>(),
                ) {
                    (Ok(ten), Ok(thirteen)) => {
                        let compact = thirteen.to_string();
                        (compact.starts_with("978") || compact.starts_with("979"))
                            && isbn::Isbn13::from(ten) != thirteen
                    }
                    _ => false,
                },
                _ => false,
            },
            isbn_10: core.isbn_10,
            isbn_13: core.isbn_13,
            pages: core.pages,
            content_rating: core.content_rating,
            genres: Vec::new(),
            moods: Vec::new(),
            tags: Vec::new(),
            contributors: MetadataContributors::default(),
            work_identifiers: BTreeMap::new(),
            manifestation_identifiers: BTreeMap::new(),
        },
        pages: HashMap::new(),
    }
}

async fn load_book_metadata(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Option<MetadataSnapshot>, AppError> {
    let Some(core) = snapshot_cores(tx, &[manifestation_id]).await?.pop() else {
        return Ok(None);
    };
    let mut snapshot = core_snapshot(core);
    let ids = &[manifestation_id];
    snapshot.values.genres = load_genres_for_manifestations(tx, ids)
        .await?
        .remove(&manifestation_id)
        .unwrap_or_default();
    snapshot.values.moods = load_moods_for_manifestations(tx, ids)
        .await?
        .remove(&manifestation_id)
        .unwrap_or_default();
    snapshot.values.tags = load_tags_for_manifestations(tx, ids)
        .await?
        .remove(&manifestation_id)
        .unwrap_or_default();
    snapshot.values.contributors =
        load_contributors_for_work(tx, snapshot.revision.work_id).await?;
    snapshot.values.work_identifiers = load_work_identifiers(tx, snapshot.revision.work_id).await?;
    snapshot.values.manifestation_identifiers =
        load_manifestation_identifiers(tx, manifestation_id).await?;
    Ok(Some(snapshot))
}
async fn selected_metadata(
    user: &CurrentUser,
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    keys: &[SnapshotKey],
) -> Result<HashMap<SnapshotKey, MetadataSnapshot>, AppError> {
    let ids: Vec<_> = keys.iter().map(|key| key.manifestation_id).collect();
    let cores: HashMap<_, _> = snapshot_cores(tx, &ids)
        .await?
        .into_iter()
        .map(|core| (core.manifestation_id, core))
        .collect();
    let mut relations: Vec<_> = keys
        .iter()
        .filter_map(|key| cores.get(&key.manifestation_id).map(|core| (key, core)))
        .flat_map(|(key, core)| {
            let revision = MetadataRevision {
                work_id: core.work_id,
                manifestation_id: core.manifestation_id,
                work_revision: core.work_revision,
                manifestation_revision: core.manifestation_revision,
            };
            key.pages.iter().map(move |page| page.key(&revision))
        })
        .collect();
    let mut distinct = std::collections::HashSet::new();
    relations.retain(|key| distinct.insert(key.clone()));
    let pages = reader::relation_pages_in_tx(user, state, tx, &relations).await?;
    let mut result = HashMap::new();
    for key in keys {
        if let Some(core) = cores.get(&key.manifestation_id) {
            let mut snapshot = core_snapshot(core.clone());
            for page in &key.pages {
                let relation = page.key(&snapshot.revision);
                if let Some(value) = pages.get(&relation) {
                    snapshot.pages.insert(relation, value.clone());
                }
            }
            result.insert(key.clone(), snapshot);
        }
    }
    Ok(result)
}
pub async fn snapshot_pages(
    user: &CurrentUser,
    state: &AppState,
    keys: &[SnapshotKey],
) -> Result<HashMap<SnapshotKey, MetadataSnapshot>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let result = selected_metadata(user, state, &mut tx, keys).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(result)
}
async fn project_metadata(
    user: &CurrentUser,
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    selection: &SnapshotSelection,
) -> Result<MetadataSnapshot, AppError> {
    match selection {
        SnapshotSelection::Complete => load_book_metadata(tx, manifestation_id)
            .await?
            .ok_or(AppError::NotFound),
        SnapshotSelection::Selected(pages) => {
            let key = SnapshotKey {
                manifestation_id,
                pages: pages.clone(),
            };
            selected_metadata(user, state, tx, std::slice::from_ref(&key))
                .await?
                .remove(&key)
                .ok_or(AppError::NotFound)
        }
    }
}
async fn metadata_revision(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<MetadataRevision, AppError> {
    snapshot_cores(tx, &[manifestation_id])
        .await?
        .pop()
        .map(|core| core_snapshot(core).revision)
        .ok_or(AppError::NotFound)
}

/// The editable metadata span for one manifestation.
///
/// This is the matched-pair read for [`update_book_metadata`]: same URI and
/// id key, so a read-modify-write flow targets one resource.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
pub async fn get_book_metadata(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
) -> Result<MetadataSnapshot, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_snapshot(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let metadata = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or(AppError::NotFound)?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(metadata)
}

/// Author/editor/translator names for a work, each ordered by stored
/// `work_authors.position` within the role. Only these three PATCH-writable
/// roles are included; narrators are stored but excluded from this
/// representation.
async fn load_contributors_for_work(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<MetadataContributors, AppError> {
    let rows = sqlx::query!(
        r#"SELECT wa.role::text AS "role!", a.name AS "name!"
             FROM work_authors wa
             JOIN authors a ON a.id = wa.author_id
            WHERE wa.work_id = $1
              AND wa.role = ANY(ARRAY['author', 'editor', 'translator']::author_role[])
            ORDER BY wa.role, wa.position ASC"#,
        work_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let mut contributors = MetadataContributors {
        author: Vec::new(),
        editor: Vec::new(),
        translator: Vec::new(),
    };
    for row in rows {
        match row.role.as_str() {
            "author" => contributors.author.push(row.name),
            "editor" => contributors.editor.push(row.name),
            "translator" => contributors.translator.push(row.name),
            other => {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "unexpected author_role '{other}' in work_authors"
                )));
            }
        }
    }
    Ok(contributors)
}

/// Work-level external identifiers keyed by scheme.
async fn load_work_identifiers(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(sqlx::query!(
        "SELECT scheme, external_id FROM work_external_identifiers WHERE work_id = $1",
        work_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| (r.scheme, r.external_id))
    .collect())
}

/// Manifestation-level external identifiers keyed by scheme.
async fn load_manifestation_identifiers(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(sqlx::query!(
        "SELECT scheme, external_id FROM manifestation_external_identifiers \
         WHERE manifestation_id = $1",
        manifestation_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| (r.scheme, r.external_id))
    .collect())
}

/// `PATCH /api/v1/books/{id}/metadata` — manual operator edit.
///
/// Bare RFC 7396 JSON Merge Patch body (no envelope): absent keys are
/// unchanged, `null` clears (except `title`). Accepts both
/// `application/json` and `application/merge-patch+json`: axum's `Json`
/// extractor already treats any `+json`-suffixed content type as JSON.
///
/// For each touched field the handler inserts a `metadata_versions` row
/// with `source = 'manual'` and then either promotes that row to canonical
/// via [`apply_version`] (when a value is supplied) or clears the
/// canonical column via [`clear_field`] (when the value is `null`). The
/// pending AI/OPF drafts on the same field stay `status = 'pending'`, so the
/// operator can revert to one of them later. The response carries the
/// complete canonical values, their matching validator and independent exact field operations.
///
/// # Errors
/// - [`AppError::Validation`] when the body has no populated fields,
///   when an ISBN/date value fails parsing, when a vocabulary list
///   (`genres`/`moods`/`tags`) contains an empty, duplicate, or
///   over-length term, or when the operator tries to clear `title`
///   (canonical title is `NOT NULL` on `works`). Reached only once
///   `If-Match` has matched, so a stale tag yields 412 rather than a
///   body error.
/// - [`AppError::MalformedHeader`] when `If-Match` violates the RFC 9110
///   §8.8.3 entity-tag grammar, or carries a form this API refuses by
///   policy: the `*` wildcard, an entity-tag list, a weak validator, or
///   more than one instance of the field.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden
///   by RLS for the current user (existence-not-leaked).
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::IfMatchRequired`] (428) when `If-Match` is absent.
/// - [`AppError::IfMatchMismatch`] (412) when `If-Match` does not match the
///   manifestation's current metadata `ETag`; the response carries the
///   current `ETag` so the caller can resync in one round trip.
/// - [`AppError::Internal`] on database errors.
#[expect(
    clippy::too_many_lines,
    reason = "sequential dispatch across the six patchable field families (scalars, vocabularies, contributors, identifiers, ISBN rematch, response assembly); each family already delegates to its own helper"
)]
#[warn(clippy::cognitive_complexity)]
pub async fn update_book_metadata(
    current_user: &CurrentUser,
    state: &AppState,
    session: &tower_sessions::Session,
    manifestation_id: Uuid,
    if_match: Option<StrongEntityTag>,
    mut req_fields: UpdateMetadataFields,
    selection: &SnapshotSelection,
) -> Result<Conditional<SaveResult>, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let if_match = if_match.ok_or(AppError::IfMatchRequired)?;
    // Extract contributors BEFORE populated() consumes the struct — it is
    // handled separately from the other (scalar) fields.
    let mut browser_save = undo::browser_save(current_user, session, req_fields.request_id.take())?;
    let contributors = req_fields.contributors.take();
    let genres = req_fields.genres.take();
    let moods = req_fields.moods.take();
    let tags = req_fields.tags.take();
    let work_identifiers = req_fields.work_identifiers.take();
    let manifestation_identifiers = req_fields.manifestation_identifiers.take();
    let fields = req_fields.populated();

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock the manifestation + parent work for the duration of the tx.
    // Mirrors `revert_manifestation` — serialises concurrent edits and
    // gates RLS-hidden rows to 404 (the join through `manifestations`
    // applies the RLS predicate).
    undo::require_save_session(&mut tx, browser_save.as_mut()).await?;
    let work_id = undo::lock_parents(&mut tx, manifestation_id).await?;

    // Compared under the row lock just taken above, so a concurrent editor
    // cannot slip a change in between this check and the writes below.
    let current_etag = metadata_revision(&mut tx, manifestation_id).await?.etag()?;
    if !if_match.matches(&current_etag) {
        return Ok(Conditional::Mismatch(current_etag));
    }

    // Semantic body validation runs only once the precondition has held.
    // RFC 9110 §13.2.1 places precondition evaluation after the normal
    // request checks (auth, existence) and before the request content is
    // processed, so a caller holding a stale representation learns that
    // first and refetches, rather than being sent to fix a body that a
    // concurrent write may already have made irrelevant. The remaining
    // per-field rejections below already sat on this side of the check.
    if fields.is_empty()
        && contributors.is_none()
        && genres.is_none()
        && moods.is_none()
        && tags.is_none()
        && work_identifiers.as_ref().is_none_or(BTreeMap::is_empty)
        && manifestation_identifiers
            .as_ref()
            .is_none_or(BTreeMap::is_empty)
    {
        return Err(AppError::Validation("no fields".into()));
    }

    let mut touched_fields: Vec<String> = fields
        .iter()
        .map(|(field, _)| (*field).to_owned())
        .collect();
    touched_fields.extend(
        [
            ("genres", genres.is_some()),
            ("moods", moods.is_some()),
            ("tags", tags.is_some()),
        ]
        .into_iter()
        .filter(|(_, touched)| *touched)
        .map(|(field, _)| field.to_owned()),
    );
    if let Some(patch) = &contributors {
        touched_fields.extend(
            [
                ("author", patch.as_ref().is_none_or(|p| p.author.is_some())),
                ("editor", patch.as_ref().is_none_or(|p| p.editor.is_some())),
                (
                    "translator",
                    patch.as_ref().is_none_or(|p| p.translator.is_some()),
                ),
            ]
            .into_iter()
            .filter(|(_, touched)| *touched)
            .map(|(role, _)| format!("contributors.{role}")),
        );
    }
    for (level, identifiers) in [
        (IdentifierLevel::Work, &work_identifiers),
        (IdentifierLevel::Manifestation, &manifestation_identifiers),
    ] {
        if let Some(identifiers) = identifiers {
            if identifiers.len() > MAX_IDENTIFIERS_PER_LEVEL {
                return Err(AppError::Validation(format!(
                    "{}-level identifiers exceed {MAX_IDENTIFIERS_PER_LEVEL} entries",
                    level.as_str()
                )));
            }
            touched_fields.extend(
                identifiers
                    .keys()
                    .map(|scheme| level.canonical_field(scheme)),
            );
        }
    }
    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        &touched_fields,
    )
    .await?;

    let before = undo::begin_save(
        &mut tx,
        browser_save,
        manifestation_id,
        work_id,
        &touched_fields,
    )
    .await?;

    let touched_isbn = fields
        .iter()
        .any(|(field, _)| matches!(*field, "isbn_10" | "isbn_13"));
    for (field, maybe_value) in fields {
        apply_scalar_patch_field(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            field,
            maybe_value,
        )
        .await?;
    }

    for (vocab, patch) in [
        (VocabularyField::Genres, genres),
        (VocabularyField::Moods, moods),
        (VocabularyField::Tags, tags),
    ] {
        if let Some(maybe_names) = patch {
            apply_vocabulary_patch(
                &mut tx,
                manifestation_id,
                current_user.user_id,
                vocab,
                maybe_names,
            )
            .await?;
        }
    }

    if let Some(patch) = contributors {
        apply_contributors_patch(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            patch,
        )
        .await?;
    }

    apply_identifier_patches(
        &mut tx,
        manifestation_id,
        work_id,
        current_user.user_id,
        work_identifiers,
        manifestation_identifiers,
    )
    .await?;

    if touched_isbn {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    // Computed before commit, inside the same transaction as the writes
    // above, so the emitted ETag reflects exactly the representation this
    // PATCH just committed.
    let new_metadata =
        project_metadata(current_user, state, &mut tx, manifestation_id, selection).await?;
    let new_etag = new_metadata.revision.etag()?;

    let (request_id, operations) = undo::finish_save(&mut tx, before).await?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Conditional::Matched(SaveResult {
        snapshot: new_metadata,
        etag: new_etag,
        request_id,
        operations,
    }))
}

/// Journal + apply one scalar field from the manual PATCH surface.
/// A set (`Some`) journals the value and wires the canonical pointer via
/// `apply_version`; a clear (`None`) journals an accountability row and
/// nulls the canonical via `clear_field`.
async fn apply_scalar_patch_field(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    field: &str,
    maybe_value: Option<serde_json::Value>,
) -> Result<(), AppError> {
    if let Some(value) = maybe_value {
        let json = match field {
            "isbn_10" | "isbn_13" => Value::String(normalise_isbn_field(field, &value)?),
            _ => value,
        };
        let version_id = insert_manual_version(tx, manifestation_id, user_id, field, &json).await?;
        apply_version(tx, field, &json, version_id, manifestation_id, work_id).await?;
    } else {
        // Audit-trail row: source='manual', new_value=null,
        // resolved_by = caller. `clear_field` does NOT wire a
        // canonical pointer to this row — by design — so it lives
        // in the journal as an orphan record of the clear action,
        // but `resolved_by` makes the action accountable.
        insert_manual_version(
            tx,
            manifestation_id,
            user_id,
            field,
            &serde_json::Value::Null,
        )
        .await?;
        clear_field(tx, field, manifestation_id, work_id).await?;
    }
    Ok(())
}

/// `identifier_schemes` seeds 10 rows (`backend/migrations/20260810000000_initial_schema.up.sql`);
/// bounds the entries accepted per level in one patch with headroom for
/// growth in the scheme vocabulary.
const MAX_IDENTIFIERS_PER_LEVEL: usize = 64;

/// Apply both identifier maps from the manual PATCH surface, then re-queue
/// enrichment once if anything changed. Resetting the attempt counter and
/// timestamp alongside the status clears any backoff window from a prior
/// failed run; status alone would leave the row uneligible for up to 24
/// hours (see `queue::claim_next`).
///
/// An `in_progress` row is never flipped to `pending` here: the active
/// worker still owns the claim, and making the row eligible again would let
/// a second worker run the same manifestation concurrently. The edit sets
/// `enrichment_rerun_requested` instead, and the worker's completion
/// bookkeeping converts the flag into a fresh eligible row (the active run
/// snapshotted its lookup keys before this edit, so its result may be
/// stale). All CASE arms read the pre-update row state, so the status test
/// is consistent across every column.
async fn apply_identifier_patches(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    work_identifiers: Option<BTreeMap<String, Option<String>>>,
    manifestation_identifiers: Option<BTreeMap<String, Option<String>>>,
) -> Result<(), AppError> {
    let mut touched = false;
    for (level, map) in [
        (IdentifierLevel::Work, work_identifiers),
        (IdentifierLevel::Manifestation, manifestation_identifiers),
    ] {
        for (scheme, maybe_value) in map.into_iter().flatten() {
            apply_identifier_patch(
                tx,
                manifestation_id,
                work_id,
                user_id,
                level,
                &scheme,
                maybe_value,
            )
            .await?;
            touched = true;
        }
    }
    if touched {
        requeue_identifier_enrichment(tx, manifestation_id).await?;
    }
    Ok(())
}

async fn requeue_identifier_enrichment(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_rerun_requested = (enrichment_status = 'in_progress'), \
                 enrichment_status = CASE WHEN enrichment_status = 'in_progress' \
                                          THEN enrichment_status \
                                          ELSE 'pending'::enrichment_status END, \
                 enrichment_attempt_count = CASE WHEN enrichment_status = 'in_progress' \
                                                 THEN enrichment_attempt_count ELSE 0 END, \
                 enrichment_attempted_at = CASE WHEN enrichment_status = 'in_progress' \
                                                THEN enrichment_attempted_at ELSE NULL END, \
                 enrichment_retry_not_before = CASE WHEN enrichment_status = 'in_progress' \
                                                    THEN enrichment_retry_not_before ELSE NULL END, \
                 enrichment_error = CASE WHEN enrichment_status = 'in_progress' \
                                         THEN enrichment_error ELSE NULL END, \
                 enrichment_failures = CASE WHEN enrichment_status = 'in_progress' \
                                            THEN enrichment_failures ELSE '[]'::jsonb END \
             WHERE id = $1",
            manifestation_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Journal + apply one external-identifier slot from the manual PATCH
/// surface. A set (`Some`) parses the raw value into the scheme's canonical
/// form, journals it under `identifiers.<level>.<scheme>`, and upserts the
/// registry slot via `apply_version`; a clear (`None`) journals an
/// accountability row and deletes the slot via `clear_field`. Neither path
/// enqueues a writeback (identifiers are never written back to the file).
async fn apply_identifier_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    level: IdentifierLevel,
    scheme: &str,
    maybe_value: Option<String>,
) -> Result<(), AppError> {
    // Validate the address before journaling so an unknown scheme or a
    // wrong-level address never produces a journal row, on the clear path
    // included.
    external_id::validate_scheme_level(level, scheme).map_err(AppError::Validation)?;
    let field = level.canonical_field(scheme);
    if let Some(raw) = maybe_value {
        let canonical =
            external_id::parse_external_id(level, scheme, &raw).map_err(AppError::Validation)?;
        let json = Value::String(canonical);
        let version_id =
            insert_manual_version(tx, manifestation_id, user_id, &field, &json).await?;
        apply_version(tx, &field, &json, version_id, manifestation_id, work_id).await?;
    } else {
        insert_manual_version(tx, manifestation_id, user_id, &field, &Value::Null).await?;
        clear_field(tx, &field, manifestation_id, work_id).await?;
    }
    Ok(())
}

/// Record an exact manual observation, retaining its submission author on repeat saves.
async fn insert_manual_version(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: &str,
    value: &Value,
) -> Result<Uuid, AppError> {
    let hash = value_hash::value_hash(field, value);
    let id: Uuid = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, \
             match_type, confidence_score, status, submitted_by, resolved_by, resolved_at) \
         VALUES ($1, 'manual', $2, $3, $4, 'manual', 1.0, \
                 'pending'::metadata_review_status, $5, $5, now()) \
         ON CONFLICT (manifestation_id, source, field_name, observation_key) \
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
