//! Write-scoped adapters for the canonical editing operations.

use super::id::Kind as _;
use super::{WriteScope, id, library, safe_error};
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::routes::etag::Conditional;
use crate::services::{
    metadata::editing::{self, undo},
    reading, shelves,
};
use crate::state::AppState;
use async_graphql::connection::{Edge, EmptyFields};
use async_graphql::{Context, ID, InputObject, MaybeUndefined, Object, SimpleObject, Union};
use std::collections::BTreeMap;
use tower_sessions::Session;
use uuid::Uuid;

pub struct Mutation;

input_output_enum!(LibraryDensity { Comfortable => "COMFORTABLE", Compact => "COMPACT" });
impl From<LibraryDensity> for crate::models::user_preferences::LibraryDensity {
    fn from(value: LibraryDensity) -> Self {
        match value {
            LibraryDensity::Comfortable => Self::Comfortable,
            LibraryDensity::Compact => Self::Compact,
        }
    }
}
impl From<crate::models::user_preferences::LibraryDensity> for LibraryDensity {
    fn from(value: crate::models::user_preferences::LibraryDensity) -> Self {
        match value {
            crate::models::user_preferences::LibraryDensity::Comfortable => Self::Comfortable,
            crate::models::user_preferences::LibraryDensity::Compact => Self::Compact,
        }
    }
}
input_output_enum!(LibraryView { Grid => "GRID", Table => "TABLE" });
impl From<LibraryView> for crate::models::user_preferences::LibraryView {
    fn from(value: LibraryView) -> Self {
        match value {
            LibraryView::Grid => Self::Grid,
            LibraryView::Table => Self::Table,
        }
    }
}
impl From<crate::models::user_preferences::LibraryView> for LibraryView {
    fn from(value: crate::models::user_preferences::LibraryView) -> Self {
        match value {
            crate::models::user_preferences::LibraryView::Grid => Self::Grid,
            crate::models::user_preferences::LibraryView::Table => Self::Table,
        }
    }
}
input_output_enum!(LibraryColumn {
    Subtitle => "SUBTITLE",
    Authors => "AUTHORS",
    Series => "SERIES",
    Isbn13 => "ISBN_13",
    Pages => "PAGES",
    Status => "STATUS",
    Rating => "RATING",
    Added => "ADDED",
});
impl LibraryColumn {
    const ALL: [Self; 8] = [
        Self::Subtitle,
        Self::Authors,
        Self::Series,
        Self::Isbn13,
        Self::Pages,
        Self::Status,
        Self::Rating,
        Self::Added,
    ];
    const fn key(self) -> &'static str {
        match self {
            Self::Subtitle => "subtitle",
            Self::Authors => "authors",
            Self::Series => "series",
            Self::Isbn13 => "isbn_13",
            Self::Pages => "pages",
            Self::Status => "status",
            Self::Rating => "rating",
            Self::Added => "added",
        }
    }
    fn known<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<Self> {
        keys.into_iter()
            .filter_map(|key| Self::ALL.into_iter().find(|column| column.key() == key))
            .collect()
    }
}

#[derive(SimpleObject)]
pub(super) struct LibrarySortLevel {
    column: library::SortField,
    direction: library::Direction,
}

fn library_sort(raw: &str) -> async_graphql::Result<Vec<LibrarySortLevel>> {
    let spec = crate::routes::sort_spec::SortSpec::parse(raw)
        .map_err(|error| safe_error(AppError::Internal(error.into())))?;
    Ok(spec
        .levels()
        .iter()
        .map(|level| LibrarySortLevel {
            column: library::SortField::from_column(level.column),
            direction: match level.direction {
                crate::routes::sort_spec::SortDirection::Asc => library::Direction::Asc,
                crate::routes::sort_spec::SortDirection::Desc => library::Direction::Desc,
            },
        })
        .collect())
}

#[derive(SimpleObject)]
pub(super) struct LibraryPreferenceDefaults {
    hidden_columns: Vec<LibraryColumn>,
    density: LibraryDensity,
    view: LibraryView,
    sort: Vec<LibrarySortLevel>,
}

#[derive(SimpleObject)]
pub(super) struct LibraryPreferences {
    hidden_columns: Option<Vec<LibraryColumn>>,
    density: Option<LibraryDensity>,
    view: Option<LibraryView>,
    sort: Option<Vec<LibrarySortLevel>>,
    defaults: LibraryPreferenceDefaults,
}

impl LibraryPreferences {
    pub(super) fn new(
        value: &crate::services::preferences::PreferencesResponse,
    ) -> async_graphql::Result<Self> {
        Ok(Self {
            hidden_columns: value
                .overrides
                .hidden_columns
                .as_ref()
                .map(|keys| LibraryColumn::known(keys.iter().map(String::as_str))),
            density: value.overrides.density.map(Into::into),
            view: value.overrides.view.map(Into::into),
            sort: value
                .overrides
                .sort_stack
                .as_deref()
                .map(library_sort)
                .transpose()?,
            defaults: LibraryPreferenceDefaults {
                hidden_columns: LibraryColumn::known(value.defaults.hidden_columns.iter().copied()),
                density: value.defaults.density.into(),
                view: value.defaults.view.into(),
                sort: library_sort(value.defaults.sort_stack)?,
            },
        })
    }
}

