//! Exact field mementos and browser-session operation outcomes.
//!
//! THREAT: every lookup binds the operation to its live authenticated session, user and visible edition.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, ETAG};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
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

pub(super) const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_CAPTURE_ENTRIES: usize = 10_000;
const OPERATION_TIME: Duration = Duration::from_secs(5);

pub(super) struct ResourceBudget {
    bytes: usize,
    entries: usize,
    max_bytes: usize,
    max_entries: usize,
    started: Instant,
}

struct BoundedCounter {
    used: usize,
    remaining: usize,
    exceeded: bool,
}

impl Write for BoundedCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("Undo resource limit exceeded"));
        }
        self.used += bytes.len();
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ResourceBudget {
    pub(super) fn capture() -> Self {
        Self {
            bytes: 0,
            entries: 0,
            max_bytes: MAX_CAPTURE_BYTES,
            max_entries: MAX_CAPTURE_ENTRIES,
            started: Instant::now(),
        }
    }

    fn restoration() -> Self {
        Self {
            max_bytes: 2 * MAX_CAPTURE_BYTES,
            max_entries: 2 * MAX_CAPTURE_ENTRIES,
            ..Self::capture()
        }
    }

    fn refused() -> AppError {
        AppError::Validation(
            "This change is too large or took too long to prepare for Undo.".into(),
        )
    }

    fn check(&self) -> Result<(), AppError> {
        if self.started.elapsed() >= OPERATION_TIME {
            return Err(Self::refused());
        }
        Ok(())
    }

    fn reserve(&mut self, bytes: usize, entries: usize) -> Result<(), AppError> {
        self.check()?;
        if bytes > self.max_bytes - self.bytes || entries > self.max_entries - self.entries {
            return Err(Self::refused());
        }
        self.bytes += bytes;
        self.entries += entries;
        Ok(())
    }

    fn admit(&mut self, value: &impl Serialize, entries: usize) -> Result<(), AppError> {
        self.check()?;
        let mut counter = BoundedCounter {
            used: 0,
            remaining: self.max_bytes - self.bytes,
            exceeded: false,
        };
        if let Err(error) = serde_json::to_writer(&mut counter, value) {
            return Err(if counter.exceeded {
                Self::refused()
            } else {
                internal(error)
            });
        }
        self.reserve(counter.used + 1, entries)
    }

    fn limits(&self) -> Result<(i32, i64), AppError> {
        self.check()?;
        Ok((
            i32::try_from(self.max_bytes - self.bytes).map_err(internal)?,
            i64::try_from(self.max_entries - self.entries + 1).map_err(internal)?,
        ))
    }

    async fn configure(&mut self, tx: &mut Transaction<'_, Postgres>) -> Result<(), AppError> {
        let limit = i32::try_from(OPERATION_TIME.as_millis()).map_err(internal)?;
        let current = sqlx::query!(
            "SELECT s.setting::int AS \"statement!\", t.setting::int AS \"transaction!\" \
             FROM pg_settings s CROSS JOIN pg_settings t \
             WHERE s.name = 'statement_timeout' AND t.name = 'transaction_timeout'",
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(internal)?;
        let statement = if current.statement > 0 {
            current.statement.min(limit)
        } else {
            limit
        };
        let transaction = if current.transaction > 0 {
            current.transaction.min(limit)
        } else {
            limit
        };
        if current.transaction > limit {
            // PostgreSQL requires restarting an active transaction timer to shorten it.
            sqlx::query!("SELECT set_config('transaction_timeout', '0', true)")
                .fetch_one(&mut **tx)
                .await
                .map_err(internal)?;
        }
        sqlx::query!(
            "SELECT set_config('statement_timeout', $1, true) AS statement_limit, \
             set_config('transaction_timeout', $2, true) AS transaction_limit",
            statement.to_string(),
            transaction.to_string(),
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(internal)?;
        self.started = Instant::now();
        Ok(())
    }
}

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
    budget: &mut ResourceBudget,
) -> Result<Capture, AppError> {
    let mut result = capture_stamps(tx, edition, work).await?;
    let fields = capturable_fields(tx, edition, fields).await?;
    let mut states = BTreeMap::new();
    for field in &fields {
        budget.reserve(128 + field.len(), 1)?;
        let state = if field.starts_with("contributors.") {
            FieldState::Contributors {
                members: Vec::new(),
            }
        } else if matches!(field.as_str(), "genres" | "moods" | "tags") {
            FieldState::Vocabulary {
                members: Vec::new(),
            }
        } else if field.starts_with("identifiers.") {
            FieldState::Identifier {
                value: None,
                version: None,
            }
        } else {
            FieldState::Scalar {
                value: Value::Null,
                version: None,
            }
        };
        states.insert(field.clone(), state);
    }
    capture_scalars(tx, edition, work, &fields, &mut states, budget).await?;
    capture_contributors(tx, work, &fields, &mut states, budget).await?;
    capture_terms(tx, edition, &fields, &mut states, budget).await?;
    capture_identifiers(tx, edition, work, &fields, &mut states, budget).await?;
    result.fields = attach_sources(tx, states, budget).await?;
    Ok(result)
}

async fn capture_stamps(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
) -> Result<Capture, AppError> {
    let row = sqlx::query!(
        "SELECT w.metadata_field_revisions AS work, m.metadata_field_revisions AS edition \
         FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1 AND w.id = $2",
        edition,
        work,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(internal)?;
    Ok(Capture {
        fields: BTreeMap::new(),
        work_stamps: row.work,
        edition_stamps: row.edition,
    })
}

async fn capturable_fields(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    fields: &[String],
) -> Result<Vec<String>, AppError> {
    let row = sqlx::query!(
        "SELECT CASE WHEN 'isbn_10' = ANY($2) THEN left(translate(isbn_10, '- ', ''), 14) END AS isbn_10, \
         CASE WHEN 'isbn_13' = ANY($2) THEN left(translate(isbn_13, '- ', ''), 14) END AS isbn_13 \
         FROM manifestations WHERE id = $1", edition, fields,
    ).fetch_one(&mut **tx).await.map_err(internal)?;
    let mut result = Vec::new();
    for field in fields {
        let candidate = match field.as_str() {
            "isbn_10" => row.isbn_10.as_ref(),
            "isbn_13" => row.isbn_13.as_ref(),
            _ => None,
        };
        if candidate.is_some_and(|value| {
            super::normalise_isbn_field(field, &Value::String(value.clone())).is_err()
        }) {
            continue;
        }
        result.push(field.clone());
    }
    Ok(result)
}

async fn capture_scalars(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    fields: &[String],
    states: &mut BTreeMap<String, FieldState>,
    budget: &mut ResourceBudget,
) -> Result<(), AppError> {
    let (bytes, _) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT slot.field AS \"field!\", slot.version, \
         CASE WHEN slot.raw IS NULL THEN 'null'::jsonb \
              WHEN octet_length(slot.raw) > $4 THEN NULL \
              WHEN octet_length(to_jsonb(slot.raw)::text) > $4 THEN NULL \
              WHEN slot.field = 'pages' THEN to_jsonb(slot.raw::int) ELSE to_jsonb(slot.raw) END AS value \
         FROM manifestations m JOIN works w ON w.id = m.work_id \
         CROSS JOIN LATERAL (VALUES \
           ('title', CASE WHEN 'title' = ANY($3) THEN w.title END, w.title_version_id), \
           ('subtitle', CASE WHEN 'subtitle' = ANY($3) THEN w.subtitle END, w.subtitle_version_id), \
           ('description', CASE WHEN 'description' = ANY($3) THEN w.description END, w.description_version_id), \
           ('language', CASE WHEN 'language' = ANY($3) THEN w.language END, w.language_version_id), \
           ('publisher', CASE WHEN 'publisher' = ANY($3) THEN m.publisher END, m.publisher_version_id), \
           ('pub_date', CASE WHEN 'pub_date' = ANY($3) THEN m.pub_date END, m.pub_date_version_id), \
           ('isbn_10', CASE WHEN 'isbn_10' = ANY($3) THEN m.isbn_10 END, m.isbn_10_version_id), \
           ('isbn_13', CASE WHEN 'isbn_13' = ANY($3) THEN m.isbn_13 END, m.isbn_13_version_id), \
           ('pages', CASE WHEN 'pages' = ANY($3) THEN m.pages::text END, m.pages_version_id), \
           ('content_rating', CASE WHEN 'content_rating' = ANY($3) THEN m.content_rating::text END, m.content_rating_version_id) \
         ) AS slot(field, raw, version) WHERE m.id = $1 AND w.id = $2 AND slot.field = ANY($3)",
        edition, work, fields, bytes,
    ).fetch(&mut **tx);
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let state = FieldState::Scalar {
            value: row.value.ok_or_else(ResourceBudget::refused)?,
            version: row.version,
        };
        budget.admit(&state, 0)?;
        states.insert(row.field, state);
    }
    Ok(())
}

