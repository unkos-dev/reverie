//! Exact field mementos and browser-session operation outcomes.
//!
//! THREAT: every lookup binds the operation to its live authenticated session, user and visible edition.

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, ETAG};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use tower_sessions::Session;
use uuid::Uuid;

use super::{BookMetadata, is_work_scoped_field, load_book_metadata, require_unlocked_fields};
use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::extract::{ApiJson, ApiPath};
use crate::routes::etag::{if_match_mismatch, parse_if_match};
use crate::state::AppState;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum FieldState {
    Scalar {
        value: Value,
        version: Option<Uuid>,
    },
    Contributors {
        members: Vec<Contributor>,
    },
    Vocabulary {
        members: Vec<Term>,
    },
    Identifier {
        value: Option<String>,
        version: Option<Uuid>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Contributor {
    id: Uuid,
    name: String,
    position: i32,
    version: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Term {
    id: Uuid,
    name: String,
    version: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct SourceMeaning {
    id: Uuid,
    source: String,
    field_name: String,
    value: Value,
    match_type: String,
    submitted_by: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct CapturedField {
    state: FieldState,
    sources: Vec<SourceMeaning>,
}

impl FieldState {
    fn pointers(&self) -> BTreeSet<Uuid> {
        match self {
            Self::Scalar { version, .. } | Self::Identifier { version, .. } => {
                version.iter().copied().collect()
            }
            Self::Contributors { members } => members.iter().filter_map(|m| m.version).collect(),
            Self::Vocabulary { members } => members.iter().filter_map(|m| m.version).collect(),
        }
    }
}

pub(super) struct Capture {
    pub fields: BTreeMap<String, CapturedField>,
    work_stamps: Value,
    edition_stamps: Value,
}

impl Capture {
    fn revision(&self, field: &str) -> i64 {
        let stamps = if is_work_scoped_field(field) {
            &self.work_stamps
        } else {
            &self.edition_stamps
        };
        stamps.get(field).and_then(Value::as_i64).unwrap_or(0)
    }
}

fn internal(error: impl Into<anyhow::Error>) -> AppError {
    AppError::Internal(error.into())
}

pub(super) async fn capture(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    fields: &[String],
) -> Result<Capture, AppError> {
    let owner = sqlx::query!(
        "SELECT to_jsonb(w) AS \"work!\", to_jsonb(m) AS \"edition!\" \
         FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1 AND w.id = $2",
        edition,
        work,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    let contributors = sqlx::query!(
        "SELECT a.id, a.name, wa.role::text AS \"role!\", wa.position, wa.source_version_id \
         FROM work_authors wa JOIN authors a ON a.id = wa.author_id \
         WHERE wa.work_id = $1 ORDER BY wa.role, wa.position, a.id",
        work,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?;
    let terms = sqlx::query!(
        "SELECT 'genres' AS \"field!\", g.id AS \"id!\", g.name AS \"name!\", j.source_version_id \
         FROM manifestation_genres j JOIN genres g ON g.id = j.genre_id WHERE j.manifestation_id = $1 \
         UNION ALL SELECT 'moods', g.id, g.name, j.source_version_id \
         FROM manifestation_moods j JOIN moods g ON g.id = j.mood_id WHERE j.manifestation_id = $1 \
         UNION ALL SELECT 'tags', g.id, g.name, j.source_version_id \
         FROM manifestation_tags j JOIN tags g ON g.id = j.tag_id WHERE j.manifestation_id = $1 \
         ORDER BY 1, 2", edition,
    ).fetch_all(&mut **tx).await.map_err(internal)?;
    let identifiers = sqlx::query!(
        "SELECT 'identifiers.work.' || scheme AS \"field!\", external_id AS \"value!\", source_version_id \
         FROM work_external_identifiers WHERE work_id = $1 \
         UNION ALL SELECT 'identifiers.manifestation.' || scheme, external_id, source_version_id \
         FROM manifestation_external_identifiers WHERE manifestation_id = $2", work, edition,
    ).fetch_all(&mut **tx).await.map_err(internal)?;
    let mut states = BTreeMap::new();
    for field in fields {
        let state = if let Some(role) = field.strip_prefix("contributors.") {
            FieldState::Contributors {
                members: contributors
                    .iter()
                    .filter(|r| r.role == role)
                    .map(|r| Contributor {
                        id: r.id,
                        name: r.name.clone(),
                        position: r.position,
                        version: r.source_version_id,
                    })
                    .collect(),
            }
        } else if matches!(field.as_str(), "genres" | "moods" | "tags") {
            FieldState::Vocabulary {
                members: terms
                    .iter()
                    .filter(|r| r.field == *field)
                    .map(|r| Term {
                        id: r.id,
                        name: r.name.clone(),
                        version: r.source_version_id,
                    })
                    .collect(),
            }
        } else if field.starts_with("identifiers.") {
            let slot = identifiers.iter().find(|r| r.field == *field);
            FieldState::Identifier {
                value: slot.map(|r| r.value.clone()),
                version: slot.and_then(|r| r.source_version_id),
            }
        } else {
            let owner = if is_work_scoped_field(field) {
                &owner.work
            } else {
                &owner.edition
            };
            let value = owner
                .get(field)
                .ok_or_else(|| internal(anyhow::anyhow!("unsupported captured metadata field")))?
                .clone();
            let version = serde_json::from_value(owner[format!("{field}_version_id")].clone())
                .map_err(internal)?;
            FieldState::Scalar { value, version }
        };
        states.insert(field.clone(), state);
    }
    let fields = attach_sources(tx, states).await?;
    Ok(Capture {
        fields,
        work_stamps: owner.work["metadata_field_revisions"].clone(),
        edition_stamps: owner.edition["metadata_field_revisions"].clone(),
    })
}

async fn attach_sources(
    tx: &mut Transaction<'_, Postgres>,
    states: BTreeMap<String, FieldState>,
) -> Result<BTreeMap<String, CapturedField>, AppError> {
    let pointers: Vec<Uuid> = states
        .values()
        .flat_map(FieldState::pointers)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let sources = source_meanings(tx, &pointers).await?;
    let fields = states
        .into_iter()
        .map(|(field, state)| {
            let ids = state.pointers();
            let sources = sources
                .iter()
                .filter(|s| ids.contains(&s.id))
                .cloned()
                .collect();
            (field, CapturedField { state, sources })
        })
        .collect();
    Ok(fields)
}

async fn source_meanings(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<SourceMeaning>, AppError> {
    Ok(sqlx::query!(
        "SELECT id, source, field_name, new_value AS \"value!\", match_type, submitted_by \
         FROM metadata_versions WHERE id = ANY($1) ORDER BY id",
        ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?
    .into_iter()
    .map(|r| SourceMeaning {
        id: r.id,
        source: r.source,
        field_name: r.field_name,
        value: r.value,
        match_type: r.match_type,
        submitted_by: r.submitted_by,
    })
    .collect())
}

/// One saved field operation, bound to the issuing browser session and fixed expiry.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct UndoOperation {
    /// Opaque restoration handle; send it in an operation request body.
    operation_id: Uuid,
    /// Canonical field key selected by this operation.
    field_name: String,
    /// Original deadline, unchanged by resolution or lock refusal.
    expires_at: DateTime<Utc>,
    /// Current server decision for this operation.
    status: OperationStatus,
}

/// Server-confirmed availability or committed consumption of a saved operation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
enum OperationStatus {
    Eligible,
    Locked,
    Unavailable,
    Consumed,
}

pub(super) struct BrowserSave {
    session: String,
    user: Uuid,
    request: Uuid,
}

pub(super) struct PreparedSave {
    browser: BrowserSave,
    edition: Uuid,
    work: Uuid,
    before: Capture,
}

pub(super) fn browser_save(
    user: &CurrentUser,
    session: &Session,
    request: Option<Uuid>,
) -> Result<Option<BrowserSave>, AppError> {
    match user.require_browser_session() {
        Ok(()) => Ok(Some(BrowserSave {
            session: session.id().ok_or(AppError::UndoUnavailable)?.to_string(),
            user: user.user_id,
            request: request.unwrap_or_else(Uuid::new_v4),
        })),
        Err(AppError::Forbidden) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn require_save_session(
    tx: &mut Transaction<'_, Postgres>,
    browser: Option<&BrowserSave>,
) -> Result<(), AppError> {
    if let Some(browser) = browser {
        require_live_session(tx, &browser.session).await?;
    }
    Ok(())
}

pub(super) async fn begin_save(
    tx: &mut Transaction<'_, Postgres>,
    browser: Option<BrowserSave>,
    edition: Uuid,
    work: Uuid,
    fields: &[String],
) -> Result<Option<PreparedSave>, AppError> {
    let Some(browser) = browser else {
        return Ok(None);
    };
    require_new_request(tx, &browser.session, browser.request).await?;
    let before = capture(tx, edition, work, fields).await?;
    Ok(Some(PreparedSave {
        browser,
        edition,
        work,
        before,
    }))
}

pub(super) async fn finish_save(
    tx: &mut Transaction<'_, Postgres>,
    save: Option<PreparedSave>,
) -> Result<(Option<Uuid>, Vec<UndoOperation>), AppError> {
    let Some(save) = save else {
        return Ok((None, Vec::new()));
    };
    let fields: Vec<String> = save.before.fields.keys().cloned().collect();
    let after = capture(tx, save.edition, save.work, &fields).await?;
    let request = save.browser.request;
    let operations = capture_save(tx, save, after).await?;
    Ok((Some(request), operations))
}

async fn capture_save(
    tx: &mut Transaction<'_, Postgres>,
    save: PreparedSave,
    after: Capture,
) -> Result<Vec<UndoOperation>, AppError> {
    let PreparedSave {
        browser,
        edition,
        work,
        before,
    } = save;
    let BrowserSave {
        session,
        user,
        request,
    } = browser;
    let expires =
        sqlx::query_scalar!("SELECT clock_timestamp() + interval '1 hour' AS \"expires!\"")
            .fetch_one(&mut **tx)
            .await
            .map_err(internal)?;
    let mut result = Vec::new();
    for (field, before) in before.fields {
        sqlx::query!(
            "UPDATE metadata_undo_operations SET superseded_at = clock_timestamp() \
             WHERE user_id = $1 AND session_id = $2 AND manifestation_id = $3 AND field_name = $4 \
               AND consumed_at IS NULL AND superseded_at IS NULL",
            user,
            session,
            edition,
            field,
        )
        .execute(&mut **tx)
        .await
        .map_err(internal)?;
        let after_field = after
            .fields
            .get(&field)
            .ok_or_else(|| internal(anyhow::anyhow!("missing saved field memento")))?;
        if &before == after_field || !valid_isbn_before(&field, &before.state) {
            continue;
        }
        let id = Uuid::new_v4();
        let before_json = serde_json::to_value(&before).map_err(internal)?;
        let after_json = serde_json::to_value(after_field).map_err(internal)?;
        let revision = after.revision(&field);
        sqlx::query!(
            "INSERT INTO metadata_undo_operations \
             (id, user_id, session_id, manifestation_id, work_id, request_id, field_name, before_state, after_state, after_revision, expires_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            id, user, session, edition, work, request, field, before_json, after_json, revision, expires,
        ).execute(&mut **tx).await.map_err(internal)?;
        result.push(UndoOperation {
            operation_id: id,
            field_name: field,
            expires_at: expires,
            status: OperationStatus::Eligible,
        });
    }
    Ok(result)
}

fn valid_isbn_before(field: &str, state: &FieldState) -> bool {
    match (field, state) {
        ("isbn_10" | "isbn_13", FieldState::Scalar { value, .. }) if !value.is_null() => {
            super::normalise_isbn_field(field, value).is_ok()
        }
        _ => true,
    }
}

pub(super) async fn require_live_session(
    tx: &mut Transaction<'_, Postgres>,
    session: &str,
) -> Result<(), AppError> {
    let live = sqlx::query_scalar!(
        "SELECT id FROM tower_sessions.session WHERE id = $1 AND expiry_date > clock_timestamp() FOR SHARE", session,
    ).fetch_optional(&mut **tx).await.map_err(internal)?;
    if live.is_none() {
        return Err(AppError::UndoUnavailable);
    }
    Ok(())
}

pub(super) async fn require_new_request(
    tx: &mut Transaction<'_, Postgres>,
    session: &str,
    request: Uuid,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM metadata_undo_operations WHERE session_id = $1 AND request_id = $2) AS \"exists!\"",
        session, request,
    ).fetch_one(&mut **tx).await.map_err(internal)?;
    if exists {
        return Err(AppError::Validation(
            "save request identifier already recorded".into(),
        ));
    }
    Ok(())
}

struct Targets {
    sources: BTreeMap<Uuid, SourceMeaning>,
    authors: BTreeMap<Uuid, String>,
    terms: BTreeMap<(String, Uuid), String>,
}

impl Targets {
    fn contains(&self, field: &str, before: &CapturedField) -> bool {
        let ids = before.state.pointers();
        if before.sources.len() != ids.len()
            || before
                .sources
                .iter()
                .any(|source| self.sources.get(&source.id) != Some(source))
        {
            return false;
        }
        match &before.state {
            FieldState::Contributors { members } => members
                .iter()
                .all(|m| self.authors.get(&m.id) == Some(&m.name)),
            FieldState::Vocabulary { members } => members
                .iter()
                .all(|m| self.terms.get(&(field.to_owned(), m.id)) == Some(&m.name)),
            _ => true,
        }
    }
}

async fn load_targets(
    tx: &mut Transaction<'_, Postgres>,
    before: &[&CapturedField],
) -> Result<Targets, AppError> {
    let sources: Vec<Uuid> = before
        .iter()
        .flat_map(|f| f.state.pointers())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sqlx::query!(
        "SELECT id FROM metadata_versions WHERE id = ANY($1) ORDER BY id FOR SHARE",
        &sources
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?;
    let sources = source_meanings(tx, &sources)
        .await?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    let ids: Vec<Uuid> = before
        .iter()
        .flat_map(|f| match &f.state {
            FieldState::Contributors { members } => {
                members.iter().map(|m| m.id).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect();
    let authors = sqlx::query!(
        "SELECT id, name FROM authors WHERE id = ANY($1) ORDER BY id FOR SHARE",
        &ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?
    .into_iter()
    .map(|r| (r.id, r.name))
    .collect();
    let ids: Vec<Uuid> = before
        .iter()
        .flat_map(|f| match &f.state {
            FieldState::Vocabulary { members } => members.iter().map(|m| m.id).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    let mut terms = BTreeMap::new();
    for row in sqlx::query!(
        "SELECT id, name FROM genres WHERE id = ANY($1) ORDER BY id FOR SHARE",
        &ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?
    {
        terms.insert(("genres".to_owned(), row.id), row.name);
    }
    for row in sqlx::query!(
        "SELECT id, name FROM moods WHERE id = ANY($1) ORDER BY id FOR SHARE",
        &ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?
    {
        terms.insert(("moods".to_owned(), row.id), row.name);
    }
    for row in sqlx::query!(
        "SELECT id, name FROM tags WHERE id = ANY($1) ORDER BY id FOR SHARE",
        &ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?
    {
        terms.insert(("tags".to_owned(), row.id), row.name);
    }
    Ok(Targets {
        sources,
        authors,
        terms,
    })
}

fn restoration_error(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23503")
    {
        AppError::UndoUnavailable
    } else {
        internal(error)
    }
}

fn restored_value<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone()).map_err(|_| AppError::UndoUnavailable)
}

async fn restore_work_scalar(
    tx: &mut Transaction<'_, Postgres>,
    work: Uuid,
    field: &str,
    value: &Value,
    version: Option<Uuid>,
) -> Result<(), AppError> {
    match field {
        "title" => {
            let value: String = restored_value(value)?;
            sqlx::query!("UPDATE works SET title = $1, title_version_id = $2, sort_title = lower($1) WHERE id = $3", value, version, work)
                .execute(&mut **tx).await.map_err(restoration_error)?;
        }
        "subtitle" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE works SET subtitle = $1, subtitle_version_id = $2 WHERE id = $3",
                value,
                version,
                work
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "description" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
                value,
                version,
                work
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "language" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE works SET language = $1, language_version_id = $2 WHERE id = $3",
                value,
                version,
                work
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        _ => return Err(AppError::UndoUnavailable),
    }
    Ok(())
}

async fn restore_scalar(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    field: &str,
    value: &Value,
    version: Option<Uuid>,
) -> Result<(), AppError> {
    if is_work_scoped_field(field) {
        return restore_work_scalar(tx, work, field, value, version).await;
    }
    match field {
        "publisher" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "pub_date" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "isbn_10" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "isbn_13" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!(
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "pages" => {
            let value: Option<i32> = restored_value(value)?;
            sqlx::query!(
                "UPDATE manifestations SET pages = $1, pages_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
        }
        "content_rating" => {
            let value: Option<String> = restored_value(value)?;
            sqlx::query!("UPDATE manifestations SET content_rating = $1::text::content_rating, content_rating_version_id = $2 WHERE id = $3", value, version, edition)
                .execute(&mut **tx).await.map_err(restoration_error)?;
        }
        _ => return Err(AppError::UndoUnavailable),
    }
    Ok(())
}

async fn restore_identifier(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    field: &str,
    value: Option<&str>,
    version: Option<Uuid>,
) -> Result<(), AppError> {
    let (level, scheme) = super::parse_identifier_field(field)?;
    let present = sqlx::query_scalar!("SELECT id FROM identifier_schemes WHERE id = $1", scheme)
        .fetch_optional(&mut **tx)
        .await
        .map_err(restoration_error)?;
    if present.is_none() {
        return Err(AppError::UndoUnavailable);
    }
    match (level, value) {
        (super::IdentifierLevel::Work, Some(value)) => {
            crate::models::external_identifier::upsert_work_identifier(
                &mut **tx, work, scheme, value, version,
            )
            .await
            .map_err(restoration_error)?;
        }
        (super::IdentifierLevel::Manifestation, Some(value)) => {
            crate::models::external_identifier::upsert_manifestation_identifier(
                &mut **tx, edition, scheme, value, version,
            )
            .await
            .map_err(restoration_error)?;
        }
        (super::IdentifierLevel::Work, None) => {
            crate::models::external_identifier::delete_work_identifier(&mut **tx, work, scheme)
                .await
                .map_err(restoration_error)?;
        }
        (super::IdentifierLevel::Manifestation, None) => {
            crate::models::external_identifier::delete_manifestation_identifier(
                &mut **tx, edition, scheme,
            )
            .await
            .map_err(restoration_error)?;
        }
    }
    Ok(())
}

async fn restore(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    field: &str,
    before: &CapturedField,
) -> Result<(), AppError> {
    if !load_targets(tx, &[before]).await?.contains(field, before) {
        return Err(AppError::UndoUnavailable);
    }
    if !valid_isbn_before(field, &before.state) {
        return Err(AppError::UndoUnavailable);
    }
    match &before.state {
        FieldState::Scalar { value, version } => {
            restore_scalar(tx, edition, work, field, value, *version).await?;
        }
        FieldState::Contributors { members } => {
            let role = field
                .strip_prefix("contributors.")
                .ok_or(AppError::UndoUnavailable)?;
            super::delete_role_rows(tx, work, role).await?;
            let ids: Vec<Uuid> = members.iter().map(|m| m.id).collect();
            let positions: Vec<i32> = members.iter().map(|m| m.position).collect();
            let versions: Vec<Option<Uuid>> = members.iter().map(|m| m.version).collect();
            sqlx::query!(
                "INSERT INTO work_authors (work_id, author_id, role, position, source_version_id) \
                 SELECT $1, id, ($2::text)::author_role, position, version \
                 FROM UNNEST($3::uuid[], $4::int[], $5::uuid[]) AS m(id, position, version)",
                work,
                role,
                &ids,
                &positions,
                &versions as &[Option<Uuid>],
            )
            .execute(&mut **tx)
            .await
            .map_err(restoration_error)?;
            if role == "author" {
                crate::models::work::refresh_first_author_sort(tx, work)
                    .await
                    .map_err(restoration_error)?;
            }
        }
        FieldState::Vocabulary { members } => {
            let vocab = match field {
                "genres" => super::VocabularyField::Genres,
                "moods" => super::VocabularyField::Moods,
                "tags" => super::VocabularyField::Tags,
                _ => return Err(AppError::UndoUnavailable),
            };
            super::delete_vocabulary_rows(tx, edition, vocab).await?;
            let ids: Vec<Uuid> = members.iter().map(|m| m.id).collect();
            let versions: Vec<Option<Uuid>> = members.iter().map(|m| m.version).collect();
            match field {
                "genres" => {
                    sqlx::query!("INSERT INTO manifestation_genres (manifestation_id, genre_id, source_version_id) SELECT $1, id, version FROM UNNEST($2::uuid[], $3::uuid[]) AS m(id, version)", edition, &ids, &versions as &[Option<Uuid>]).execute(&mut **tx).await.map_err(restoration_error)?;
                }
                "moods" => {
                    sqlx::query!("INSERT INTO manifestation_moods (manifestation_id, mood_id, source_version_id) SELECT $1, id, version FROM UNNEST($2::uuid[], $3::uuid[]) AS m(id, version)", edition, &ids, &versions as &[Option<Uuid>]).execute(&mut **tx).await.map_err(restoration_error)?;
                }
                "tags" => {
                    sqlx::query!("INSERT INTO manifestation_tags (manifestation_id, tag_id, source_version_id) SELECT $1, id, version FROM UNNEST($2::uuid[], $3::uuid[]) AS m(id, version)", edition, &ids, &versions as &[Option<Uuid>]).execute(&mut **tx).await.map_err(restoration_error)?;
                }
                _ => return Err(AppError::UndoUnavailable),
            }
        }
        FieldState::Identifier { value, version } => {
            restore_identifier(tx, edition, work, field, value.as_deref(), *version).await?;
        }
    }
    if !field.starts_with("identifiers.") {
        super::enqueue_writeback(tx, edition, field).await?;
    }
    if matches!(field, "isbn_10" | "isbn_13") {
        crate::models::work::rematch_on_isbn_change(tx, edition)
            .await
            .map_err(restoration_error)?;
    }
    Ok(())
}

pub(super) async fn lock_parents(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
) -> Result<Uuid, AppError> {
    let work = sqlx::query_scalar!(
        "SELECT work_id FROM manifestations WHERE id = $1 FOR UPDATE",
        edition
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?
    .ok_or(AppError::NotFound)?;
    sqlx::query_scalar!("SELECT id FROM works WHERE id = $1 FOR UPDATE", work)
        .fetch_optional(&mut **tx)
        .await
        .map_err(internal)?
        .ok_or(AppError::NotFound)?;
    Ok(work)
}

/// Select one opaque saved-field operation.
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct UndoRequest {
    /// Handle returned by a browser save or operation resolution.
    #[schema(max_length = 45)]
    operation_id: Uuid,
}

/// Resolve a save or a single operation without replaying a write.
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ResolveRequest {
    /// Client UUID supplied with the original save; mutually exclusive with `operation_id`.
    #[schema(max_length = 45)]
    request_id: Option<Uuid>,
    /// Saved-field handle; mutually exclusive with `request_id`.
    #[schema(max_length = 45)]
    operation_id: Option<Uuid>,
}

/// Confirmed current metadata and session-owned operation evidence.
#[derive(Serialize, utoipa::ToSchema)]
struct OperationResponse {
    /// Complete canonical metadata paired with the response `ETag`.
    values: BookMetadata,
    /// Whether the selected save or restoration has committed evidence. Absence remains unknown.
    outcome: OperationOutcome,
    /// Bounded field operations belonging to the selected save or operation.
    #[schema(max_items = 144)]
    operations: Vec<UndoOperation>,
}

/// Evidence of a save or restoration, independent of apparent value equality.
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
enum OperationOutcome {
    Saved,
    Restored,
    Unknown,
}

fn operation_response(
    values: BookMetadata,
    etag: HeaderValue,
    outcome: OperationOutcome,
    operations: Vec<UndoOperation>,
) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        StatusCode::OK,
        headers,
        axum::Json(OperationResponse {
            values,
            outcome,
            operations,
        }),
    )
        .into_response()
}

fn session_id(user: &CurrentUser, session: &Session) -> Result<String, AppError> {
    user.require_browser_session()?;
    session
        .id()
        .map(|id| id.to_string())
        .ok_or(AppError::UndoUnavailable)
}

#[utoipa::path(
    post, path = "/api/v1/books/{id}/metadata/undo", tag = "metadata",
    summary = "Undo one saved metadata field",
    description = "Restores the exact captured field state once for its issuing browser session. Requires current aggregate If-Match and an eligible, unlocked field. Confidence changes retain field-identity invalidation. No write is replayed automatically.",
    security(("session_cookie" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id"), ("If-Match" = String, Header, max_length = 24, description = "Current quoted 128-bit metadata entity-tag returned by a matched response. Other well-formed tags return 412")),
    request_body = UndoRequest,
    responses(
        (status = 200, description = "Exact field restoration committed", body = OperationResponse, headers(("ETag" = String, description = "Metadata validator matching the returned values"))),
        (status = 400, description = "Malformed body, path or If-Match", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session and adult write scope required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Edition missing or hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Field locked or recorded after-state conflicts; operation not consumed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "Aggregate metadata validator is stale", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("ETag" = String, description = "Current metadata validator"))),
        (status = 422, description = "Operation unavailable", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match or CSRF token missing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json")
    )
)]
pub(super) async fn undo_metadata(
    user: CurrentUser,
    State(state): State<AppState>,
    session: Session,
    ApiPath(edition): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<UndoRequest>,
) -> Result<Response, AppError> {
    user.require_scope(Scope::Write)?;
    user.require_not_child()?;
    let session = session_id(&user, &session)?;
    let supplied = parse_if_match(&headers)?.ok_or(AppError::IfMatchRequired)?;
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, user.user_id)
        .await
        .map_err(internal)?;
    require_live_session(&mut tx, &session).await?;
    let work = lock_parents(&mut tx, edition).await?;
    let current = load_book_metadata(&mut tx, edition)
        .await?
        .ok_or(AppError::NotFound)?;
    let tag = current.revision.etag()?;
    if !supplied.matches(&tag) {
        return Ok(if_match_mismatch(&tag));
    }
    let op = sqlx::query!(
        "SELECT field_name, before_state, after_state, after_revision, expires_at \
         FROM metadata_undo_operations WHERE id = $1 AND user_id = $2 AND session_id = $3 \
           AND manifestation_id = $4 AND work_id = $5 AND expires_at > clock_timestamp() \
           AND consumed_at IS NULL AND superseded_at IS NULL FOR UPDATE",
        request.operation_id,
        user.user_id,
        session,
        edition,
        work,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?
    .ok_or(AppError::UndoUnavailable)?;
    let fields = vec![op.field_name.clone()];
    let now = capture(&mut tx, edition, work, &fields).await?;
    if now.revision(&op.field_name) != op.after_revision {
        return Err(AppError::UndoUnavailable);
    }
    require_unlocked_fields(&mut tx, edition, user.user_id, &fields).await?;
    let before: CapturedField = serde_json::from_value(op.before_state).map_err(internal)?;
    let after: CapturedField = serde_json::from_value(op.after_state).map_err(internal)?;
    if now.fields.get(&op.field_name) != Some(&after) {
        tracing::warn!("metadata Undo refused a recorded-state mismatch");
        return Err(AppError::UndoConflict);
    }
    restore(&mut tx, edition, work, &op.field_name, &before).await?;
    let consumed = sqlx::query!(
        "UPDATE metadata_undo_operations SET consumed_at = clock_timestamp() \
         WHERE id = $1 AND expires_at > clock_timestamp() RETURNING id",
        request.operation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal)?;
    if consumed.is_none() {
        return Err(AppError::UndoUnavailable);
    }
    let confirmed = load_book_metadata(&mut tx, edition)
        .await?
        .ok_or(AppError::NotFound)?;
    let tag = confirmed.revision.etag()?;
    tx.commit().await.map_err(internal)?;
    Ok(operation_response(
        confirmed.values,
        tag,
        OperationOutcome::Restored,
        vec![UndoOperation {
            operation_id: request.operation_id,
            field_name: op.field_name,
            expires_at: op.expires_at,
            status: OperationStatus::Consumed,
        }],
    ))
}

#[utoipa::path(
    post, path = "/api/v1/books/{id}/metadata/operations/resolve", tag = "metadata",
    summary = "Resolve a metadata operation outcome",
    description = "Returns current paired metadata and session-owned committed evidence for one save request or field operation. An absent receipt means unknown, never proof of rollback. This read does not restore or replay a write.",
    security(("session_cookie" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Current metadata and operation evidence", body = OperationResponse, headers(("ETag" = String, description = "Metadata validator matching the returned values"))),
        (status = 400, description = "Malformed body or path", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session and adult write scope required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Edition missing or hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Select exactly one request or operation identifier", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "CSRF token missing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json")
    )
)]
pub(super) async fn resolve_metadata(
    user: CurrentUser,
    State(state): State<AppState>,
    session: Session,
    ApiPath(edition): ApiPath<Uuid>,
    ApiJson(request): ApiJson<ResolveRequest>,
) -> Result<Response, AppError> {
    user.require_scope(Scope::Write)?;
    user.require_not_child()?;
    let session = session_id(&user, &session)?;
    if request.request_id.is_some() == request.operation_id.is_some() {
        return Err(AppError::Validation(
            "select one operation or save request".into(),
        ));
    }
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, user.user_id)
        .await
        .map_err(internal)?;
    require_live_session(&mut tx, &session).await?;
    let work = lock_parents(&mut tx, edition).await?;
    let rows = sqlx::query!(
        "SELECT id, field_name, after_revision, before_state, after_state, expires_at, consumed_at, superseded_at, work_id \
         FROM metadata_undo_operations WHERE user_id = $1 AND session_id = $2 AND manifestation_id = $3 \
           AND (($4::uuid IS NOT NULL AND request_id = $4) OR ($5::uuid IS NOT NULL AND id = $5)) \
           AND expires_at > clock_timestamp() ORDER BY field_name LIMIT 144 FOR SHARE",
        user.user_id, session, edition, request.request_id, request.operation_id,
    ).fetch_all(&mut *tx).await.map_err(internal)?;
    let fields: Vec<String> = rows.iter().map(|r| r.field_name.clone()).collect();
    let now = capture(&mut tx, edition, work, &fields).await?;
    let before: BTreeMap<String, CapturedField> = rows
        .iter()
        .map(|row| {
            serde_json::from_value(row.before_state.clone())
                .map(|state| (row.field_name.clone(), state))
                .map_err(internal)
        })
        .collect::<Result<_, _>>()?;
    let targets = load_targets(&mut tx, &before.values().collect::<Vec<_>>()).await?;
    let locked: BTreeSet<String> =
        match require_unlocked_fields(&mut tx, edition, user.user_id, &fields).await {
            Ok(()) => BTreeSet::new(),
            Err(AppError::FieldLocked(fields)) => fields.into_iter().collect(),
            Err(error) => return Err(error),
        };
    let decision_time = sqlx::query_scalar!("SELECT clock_timestamp() AS \"now!\"")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    let mut operations = Vec::new();
    let mut restored = false;
    for row in rows
        .into_iter()
        .filter(|row| row.expires_at > decision_time)
    {
        let status = if row.consumed_at.is_some() {
            restored = true;
            OperationStatus::Consumed
        } else if row.superseded_at.is_some()
            || row.work_id != work
            || now.revision(&row.field_name) != row.after_revision
        {
            OperationStatus::Unavailable
        } else {
            let after: CapturedField = serde_json::from_value(row.after_state).map_err(internal)?;
            if now.fields.get(&row.field_name) != Some(&after)
                || before
                    .get(&row.field_name)
                    .is_none_or(|before| !targets.contains(&row.field_name, before))
            {
                OperationStatus::Unavailable
            } else if locked.contains(&row.field_name) {
                OperationStatus::Locked
            } else {
                OperationStatus::Eligible
            }
        };
        operations.push(UndoOperation {
            operation_id: row.id,
            field_name: row.field_name,
            expires_at: row.expires_at,
            status,
        });
    }
    let outcome = if operations.is_empty() {
        OperationOutcome::Unknown
    } else if restored && request.operation_id.is_some() {
        OperationOutcome::Restored
    } else {
        OperationOutcome::Saved
    };
    let current = load_book_metadata(&mut tx, edition)
        .await?
        .ok_or(AppError::NotFound)?;
    let tag = current.revision.etag()?;
    tx.commit().await.map_err(internal)?;
    Ok(operation_response(current.values, tag, outcome, operations))
}