#[derive(InputObject)]
struct LibraryColumnVisibility {
    column: LibraryColumn,
    hidden: bool,
}

#[derive(InputObject)]
struct UpdateLibraryPreferencesInput {
    density: MaybeUndefined<LibraryDensity>,
    view: MaybeUndefined<LibraryView>,
    sort: MaybeUndefined<Vec<library::SortLevel>>,
    columns: Option<Vec<LibraryColumnVisibility>>,
    reset_hidden_columns: Option<bool>,
}

#[derive(SimpleObject)]
struct LibraryPreferencesUpdated {
    viewer: super::Viewer,
}

#[derive(SimpleObject)]
struct LibraryPreferencesInvalid {
    message: String,
}

#[derive(Union)]
enum UpdateLibraryPreferencesResult {
    LibraryPreferencesUpdated(LibraryPreferencesUpdated),
    LibraryPreferencesInvalid(LibraryPreferencesInvalid),
}

#[derive(InputObject)]
struct RefreshMetadataInput {
    id: ID,
}

#[derive(SimpleObject)]
struct MetadataRefreshQueued {
    manifestation_id: ID,
}

#[derive(Union)]
enum RefreshMetadataResult {
    MetadataRefreshQueued(MetadataRefreshQueued),
    NotFound(NotFound),
}

input_enum!(ContentRatingInput { Everyone => "EVERYONE", Teen => "TEEN", Mature => "MATURE", Adult => "ADULT", Explicit => "EXPLICIT" });
impl From<ContentRatingInput> for crate::models::content_rating::ContentRating {
    fn from(value: ContentRatingInput) -> Self {
        match value {
            ContentRatingInput::Everyone => Self::Everyone,
            ContentRatingInput::Teen => Self::Teen,
            ContentRatingInput::Mature => Self::Mature,
            ContentRatingInput::Adult => Self::Adult,
            ContentRatingInput::Explicit => Self::Explicit,
        }
    }
}
#[derive(InputObject)]
struct ContributorPatch {
    author: MaybeUndefined<Vec<String>>,
    editor: MaybeUndefined<Vec<String>>,
    translator: MaybeUndefined<Vec<String>>,
}
impl From<ContributorPatch> for editing::ContributorsPatch {
    fn from(value: ContributorPatch) -> Self {
        Self {
            author: value.author.into(),
            editor: value.editor.into(),
            translator: value.translator.into(),
        }
    }
}
#[derive(InputObject)]
struct IdentifierPatch {
    scheme: String,
    value: Option<String>,
}
fn identifiers(values: Vec<IdentifierPatch>) -> BTreeMap<String, Option<String>> {
    values
        .into_iter()
        .map(|value| (value.scheme, value.value))
        .collect()
}
#[derive(InputObject)]
struct MetadataPatch {
    title: MaybeUndefined<String>,
    subtitle: MaybeUndefined<String>,
    description: MaybeUndefined<String>,
    language: MaybeUndefined<String>,
    publisher: MaybeUndefined<String>,
    publication_date: MaybeUndefined<String>,
    isbn_10: MaybeUndefined<String>,
    isbn_13: MaybeUndefined<String>,
    pages: MaybeUndefined<i32>,
    content_rating: MaybeUndefined<ContentRatingInput>,
    genres: MaybeUndefined<Vec<String>>,
    moods: MaybeUndefined<Vec<String>>,
    tags: MaybeUndefined<Vec<String>>,
    contributors: MaybeUndefined<ContributorPatch>,
    work_identifiers: Option<Vec<IdentifierPatch>>,
    manifestation_identifiers: Option<Vec<IdentifierPatch>>,
}
impl MetadataPatch {
    fn domain(self, request_id: Option<Uuid>) -> editing::UpdateMetadataFields {
        editing::UpdateMetadataFields {
            request_id,
            title: self.title.into(),
            subtitle: self.subtitle.into(),
            description: self.description.into(),
            language: self.language.into(),
            publisher: self.publisher.into(),
            pub_date: self.publication_date.into(),
            isbn_10: self.isbn_10.into(),
            isbn_13: self.isbn_13.into(),
            pages: self.pages.into(),
            content_rating: self.content_rating.map_value(Into::into).into(),
            genres: self.genres.into(),
            moods: self.moods.into(),
            tags: self.tags.into(),
            contributors: self.contributors.map_value(Into::into).into(),
            work_identifiers: self.work_identifiers.map(identifiers),
            manifestation_identifiers: self.manifestation_identifiers.map(identifiers),
        }
    }
}
#[derive(InputObject)]
struct UpdateReadingStateInput {
    id: ID,
    baseline: library::ETag,
    patch: ReadingPatch,
}

#[derive(SimpleObject)]
struct ReadingStateUpdated {
    manifestation: library::Manifestation,
}

#[derive(Union)]
enum UpdateReadingStateResult {
    ReadingStateUpdated(ReadingStateUpdated),
    StaleBaseline(StaleBaseline),
    MetadataInvalid(MetadataInvalid),
    NotFound(NotFound),
}

impl From<Refusal> for async_graphql::Result<UpdateReadingStateResult> {
    fn from(refusal: Refusal) -> Self {
        Ok(match refusal {
            Refusal::Stale(value) => value.into(),
            Refusal::Invalid(value) => value.into(),
            Refusal::NotFound(value) => value.into(),
            _ => return Err(Refusal::unexpected()),
        })
    }
}