async fn capture_contributors(
    tx: &mut Transaction<'_, Postgres>,
    work: Uuid,
    fields: &[String],
    states: &mut BTreeMap<String, FieldState>,
    budget: &mut ResourceBudget,
) -> Result<(), AppError> {
    let (bytes, limit) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT a.id, CASE WHEN octet_length(a.name) > $3 THEN NULL \
         WHEN octet_length(to_jsonb(a.name)::text) > $3 THEN NULL ELSE a.name END AS name, \
         'contributors.' || wa.role::text AS \"field!\", wa.position, wa.source_version_id \
         FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = $1 \
         AND 'contributors.' || wa.role::text = ANY($2) ORDER BY wa.role, wa.position, a.id LIMIT $4",
        work, fields, bytes, limit,
    ).fetch(&mut **tx);
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let member = Contributor {
            id: row.id,
            name: row.name.ok_or_else(ResourceBudget::refused)?,
            position: row.position,
            version: row.source_version_id,
        };
        budget.admit(&member, 1)?;
        if let Some(FieldState::Contributors { members }) = states.get_mut(&row.field) {
            members.push(member);
        }
    }
    Ok(())
}

async fn capture_terms(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    fields: &[String],
    states: &mut BTreeMap<String, FieldState>,
    budget: &mut ResourceBudget,
) -> Result<(), AppError> {
    let (bytes, limit) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT terms.field AS \"field!\", terms.id AS \"id!\", terms.source_version_id, \
         CASE WHEN octet_length(terms.name) > $3 THEN NULL \
         WHEN octet_length(to_jsonb(terms.name)::text) > $3 THEN NULL ELSE terms.name END AS name FROM ( \
         SELECT 'genres' AS field, g.id, g.name, j.source_version_id FROM manifestation_genres j \
         JOIN genres g ON g.id = j.genre_id WHERE j.manifestation_id = $1 AND 'genres' = ANY($2) \
         UNION ALL SELECT 'moods', g.id, g.name, j.source_version_id FROM manifestation_moods j \
         JOIN moods g ON g.id = j.mood_id WHERE j.manifestation_id = $1 AND 'moods' = ANY($2) \
         UNION ALL SELECT 'tags', g.id, g.name, j.source_version_id FROM manifestation_tags j \
         JOIN tags g ON g.id = j.tag_id WHERE j.manifestation_id = $1 AND 'tags' = ANY($2) \
         ) terms ORDER BY terms.field, terms.id LIMIT $4", edition, fields, bytes, limit,
    ).fetch(&mut **tx);
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let member = Term {
            id: row.id,
            name: row.name.ok_or_else(ResourceBudget::refused)?,
            version: row.source_version_id,
        };
        budget.admit(&member, 1)?;
        if let Some(FieldState::Vocabulary { members }) = states.get_mut(&row.field) {
            members.push(member);
        }
    }
    Ok(())
}