#[derive(InputObject)]
struct ReadingPatch {
    status: MaybeUndefined<library::ReadingStatus>,
    rating: MaybeUndefined<i16>,
    notes: MaybeUndefined<String>,
}

#[derive(SimpleObject)]
struct FieldOperation {
    operation_id: library::UuidValue,
    field_name: String,
    expires_at: library::DateTime,
    status: FieldOperationStatus,
}
output_enum!(FieldOperationStatus { Eligible => "ELIGIBLE", Locked => "LOCKED", Unavailable => "UNAVAILABLE", Consumed => "CONSUMED" });
impl From<undo::UndoOperation> for FieldOperation {
    fn from(value: undo::UndoOperation) -> Self {
        Self {
            operation_id: library::UuidValue(value.operation_id),
            field_name: value.field_name,
            expires_at: library::DateTime(value.expires_at),
            status: match value.status {
                undo::OperationStatus::Eligible => FieldOperationStatus::Eligible,
                undo::OperationStatus::Locked => FieldOperationStatus::Locked,
                undo::OperationStatus::Unavailable => FieldOperationStatus::Unavailable,
                undo::OperationStatus::Consumed => FieldOperationStatus::Consumed,
            },
        }
    }
}
#[derive(SimpleObject)]
pub(super) struct MetadataSaved {
    request_id: Option<library::UuidValue>,
    #[graphql(complexity = "super::complexity::weighted(144, child_complexity)?")]
    operations: Vec<FieldOperation>,
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataRestored {
    operation: FieldOperation,
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataVersionAccepted {
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataVersionRejected {
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataFieldReverted {
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataFieldLockAdded {
    manifestation: library::Manifestation,
}
#[derive(SimpleObject)]
pub(super) struct MetadataFieldLockRemoved {
    manifestation: library::Manifestation,
}

#[derive(SimpleObject)]
pub(super) struct StaleBaseline {
    message: String,
}
#[derive(SimpleObject)]
pub(super) struct FieldError {
    field: String,
    message: String,
}
#[derive(SimpleObject)]
pub(super) struct MetadataInvalid {
    message: String,
    field_errors: Vec<FieldError>,
}
#[derive(SimpleObject)]
pub(super) struct FieldsLocked {
    message: String,
    fields: Vec<String>,
}
#[derive(SimpleObject)]
pub(super) struct EditTooLarge {
    message: String,
}
#[derive(SimpleObject)]
pub(super) struct UndoUnavailable {
    message: String,
}
#[derive(SimpleObject)]
pub(super) struct UndoConflict {
    message: String,
}

enum Refusal {
    Stale(StaleBaseline),
    Invalid(MetadataInvalid),
    Locked(FieldsLocked),
    TooLarge(EditTooLarge),
    Unavailable(UndoUnavailable),
    Conflict(UndoConflict),
    NotFound(NotFound),
}
impl Refusal {
    fn stale() -> Self {
        Self::Stale(StaleBaseline {
            message: "The baseline is stale.".to_owned(),
        })
    }
    fn not_found() -> Self {
        Self::NotFound(NotFound::new())
    }
    fn classify(error: AppError) -> async_graphql::Result<Self> {
        Ok(match error {
            AppError::Validation(message) => Self::Invalid(MetadataInvalid {
                message,
                field_errors: Vec::new(),
            }),
            AppError::FieldValidation { detail, errors } => Self::Invalid(MetadataInvalid {
                message: detail,
                field_errors: errors
                    .into_iter()
                    .map(|(field, message)| FieldError { field, message })
                    .collect(),
            }),
            AppError::FieldLocked(fields) => Self::Locked(FieldsLocked {
                message: "Metadata fields are locked.".to_owned(),
                fields,
            }),
            AppError::EditTooLarge => Self::TooLarge(EditTooLarge {
                message: crate::error::EDIT_TOO_LARGE.to_owned(),
            }),
            AppError::UndoUnavailable => Self::Unavailable(UndoUnavailable {
                message: "Metadata Undo is unavailable.".to_owned(),
            }),
            AppError::UndoConflict => Self::Conflict(UndoConflict {
                message: "Metadata Undo state conflicts.".to_owned(),
            }),
            AppError::NotFound => Self::not_found(),
            error => return Err(safe_error(error)),
        })
    }
    fn unexpected() -> async_graphql::Error {
        safe_error(AppError::Internal(anyhow::anyhow!(
            "service returned a refusal its mutation does not declare"
        )))
    }
}

#[derive(Union)]
pub(super) enum UpdateMetadataResult {
    MetadataSaved(MetadataSaved),
    StaleBaseline(StaleBaseline),
    MetadataInvalid(MetadataInvalid),
    FieldsLocked(FieldsLocked),
    EditTooLarge(EditTooLarge),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<UpdateMetadataResult> {
    fn from(refusal: Refusal) -> Self {
        Ok(match refusal {
            Refusal::Stale(value) => value.into(),
            Refusal::Invalid(value) => value.into(),
            Refusal::Locked(value) => value.into(),
            Refusal::TooLarge(value) => value.into(),
            Refusal::NotFound(value) => value.into(),
            _ => return Err(Refusal::unexpected()),
        })
    }
}
#[derive(Union)]
pub(super) enum UndoMetadataResult {
    MetadataRestored(MetadataRestored),
    StaleBaseline(StaleBaseline),
    UndoUnavailable(UndoUnavailable),
    UndoConflict(UndoConflict),
    FieldsLocked(FieldsLocked),
    EditTooLarge(EditTooLarge),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<UndoMetadataResult> {
    fn from(refusal: Refusal) -> Self {
        Ok(match refusal {
            Refusal::Stale(value) => value.into(),
            Refusal::Unavailable(value) => value.into(),
            Refusal::Conflict(value) => value.into(),
            Refusal::Locked(value) => value.into(),
            Refusal::TooLarge(value) => value.into(),
            Refusal::NotFound(value) => value.into(),
            Refusal::Invalid(_) => return Err(Refusal::unexpected()),
        })
    }
}
#[derive(Union)]
pub(super) enum AcceptMetadataVersionResult {
    MetadataVersionAccepted(MetadataVersionAccepted),
    FieldsLocked(FieldsLocked),
    MetadataInvalid(MetadataInvalid),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<AcceptMetadataVersionResult> {
    fn from(refusal: Refusal) -> Self {
        Ok(match refusal {
            Refusal::Locked(value) => value.into(),
            Refusal::Invalid(value) => value.into(),
            Refusal::NotFound(value) => value.into(),
            _ => return Err(Refusal::unexpected()),
        })
    }
}
#[derive(Union)]
pub(super) enum RejectMetadataVersionResult {
    MetadataVersionRejected(MetadataVersionRejected),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<RejectMetadataVersionResult> {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::NotFound(value) => Ok(value.into()),
            _ => Err(Refusal::unexpected()),
        }
    }
}
#[derive(Union)]
pub(super) enum RevertMetadataFieldResult {
    MetadataFieldReverted(MetadataFieldReverted),
    FieldsLocked(FieldsLocked),
    MetadataInvalid(MetadataInvalid),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<RevertMetadataFieldResult> {
    fn from(refusal: Refusal) -> Self {
        Ok(match refusal {
            Refusal::Locked(value) => value.into(),
            Refusal::Invalid(value) => value.into(),
            Refusal::NotFound(value) => value.into(),
            _ => return Err(Refusal::unexpected()),
        })
    }
}
#[derive(Union)]
pub(super) enum LockMetadataFieldResult {
    MetadataFieldLockAdded(MetadataFieldLockAdded),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<LockMetadataFieldResult> {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::NotFound(value) => Ok(value.into()),
            _ => Err(Refusal::unexpected()),
        }
    }
}
#[derive(Union)]
pub(super) enum UnlockMetadataFieldResult {
    MetadataFieldLockRemoved(MetadataFieldLockRemoved),
    NotFound(NotFound),
}
impl From<Refusal> for async_graphql::Result<UnlockMetadataFieldResult> {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::NotFound(value) => Ok(value.into()),
            _ => Err(Refusal::unexpected()),
        }
    }
}

fn refused<T>(error: AppError) -> async_graphql::Result<T>
where
    async_graphql::Result<T>: From<Refusal>,
{
    Refusal::classify(error)?.into()
}

input_enum!(FieldLockScope { Work => "WORK", Manifestation => "MANIFESTATION" });
impl FieldLockScope {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Manifestation => "manifestation",
        }
    }
}

#[derive(InputObject)]
pub(super) struct UpdateMetadataInput {
    id: ID,
    baseline: library::ETag,
    request_id: Option<library::UuidValue>,
    patch: MetadataPatch,
}
#[derive(InputObject)]
pub(super) struct UndoMetadataInput {
    id: ID,
    baseline: library::ETag,
    operation_id: library::UuidValue,
}
#[derive(InputObject)]
pub(super) struct AcceptMetadataVersionInput {
    id: ID,
    version_id: ID,
}
#[derive(InputObject)]
pub(super) struct RejectMetadataVersionInput {
    id: ID,
    version_id: ID,
}
#[derive(InputObject)]
pub(super) struct RevertMetadataFieldInput {
    id: ID,
    field_name: String,
    version_id: Option<ID>,
}
#[derive(InputObject)]
pub(super) struct LockMetadataFieldInput {
    id: ID,
    field_name: String,
    entity_type: FieldLockScope,
}
#[derive(InputObject)]
pub(super) struct UnlockMetadataFieldInput {
    id: ID,
    field_name: String,
    entity_type: FieldLockScope,
}

fn acknowledgement_selection(
    ctx: &Context<'_>,
) -> async_graphql::Result<Option<editing::SnapshotSelection>> {
    let look = ctx.look_ahead().field("manifestation").field("metadata");
    if look.exists() {
        library::snapshot_selection(&look).map(Some)
    } else {
        Ok(None)
    }
}
async fn acknowledged(
    ctx: &Context<'_>,
    id: &ID,
    snapshot: Option<&editing::MetadataSnapshot>,
) -> async_graphql::Result<library::Manifestation> {
    // A committed write is not a refusal, so a missing read-back goes to request resolution.
    let manifestation = library::manifestation(ctx, id).await?.ok_or_else(|| {
        safe_error(AppError::Internal(anyhow::anyhow!(
            "committed metadata write has no visible manifestation"
        )))
    })?;
    Ok(match snapshot {
        Some(snapshot) => {
            manifestation.with_metadata(library::MetadataRevision::from_domain(snapshot)?)
        }
        None => manifestation,
    })
}
#[derive(SimpleObject)]
pub(super) struct MetadataOperation {
    outcome: OperationOutcome,
    #[graphql(complexity = "super::complexity::weighted(144, child_complexity)?")]
    operations: Vec<FieldOperation>,
    metadata: library::MetadataRevision,
}
output_enum!(OperationOutcome { Saved => "SAVED", Restored => "RESTORED", Unknown => "UNKNOWN" });
impl MetadataOperation {
    fn new(result: undo::OperationResult) -> async_graphql::Result<Self> {
        Ok(Self {
            outcome: match result.outcome {
                undo::OperationOutcome::Saved => OperationOutcome::Saved,
                undo::OperationOutcome::Restored => OperationOutcome::Restored,
                undo::OperationOutcome::Unknown => OperationOutcome::Unknown,
            },
            operations: result.operations.into_iter().map(Into::into).collect(),
            metadata: library::MetadataRevision::from_domain(&result.snapshot)?,
        })
    }
}
#[derive(SimpleObject)]
pub(super) struct NotFound {
    message: String,
}
impl NotFound {
    fn new() -> Self {
        Self {
            message: "Not found".to_owned(),
        }
    }
}
#[derive(SimpleObject)]
pub(super) struct SystemShelfImmutable {
    message: String,
}
impl SystemShelfImmutable {
    fn new() -> Self {
        Self {
            message: "System shelf cannot be modified.".to_owned(),
        }
    }
}
#[derive(SimpleObject)]
pub(super) struct ShelfNameInvalid {
    message: String,
}

#[derive(InputObject)]
pub(super) struct CreateShelfInput {
    name: String,
}
#[derive(InputObject)]
pub(super) struct RenameShelfInput {
    id: ID,
    name: String,
}
#[derive(InputObject)]
pub(super) struct DeleteShelfInput {
    id: ID,
}
#[derive(InputObject)]
pub(super) struct AddShelfItemInput {
    shelf_id: ID,
    manifestation_id: ID,
}
#[derive(InputObject)]
pub(super) struct RemoveShelfItemInput {
    shelf_id: ID,
    manifestation_id: ID,
}

#[derive(SimpleObject)]
pub(super) struct ShelfCreated {
    shelf: library::Shelf,
    edge: Edge<String, library::Shelf, EmptyFields>,
}
#[derive(SimpleObject)]
pub(super) struct ShelfRenamed {
    shelf: library::Shelf,
}
#[derive(SimpleObject)]
pub(super) struct ShelfDeleted {
    deleted_shelf_id: ID,
}
#[derive(SimpleObject)]
pub(super) struct ShelfItemAdded {
    shelf: library::Shelf,
    edge: Edge<String, library::Manifestation, EmptyFields>,
}
#[derive(SimpleObject)]
pub(super) struct ShelfItemRemoved {
    shelf: library::Shelf,
    deleted_manifestation_id: ID,
}

#[derive(Union)]
pub(super) enum CreateShelfResult {
    ShelfCreated(ShelfCreated),
    ShelfNameInvalid(ShelfNameInvalid),
}
#[derive(Union)]
pub(super) enum RenameShelfResult {
    ShelfRenamed(ShelfRenamed),
    ShelfNameInvalid(ShelfNameInvalid),
    SystemShelfImmutable(SystemShelfImmutable),
    NotFound(NotFound),
}
#[derive(Union)]
pub(super) enum DeleteShelfResult {
    ShelfDeleted(ShelfDeleted),
    SystemShelfImmutable(SystemShelfImmutable),
    NotFound(NotFound),
}
#[derive(Union)]
pub(super) enum AddShelfItemResult {
    ShelfItemAdded(ShelfItemAdded),
    NotFound(NotFound),
}
#[derive(Union)]
pub(super) enum RemoveShelfItemResult {
    ShelfItemRemoved(ShelfItemRemoved),
    NotFound(NotFound),
}

fn target<K: id::Kind>(value: &ID) -> async_graphql::Result<Uuid> {
    K::decode(value)
        .ok_or_else(|| safe_error(AppError::Validation(format!("Invalid {} ID.", K::NAME))))
}
fn adult(ctx: &Context<'_>) -> async_graphql::Result<()> {
    ctx.data::<CurrentUser>()?
        .require_not_child()
        .map_err(safe_error)
}

#[Object]
impl Mutation {
    #[graphql(guard = "WriteScope")]
    async fn update_library_preferences(
        &self,
        ctx: &Context<'_>,
        input: UpdateLibraryPreferencesInput,
    ) -> async_graphql::Result<UpdateLibraryPreferencesResult> {
        let user = ctx.data::<CurrentUser>()?;
        if input.reset_hidden_columns == Some(true) && input.columns.is_some() {
            return Ok(UpdateLibraryPreferencesResult::LibraryPreferencesInvalid(
                LibraryPreferencesInvalid {
                    message: "resetHiddenColumns cannot be combined with columns".into(),
                },
            ));
        }
        let request = crate::services::preferences::UpdatePreferencesRequest {
            hidden_columns: (input.reset_hidden_columns == Some(true)).then_some(None),
            density: input.density.map_value(Into::into).into(),
            view: input.view.map_value(Into::into).into(),
            sort_stack: input
                .sort
                .map_value(|levels| library::sort_wire(&levels))
                .into(),
            column_changes: input
                .columns
                .unwrap_or_default()
                .into_iter()
                .map(|change| crate::services::preferences::ColumnChange {
                    key: change.column.key(),
                    hidden: change.hidden,
                })
                .collect(),
        };
        match crate::services::preferences::update_preferences(ctx.data()?, user, request).await {
            Ok(preferences) => Ok(UpdateLibraryPreferencesResult::LibraryPreferencesUpdated(
                LibraryPreferencesUpdated {
                    viewer: super::Viewer::new(user, Some(preferences)),
                },
            )),
            Err(AppError::Validation(message)) => {
                Ok(UpdateLibraryPreferencesResult::LibraryPreferencesInvalid(
                    LibraryPreferencesInvalid { message },
                ))
            }
            Err(error) => Err(safe_error(error)),
        }
    }

    #[graphql(guard = "WriteScope")]
    async fn refresh_metadata(
        &self,
        ctx: &Context<'_>,
        input: RefreshMetadataInput,
    ) -> async_graphql::Result<RefreshMetadataResult> {
        let user = ctx.data::<CurrentUser>()?;
        user.require_not_child().map_err(safe_error)?;
        let Some(manifestation) = id::Manifestation::decode(&input.id) else {
            return Ok(RefreshMetadataResult::NotFound(NotFound::new()));
        };
        let mut tx = crate::db::acquire_with_rls(ctx.data()?, user.user_id)
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        let changed = crate::services::enrichment::reset_retries(
            &mut tx,
            &[manifestation],
            crate::services::enrichment::RetryTiming::ClearProviderDelay,
        )
        .await
        .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        if changed == 0 {
            return Ok(RefreshMetadataResult::NotFound(NotFound::new()));
        }
        tx.commit()
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        Ok(RefreshMetadataResult::MetadataRefreshQueued(
            MetadataRefreshQueued {
                manifestation_id: input.id,
            },
        ))
    }

    #[graphql(guard = "WriteScope")]
    async fn update_metadata(
        &self,
        ctx: &Context<'_>,
        input: UpdateMetadataInput,
    ) -> async_graphql::Result<UpdateMetadataResult> {
        adult(ctx)?;
        let supplied = input.baseline.strong()?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let request_id = input.request_id.map(|value| value.0);
        let selection = library::snapshot_selection(
            &ctx.look_ahead().field("manifestation").field("metadata"),
        )?;
        let result = match editing::update_book_metadata(
            ctx.data()?,
            ctx.data()?,
            ctx.data()?,
            edition,
            Some(supplied),
            input.patch.domain(request_id),
            &selection,
        )
        .await
        {
            Ok(Conditional::Matched(result)) => result,
            Ok(Conditional::Mismatch(_)) => return Refusal::stale().into(),
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, Some(&result.snapshot)).await?;
        Ok(UpdateMetadataResult::MetadataSaved(MetadataSaved {
            request_id: result.request_id.map(library::UuidValue),
            operations: result.operations.into_iter().map(Into::into).collect(),
            manifestation,
        }))
    }
    #[graphql(guard = "WriteScope")]
    async fn update_reading_state(
        &self,
        ctx: &Context<'_>,
        input: UpdateReadingStateInput,
    ) -> async_graphql::Result<UpdateReadingStateResult> {
        let user = ctx.data::<CurrentUser>()?;
        let baseline = input.baseline.strong()?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let patch = input.patch;
        let row = match reading::patch_reading(
            user,
            ctx.data()?,
            edition,
            Some(baseline),
            reading::UpdateReadingRequest {
                status: patch.status.map_value(Into::into).into(),
                rating: patch.rating.into(),
                notes: patch.notes.into(),
            },
        )
        .await
        {
            Ok(Conditional::Matched(row)) => row,
            Ok(Conditional::Mismatch(_)) => return Refusal::stale().into(),
            Err(error) => return refused(error),
        };
        let etag = row
            .etag()
            .map_err(safe_error)?
            .to_str()
            .map_err(|error| safe_error(AppError::Internal(error.into())))?
            .to_owned();
        let revision = library::ReadingRevision::new(user.user_id, edition, &row, etag)?;
        let manifestation = acknowledged(ctx, &input.id, None)
            .await?
            .with_reading(revision);
        Ok(UpdateReadingStateResult::ReadingStateUpdated(
            ReadingStateUpdated { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn create_shelf(
        &self,
        ctx: &Context<'_>,
        input: CreateShelfInput,
    ) -> async_graphql::Result<CreateShelfResult> {
        match shelves::create_shelf(ctx.data()?, ctx.data()?, &input.name).await {
            Ok(row) => {
                let cursor = crate::routes::cursor::ShelfCursor {
                    is_system: row.is_system,
                    name: row.name.clone(),
                    id: row.id,
                }
                .encode(crate::routes::cursor::CursorApi::Graphql);
                let created = library::Shelf::from_model(row)?;
                Ok(CreateShelfResult::ShelfCreated(ShelfCreated {
                    edge: Edge::new(cursor, created.clone()),
                    shelf: created,
                }))
            }
            Err(AppError::Validation(message)) => Ok(ShelfNameInvalid { message }.into()),
            Err(error) => Err(safe_error(error)),
        }
    }
    #[graphql(guard = "WriteScope")]
    async fn rename_shelf(
        &self,
        ctx: &Context<'_>,
        input: RenameShelfInput,
    ) -> async_graphql::Result<RenameShelfResult> {
        let Some(shelf_id) = id::Shelf::decode(&input.id) else {
            return Ok(NotFound::new().into());
        };
        match shelves::rename_shelf(ctx.data()?, ctx.data()?, shelf_id, &input.name).await {
            Ok(row) => Ok(RenameShelfResult::ShelfRenamed(ShelfRenamed {
                shelf: library::Shelf::from_model(row)?,
            })),
            Err(AppError::Validation(message)) => Ok(ShelfNameInvalid { message }.into()),
            Err(AppError::NotFound) => Ok(NotFound::new().into()),
            Err(AppError::SystemShelfImmutable) => Ok(SystemShelfImmutable::new().into()),
            Err(error) => Err(safe_error(error)),
        }
    }
    #[graphql(guard = "WriteScope")]
    async fn delete_shelf(
        &self,
        ctx: &Context<'_>,
        input: DeleteShelfInput,
    ) -> async_graphql::Result<DeleteShelfResult> {
        let Some(shelf_id) = id::Shelf::decode(&input.id) else {
            return Ok(NotFound::new().into());
        };
        match shelves::delete_shelf(ctx.data()?, ctx.data()?, shelf_id).await {
            Ok(()) => Ok(DeleteShelfResult::ShelfDeleted(ShelfDeleted {
                deleted_shelf_id: input.id,
            })),
            Err(AppError::NotFound) => Ok(NotFound::new().into()),
            Err(AppError::SystemShelfImmutable) => Ok(SystemShelfImmutable::new().into()),
            Err(error) => Err(safe_error(error)),
        }
    }
    #[graphql(guard = "WriteScope")]
    async fn add_shelf_item(
        &self,
        ctx: &Context<'_>,
        input: AddShelfItemInput,
    ) -> async_graphql::Result<AddShelfItemResult> {
        let (Some(shelf_id), Some(manifestation_id)) = (
            id::Shelf::decode(&input.shelf_id),
            id::Manifestation::decode(&input.manifestation_id),
        ) else {
            return Ok(NotFound::new().into());
        };
        let added_at =
            match shelves::add_shelf_item(ctx.data()?, ctx.data()?, shelf_id, manifestation_id)
                .await
            {
                Ok(added_at) => added_at,
                Err(AppError::NotFound) => return Ok(NotFound::new().into()),
                Err(error) => return Err(safe_error(error)),
            };
        let (Some(updated), Some(manifestation)) = (
            library::shelf(ctx, &input.shelf_id).await?,
            library::manifestation(ctx, &input.manifestation_id).await?,
        ) else {
            return Ok(NotFound::new().into());
        };
        let cursor =
            crate::services::reader::shelf_item_cursor(shelf_id, manifestation_id, added_at)
                .map_err(safe_error)?;
        Ok(AddShelfItemResult::ShelfItemAdded(ShelfItemAdded {
            shelf: updated,
            edge: Edge::new(cursor, manifestation),
        }))
    }
    #[graphql(guard = "WriteScope")]
    async fn remove_shelf_item(
        &self,
        ctx: &Context<'_>,
        input: RemoveShelfItemInput,
    ) -> async_graphql::Result<RemoveShelfItemResult> {
        let (Some(shelf_id), Some(manifestation_id)) = (
            id::Shelf::decode(&input.shelf_id),
            id::Manifestation::decode(&input.manifestation_id),
        ) else {
            return Ok(NotFound::new().into());
        };
        match shelves::remove_shelf_item(ctx.data()?, ctx.data()?, shelf_id, manifestation_id).await
        {
            Ok(()) => {}
            Err(AppError::NotFound) => return Ok(NotFound::new().into()),
            Err(error) => return Err(safe_error(error)),
        }
        let Some(updated) = library::shelf(ctx, &input.shelf_id).await? else {
            return Ok(NotFound::new().into());
        };
        Ok(RemoveShelfItemResult::ShelfItemRemoved(ShelfItemRemoved {
            shelf: updated,
            deleted_manifestation_id: input.manifestation_id,
        }))
    }
    #[graphql(guard = "WriteScope")]
    async fn accept_metadata_version(
        &self,
        ctx: &Context<'_>,
        input: AcceptMetadataVersionInput,
    ) -> async_graphql::Result<AcceptMetadataVersionResult> {
        adult(ctx)?;
        let (Some(edition), Some(version_id)) = (
            id::Manifestation::decode(&input.id),
            id::MetadataVersion::decode(&input.version_id),
        ) else {
            return Refusal::not_found().into();
        };
        let selection = acknowledgement_selection(ctx)?;
        let snapshot = match editing::accept_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::VersionPayload { version_id },
            selection.as_ref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, snapshot.as_ref()).await?;
        Ok(AcceptMetadataVersionResult::MetadataVersionAccepted(
            MetadataVersionAccepted { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn reject_metadata_version(
        &self,
        ctx: &Context<'_>,
        input: RejectMetadataVersionInput,
    ) -> async_graphql::Result<RejectMetadataVersionResult> {
        adult(ctx)?;
        let (Some(edition), Some(version_id)) = (
            id::Manifestation::decode(&input.id),
            id::MetadataVersion::decode(&input.version_id),
        ) else {
            return Refusal::not_found().into();
        };
        let selection = acknowledgement_selection(ctx)?;
        let snapshot = match editing::reject_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::VersionPayload { version_id },
            selection.as_ref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, snapshot.as_ref()).await?;
        Ok(RejectMetadataVersionResult::MetadataVersionRejected(
            MetadataVersionRejected { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn revert_metadata_field(
        &self,
        ctx: &Context<'_>,
        input: RevertMetadataFieldInput,
    ) -> async_graphql::Result<RevertMetadataFieldResult> {
        adult(ctx)?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let version_id = match input.version_id.as_ref().map(id::MetadataVersion::decode) {
            None => None,
            Some(Some(version_id)) => Some(version_id),
            Some(None) => return Refusal::not_found().into(),
        };
        let selection = acknowledgement_selection(ctx)?;
        let snapshot = match editing::revert_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::RevertPayload {
                field_name: input.field_name,
                version_id,
            },
            selection.as_ref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, snapshot.as_ref()).await?;
        Ok(RevertMetadataFieldResult::MetadataFieldReverted(
            MetadataFieldReverted { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn lock_metadata_field(
        &self,
        ctx: &Context<'_>,
        input: LockMetadataFieldInput,
    ) -> async_graphql::Result<LockMetadataFieldResult> {
        adult(ctx)?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let selection = acknowledgement_selection(ctx)?;
        let snapshot = match editing::lock_field(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::LockPayload {
                field_name: input.field_name,
                entity_type: input.entity_type.as_str().to_owned(),
            },
            selection.as_ref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, snapshot.as_ref()).await?;
        Ok(LockMetadataFieldResult::MetadataFieldLockAdded(
            MetadataFieldLockAdded { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn unlock_metadata_field(
        &self,
        ctx: &Context<'_>,
        input: UnlockMetadataFieldInput,
    ) -> async_graphql::Result<UnlockMetadataFieldResult> {
        adult(ctx)?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let selection = acknowledgement_selection(ctx)?;
        let snapshot = match editing::unlock_field(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::LockPayload {
                field_name: input.field_name,
                entity_type: input.entity_type.as_str().to_owned(),
            },
            selection.as_ref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return refused(error),
        };
        let manifestation = acknowledged(ctx, &input.id, snapshot.as_ref()).await?;
        Ok(UnlockMetadataFieldResult::MetadataFieldLockRemoved(
            MetadataFieldLockRemoved { manifestation },
        ))
    }
    #[graphql(guard = "WriteScope")]
    async fn undo_metadata(
        &self,
        ctx: &Context<'_>,
        input: UndoMetadataInput,
    ) -> async_graphql::Result<UndoMetadataResult> {
        adult(ctx)?;
        if let Err(error) = undo::session_id(ctx.data()?, ctx.data()?) {
            return refused(error);
        }
        let supplied = input.baseline.strong()?;
        let Some(edition) = id::Manifestation::decode(&input.id) else {
            return Refusal::not_found().into();
        };
        let operation_id = input.operation_id.0;
        let selection = library::snapshot_selection(
            &ctx.look_ahead().field("manifestation").field("metadata"),
        )?;
        let result = match undo::undo_metadata(
            ctx.data()?,
            ctx.data()?,
            ctx.data()?,
            edition,
            Some(supplied),
            undo::UndoRequest { operation_id },
            &selection,
        )
        .await
        {
            Ok(Conditional::Matched(result)) => result,
            Ok(Conditional::Mismatch(_)) => return Refusal::stale().into(),
            Err(error) => return refused(error),
        };
        let Some(operation) = result.operations.into_iter().next() else {
            return Err(safe_error(AppError::Internal(anyhow::anyhow!(
                "metadata Undo restored without its operation"
            ))));
        };
        let manifestation = acknowledged(ctx, &input.id, Some(&result.snapshot)).await?;
        Ok(UndoMetadataResult::MetadataRestored(MetadataRestored {
            operation: operation.into(),
            manifestation,
        }))
    }
}
pub(super) async fn metadata_operation(
    ctx: &Context<'_>,
    id: ID,
    request_id: Option<library::UuidValue>,
    operation_id: Option<library::UuidValue>,
) -> async_graphql::Result<MetadataOperation> {
    adult(ctx)?;
    let selection = library::snapshot_selection(&ctx.look_ahead().field("metadata"))?;
    MetadataOperation::new(
        undo::resolve_metadata(
            ctx.data::<CurrentUser>()?,
            ctx.data::<AppState>()?,
            ctx.data::<Session>()?,
            target::<id::Manifestation>(&id)?,
            undo::ResolveRequest {
                request_id: request_id.map(|value| value.0),
                operation_id: operation_id.map(|value| value.0),
            },
            &selection,
        )
        .await
        .map_err(safe_error)?,
    )
}