async fn capture_identifiers(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    fields: &[String],
    states: &mut BTreeMap<String, FieldState>,
    budget: &mut ResourceBudget,
) -> Result<(), AppError> {
    let (bytes, limit) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT slots.field AS \"field!\", slots.source_version_id, \
         CASE WHEN octet_length(slots.value) > $4 THEN NULL \
         WHEN octet_length(to_jsonb(slots.value)::text) > $4 THEN NULL ELSE slots.value END AS value FROM ( \
         SELECT 'identifiers.work.' || scheme AS field, external_id AS value, source_version_id \
         FROM work_external_identifiers WHERE work_id = $1 AND 'identifiers.work.' || scheme = ANY($3) \
         UNION ALL SELECT 'identifiers.manifestation.' || scheme, external_id, source_version_id \
         FROM manifestation_external_identifiers WHERE manifestation_id = $2 \
         AND 'identifiers.manifestation.' || scheme = ANY($3) ) slots ORDER BY slots.field LIMIT $5",
        work, edition, fields, bytes, limit,
    ).fetch(&mut **tx);
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let state = FieldState::Identifier {
            value: Some(row.value.ok_or_else(ResourceBudget::refused)?),
            version: row.source_version_id,
        };
        budget.admit(&state, 0)?;
        states.insert(row.field, state);
    }
    Ok(())
}

async fn attach_sources(
    tx: &mut Transaction<'_, Postgres>,
    states: BTreeMap<String, FieldState>,
    budget: &mut ResourceBudget,
) -> Result<BTreeMap<String, CapturedField>, AppError> {
    let pointers: Vec<Uuid> = states
        .values()
        .flat_map(FieldState::pointers)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let sources: BTreeMap<Uuid, SourceMeaning> = source_meanings(tx, &pointers, budget)
        .await?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    let mut used = BTreeSet::new();
    let mut fields = BTreeMap::new();
    for (field, state) in states {
        let mut meanings = Vec::new();
        for id in state.pointers() {
            if let Some(source) = sources.get(&id) {
                if !used.insert(id) {
                    budget.admit(source, 1)?;
                }
                meanings.push(source.clone());
            }
        }
        fields.insert(
            field,
            CapturedField {
                state,
                sources: meanings,
            },
        );
    }
    Ok(fields)
}

async fn source_meanings(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
    budget: &mut ResourceBudget,
) -> Result<Vec<SourceMeaning>, AppError> {
    let (bytes, limit) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT id, source, field_name, CASE WHEN octet_length(new_value::text) <= $2 THEN new_value END AS value, \
         match_type, submitted_by FROM metadata_versions WHERE id = ANY($1) ORDER BY id LIMIT $3",
        ids, bytes, limit,
    ).fetch(&mut **tx);
    let mut result = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let source = SourceMeaning {
            id: row.id,
            source: row.source,
            field_name: row.field_name,
            value: row.value.ok_or_else(ResourceBudget::refused)?,
            match_type: row.match_type,
            submitted_by: row.submitted_by,
        };
        budget.admit(&source, 1)?;
        result.push(source);
    }
    Ok(result)
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
    budget: ResourceBudget,
}

pub(super) struct PreparedSave {
    browser: BrowserSave,
    edition: Uuid,
    work: Uuid,
    before: Capture,
    fields: Vec<String>,
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
            budget: ResourceBudget::capture(),
        })),
        Err(AppError::Forbidden) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn require_save_session(
    tx: &mut Transaction<'_, Postgres>,
    browser: Option<&mut BrowserSave>,
) -> Result<(), AppError> {
    if let Some(browser) = browser {
        browser.budget.configure(tx).await?;
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
    let Some(mut browser) = browser else {
        return Ok(None);
    };
    require_new_request(tx, &browser.session, browser.request).await?;
    let before = capture(tx, edition, work, fields, &mut browser.budget).await?;
    Ok(Some(PreparedSave {
        browser,
        edition,
        work,
        before,
        fields: fields.to_vec(),
    }))
}

pub(super) async fn finish_save(
    tx: &mut Transaction<'_, Postgres>,
    save: Option<PreparedSave>,
) -> Result<(Option<Uuid>, Vec<UndoOperation>), AppError> {
    let Some(mut save) = save else {
        return Ok((None, Vec::new()));
    };
    let fields: Vec<String> = save.before.fields.keys().cloned().collect();
    let after = capture(
        tx,
        save.edition,
        save.work,
        &fields,
        &mut save.browser.budget,
    )
    .await?;
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
        fields,
    } = save;
    let BrowserSave {
        session,
        user,
        request,
        budget,
    } = browser;
    let expires =
        sqlx::query_scalar!("SELECT clock_timestamp() + interval '1 hour' AS \"expires!\"")
            .fetch_one(&mut **tx)
            .await
            .map_err(internal)?;
    sqlx::query!(
        "UPDATE metadata_undo_operations SET superseded_at = clock_timestamp() \
         WHERE user_id = $1 AND session_id = $2 AND manifestation_id = $3 AND field_name = ANY($4) \
           AND consumed_at IS NULL AND superseded_at IS NULL",
        user,
        session,
        edition,
        &fields,
    )
    .execute(&mut **tx)
    .await
    .map_err(internal)?;
    let mut result = Vec::new();
    let mut ids = Vec::new();
    let mut fields = Vec::new();
    let mut before_states = Vec::new();
    let mut after_states = Vec::new();
    let mut revisions = Vec::new();
    for (field, before) in before.fields {
        let after_field = after
            .fields
            .get(&field)
            .ok_or_else(|| internal(anyhow::anyhow!("missing saved field memento")))?;
        if &before == after_field {
            continue;
        }
        let id = Uuid::new_v4();
        ids.push(id);
        fields.push(field.clone());
        before_states.push(serde_json::to_value(&before).map_err(internal)?);
        after_states.push(serde_json::to_value(after_field).map_err(internal)?);
        revisions.push(after.revision(&field));
        result.push(UndoOperation {
            operation_id: id,
            field_name: field,
            expires_at: expires,
            status: OperationStatus::Eligible,
        });
    }
    budget.check()?;
    sqlx::query!(
        "INSERT INTO metadata_undo_operations \
         (id, user_id, session_id, manifestation_id, work_id, request_id, field_name, before_state, after_state, after_revision, expires_at) \
         SELECT id, $1, $2, $3, $4, $5, field, before_state, after_state, revision, $6 \
         FROM UNNEST($7::uuid[], $8::text[], $9::jsonb[], $10::jsonb[], $11::bigint[]) \
         AS operation(id, field, before_state, after_state, revision)",
        user, session, edition, work, request, expires, &ids, &fields,
        &before_states as &[Value], &after_states as &[Value], &revisions,
    ).execute(&mut **tx).await.map_err(internal)?;
    budget.check()?;
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
    schemes: BTreeSet<String>,
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
            FieldState::Identifier { .. } => super::parse_identifier_field(field)
                .is_ok_and(|(_, scheme)| self.schemes.contains(scheme)),
            FieldState::Scalar { .. } => true,
        }
    }
}

async fn load_targets(
    tx: &mut Transaction<'_, Postgres>,
    before: &[(&str, &CapturedField)],
    budget: &mut ResourceBudget,
) -> Result<Targets, AppError> {
    let sources = target_sources(tx, before, budget).await?;
    let authors = target_authors(tx, before, budget).await?;
    let terms = target_terms(tx, before, budget).await?;
    let schemes: Vec<String> = before
        .iter()
        .filter(|(_, f)| matches!(f.state, FieldState::Identifier { .. }))
        .map(|(field, _)| super::parse_identifier_field(field).map(|(_, scheme)| scheme.to_owned()))
        .collect::<Result<_, _>>()?;
    let schemes = sqlx::query_scalar!(
        "SELECT id FROM identifier_schemes WHERE id = ANY($1)",
        &schemes
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?;
    for scheme in &schemes {
        budget.admit(scheme, 1)?;
    }
    Ok(Targets {
        sources,
        authors,
        terms,
        schemes: schemes.into_iter().collect(),
    })
}

async fn target_sources(
    tx: &mut Transaction<'_, Postgres>,
    before: &[(&str, &CapturedField)],
    budget: &mut ResourceBudget,
) -> Result<BTreeMap<Uuid, SourceMeaning>, AppError> {
    let expected: Vec<&SourceMeaning> = before.iter().flat_map(|(_, f)| f.sources.iter()).collect();
    let expected = serde_json::to_value(expected).map_err(internal)?;
    let (_, limit) = budget.limits()?;
    let mut rows = sqlx::query!(
        "SELECT v.id, v.source, v.field_name, v.new_value AS \"value!\", v.match_type, v.submitted_by \
         FROM metadata_versions v JOIN jsonb_to_recordset($1::jsonb) \
         AS expected(id uuid, source text, field_name text, value jsonb, match_type text, submitted_by uuid) \
         ON v.id = expected.id AND v.source = expected.source AND v.field_name = expected.field_name \
         AND v.new_value = coalesce(expected.value, 'null'::jsonb) AND v.match_type = expected.match_type \
         AND v.submitted_by IS NOT DISTINCT FROM expected.submitted_by \
         ORDER BY v.id LIMIT $2 FOR SHARE OF v", expected, limit,
    ).fetch(&mut **tx);
    let mut result = BTreeMap::new();
    while let Some(row) = rows.try_next().await.map_err(internal)? {
        let source = SourceMeaning {
            id: row.id,
            source: row.source,
            field_name: row.field_name,
            value: row.value,
            match_type: row.match_type,
            submitted_by: row.submitted_by,
        };
        budget.admit(&source, 1)?;
        result.insert(source.id, source);
    }
    Ok(result)
}

async fn target_authors(
    tx: &mut Transaction<'_, Postgres>,
    before: &[(&str, &CapturedField)],
    budget: &mut ResourceBudget,
) -> Result<BTreeMap<Uuid, String>, AppError> {
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for (_, field) in before {
        if let FieldState::Contributors { members } = &field.state {
            ids.extend(members.iter().map(|m| m.id));
            names.extend(members.iter().map(|m| m.name.clone()));
        }
    }
    let (_, limit) = budget.limits()?;
    let rows = sqlx::query!(
        "SELECT a.id, a.name FROM authors a JOIN UNNEST($1::uuid[], $2::text[]) expected(id, name) \
         ON a.id = expected.id AND a.name = expected.name ORDER BY a.id LIMIT $3 FOR SHARE OF a",
        &ids,
        &names,
        limit,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(internal)?;
    let mut result = BTreeMap::new();
    for row in rows {
        budget.admit(&(row.id, &row.name), 1)?;
        result.insert(row.id, row.name);
    }
    Ok(result)
}

async fn target_terms(
    tx: &mut Transaction<'_, Postgres>,
    before: &[(&str, &CapturedField)],
    budget: &mut ResourceBudget,
) -> Result<BTreeMap<(String, Uuid), String>, AppError> {
    let mut fields = Vec::new();
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for (field, value) in before {
        if let FieldState::Vocabulary { members } = &value.state {
            fields.extend(members.iter().map(|_| (*field).to_owned()));
            ids.extend(members.iter().map(|m| m.id));
            names.extend(members.iter().map(|m| m.name.clone()));
        }
    }
    let (_, limit) = budget.limits()?;
    let rows = sqlx::query!(
        "WITH expected AS MATERIALIZED (SELECT * FROM UNNEST($1::text[], $2::uuid[], $3::text[]) t(field, id, name)), \
         locked_genres AS MATERIALIZED (SELECT g.id, g.name FROM genres g JOIN expected e \
             ON e.field = 'genres' AND g.id = e.id AND g.name = e.name ORDER BY g.id FOR SHARE OF g), \
         locked_moods AS MATERIALIZED (SELECT g.id, g.name FROM moods g JOIN expected e \
             ON e.field = 'moods' AND g.id = e.id AND g.name = e.name ORDER BY g.id FOR SHARE OF g), \
         locked_tags AS MATERIALIZED (SELECT g.id, g.name FROM tags g JOIN expected e \
             ON e.field = 'tags' AND g.id = e.id AND g.name = e.name ORDER BY g.id FOR SHARE OF g) \
         SELECT 'genres' AS \"field!\", id AS \"id!\", name AS \"name!\" FROM locked_genres \
         UNION ALL SELECT 'moods', id, name FROM locked_moods \
         UNION ALL SELECT 'tags', id, name FROM locked_tags LIMIT $4", &fields, &ids, &names, limit,
    ).fetch_all(&mut **tx).await.map_err(internal)?;
    let mut result = BTreeMap::new();
    for row in rows {
        budget.admit(&(&row.field, row.id, &row.name), 1)?;
        result.insert((row.field, row.id), row.name);
    }
    Ok(result)
}

fn decode_memento(value: Value, budget: &mut ResourceBudget) -> Result<CapturedField, AppError> {
    let captured: CapturedField = serde_json::from_value(value).map_err(internal)?;
    let members = match &captured.state {
        FieldState::Contributors { members } => members.len(),
        FieldState::Vocabulary { members } => members.len(),
        FieldState::Scalar { .. } | FieldState::Identifier { .. } => 0,
    };
    budget.admit(&captured, 1 + members + captured.sources.len())?;
    Ok(captured)
}

async fn require_bounded_receipts(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    session: &str,
    edition: Uuid,
    request: Option<Uuid>,
    operation: Option<Uuid>,
) -> Result<(), AppError> {
    let bytes = sqlx::query_scalar!(
        "SELECT coalesce(sum(octet_length(before_state::text)::bigint + octet_length(after_state::text)),0)::bigint AS \"bytes!\" \
         FROM metadata_undo_operations WHERE user_id = $1 AND session_id = $2 AND manifestation_id = $3 \
         AND (($4::uuid IS NOT NULL AND request_id = $4) OR ($5::uuid IS NOT NULL AND id = $5)) \
         AND expires_at > clock_timestamp()", user, session, edition, request, operation,
    ).fetch_one(&mut **tx).await.map_err(internal)?;
    // PostgreSQL adds whitespace to the compact JSON encoding charged during capture.
    if bytes > i64::try_from(2 * MAX_CAPTURE_BYTES).map_err(internal)? {
        return Err(ResourceBudget::refused());
    }
    Ok(())
}

struct ResolutionChecks {
    before: BTreeMap<String, CapturedField>,
    targets: Targets,
    locked: BTreeSet<String>,
}

impl ResolutionChecks {
    fn availability(
        &self,
        field: &str,
        current: &Capture,
        after: Value,
        budget: &mut ResourceBudget,
    ) -> Result<OperationStatus, AppError> {
        let after = decode_memento(after, budget)?;
        Ok(
            if current.fields.get(field) != Some(&after)
                || self
                    .before
                    .get(field)
                    .is_none_or(|before| !self.targets.contains(field, before))
            {
                OperationStatus::Unavailable
            } else if self.locked.contains(field) {
                OperationStatus::Locked
            } else {
                OperationStatus::Eligible
            },
        )
    }

    async fn load(
        tx: &mut Transaction<'_, Postgres>,
        edition: Uuid,
        user: Uuid,
        fields: &[String],
        states: impl Iterator<Item = (String, Value)>,
        budget: &mut ResourceBudget,
    ) -> Result<Self, AppError> {
        let before: BTreeMap<String, CapturedField> = states
            .map(|(field, value)| decode_memento(value, budget).map(|state| (field, state)))
            .collect::<Result<_, _>>()?;
        let targets = load_targets(
            tx,
            &before
                .iter()
                .filter(|(field, _)| fields.contains(field))
                .map(|(field, before)| (field.as_str(), before))
                .collect::<Vec<_>>(),
            budget,
        )
        .await?;
        let locked = match require_unlocked_fields(tx, edition, user, fields).await {
            Ok(()) => BTreeSet::new(),
            Err(AppError::FieldLocked(fields)) => fields.into_iter().collect(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            before,
            targets,
            locked,
        })
    }
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
    let query = if field == "pages" {
        let value: Option<i32> = restored_value(value)?;
        sqlx::query!(
            "UPDATE manifestations SET pages = $1, pages_version_id = $2 WHERE id = $3",
            value,
            version,
            edition
        )
    } else {
        let value: Option<String> = restored_value(value)?;
        match field {
            "publisher" => sqlx::query!(
                "UPDATE manifestations SET publisher = $1, publisher_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            ),
            "pub_date" => sqlx::query!(
                "UPDATE manifestations SET pub_date = $1, pub_date_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            ),
            "isbn_10" => sqlx::query!(
                "UPDATE manifestations SET isbn_10 = $1, isbn_10_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            ),
            "isbn_13" => sqlx::query!(
                "UPDATE manifestations SET isbn_13 = $1, isbn_13_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            ),
            "content_rating" => sqlx::query!(
                "UPDATE manifestations SET content_rating = $1::text::content_rating, content_rating_version_id = $2 WHERE id = $3",
                value,
                version,
                edition
            ),
            _ => return Err(AppError::UndoUnavailable),
        }
    };
    query.execute(&mut **tx).await.map_err(restoration_error)?;
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
    super::requeue_identifier_enrichment(tx, edition).await
}

async fn restore(
    tx: &mut Transaction<'_, Postgres>,
    edition: Uuid,
    work: Uuid,
    field: &str,
    before: &CapturedField,
    budget: &mut ResourceBudget,
) -> Result<(), AppError> {
    if !load_targets(tx, &[(field, before)], budget)
        .await?
        .contains(field, before)
    {
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

impl OperationOutcome {
    fn from_operations(operations: &[UndoOperation], single_operation: bool) -> Self {
        if operations.is_empty() {
            Self::Unknown
        } else if single_operation
            && operations
                .iter()
                .any(|op| matches!(op.status, OperationStatus::Consumed))
        {
            Self::Restored
        } else {
            Self::Saved
        }
    }
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
        (status = 422, description = "Operation unavailable or Undo resource limit exceeded", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
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
    let mut budget = ResourceBudget::restoration();
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, user.user_id)
        .await
        .map_err(internal)?;
    budget.configure(&mut tx).await?;
    require_live_session(&mut tx, &session).await?;
    let work = lock_parents(&mut tx, edition).await?;
    let current = load_book_metadata(&mut tx, edition)
        .await?
        .ok_or(AppError::NotFound)?;
    let tag = current.revision.etag()?;
    if !supplied.matches(&tag) {
        return Ok(if_match_mismatch(&tag));
    }
    require_bounded_receipts(
        &mut tx,
        user.user_id,
        &session,
        edition,
        None,
        Some(request.operation_id),
    )
    .await?;
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
    let stamps = capture_stamps(&mut tx, edition, work).await?;
    if stamps.revision(&op.field_name) != op.after_revision {
        return Err(AppError::UndoUnavailable);
    }
    let now = capture(&mut tx, edition, work, &fields, &mut budget).await?;
    require_unlocked_fields(&mut tx, edition, user.user_id, &fields).await?;
    let before = decode_memento(op.before_state, &mut budget)?;
    let after = decode_memento(op.after_state, &mut budget)?;
    if now.fields.get(&op.field_name) != Some(&after) {
        tracing::warn!("metadata Undo refused a recorded-state mismatch");
        return Err(AppError::UndoConflict);
    }
    restore(&mut tx, edition, work, &op.field_name, &before, &mut budget).await?;
    budget.check()?;
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
    budget.check()?;
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
        (status = 422, description = "Select exactly one request or operation identifier; operation evidence must fit the resource limit", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
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
    let mut budget = ResourceBudget::restoration();
    budget.configure(&mut tx).await?;
    require_live_session(&mut tx, &session).await?;
    let work = lock_parents(&mut tx, edition).await?;
    require_bounded_receipts(
        &mut tx,
        user.user_id,
        &session,
        edition,
        request.request_id,
        request.operation_id,
    )
    .await?;
    let rows = sqlx::query!(
        "SELECT id, field_name, after_revision, before_state, after_state, expires_at, consumed_at, superseded_at, work_id \
         FROM metadata_undo_operations WHERE user_id = $1 AND session_id = $2 AND manifestation_id = $3 \
           AND (($4::uuid IS NOT NULL AND request_id = $4) OR ($5::uuid IS NOT NULL AND id = $5)) \
           AND expires_at > clock_timestamp() ORDER BY field_name LIMIT 144 FOR SHARE",
        user.user_id, session, edition, request.request_id, request.operation_id,
    ).fetch_all(&mut *tx).await.map_err(internal)?;
    let stamps = capture_stamps(&mut tx, edition, work).await?;
    let fields: Vec<String> = rows
        .iter()
        .filter(|r| {
            r.consumed_at.is_none()
                && r.superseded_at.is_none()
                && r.work_id == work
                && stamps.revision(&r.field_name) == r.after_revision
        })
        .map(|r| r.field_name.clone())
        .collect();
    let now = capture(&mut tx, edition, work, &fields, &mut budget).await?;
    let checks = ResolutionChecks::load(
        &mut tx,
        edition,
        user.user_id,
        &fields,
        rows.iter()
            .map(|row| (row.field_name.clone(), row.before_state.clone())),
        &mut budget,
    )
    .await?;
    let decision_time = sqlx::query_scalar!("SELECT clock_timestamp() AS \"now!\"")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
    let mut operations = Vec::new();
    for row in rows
        .into_iter()
        .filter(|row| row.expires_at > decision_time)
    {
        let status = if row.consumed_at.is_some() {
            OperationStatus::Consumed
        } else if row.superseded_at.is_some()
            || row.work_id != work
            || now.revision(&row.field_name) != row.after_revision
        {
            OperationStatus::Unavailable
        } else {
            checks.availability(&row.field_name, &now, row.after_state, &mut budget)?
        };
        operations.push(UndoOperation {
            operation_id: row.id,
            field_name: row.field_name,
            expires_at: row.expires_at,
            status,
        });
    }
    let outcome = OperationOutcome::from_operations(&operations, request.operation_id.is_some());
    let current = load_book_metadata(&mut tx, edition)
        .await?
        .ok_or(AppError::NotFound)?;
    let tag = current.revision.etag()?;
    budget.check()?;
    tx.commit().await.map_err(internal)?;
    Ok(operation_response(current.values, tag, outcome, operations))
}
