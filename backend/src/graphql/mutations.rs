//! Write-scoped adapters for the canonical editing operations.

use super::id::Kind as _;
use super::{WriteScope, id, library, safe_error};
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::routes::etag::{Conditional, StrongEntityTag, parse_strong_entity_tag};
use crate::services::{
    metadata::editing::{self, undo},
    reading, shelves,
};
use crate::state::AppState;
use async_graphql::{
    Context, ErrorExtensions, ID, InputObject, MaybeUndefined, Object, SimpleObject,
};
use std::collections::BTreeMap;
use tower_sessions::Session;
use uuid::Uuid;

pub struct Mutation;

#[derive(SimpleObject)]
pub(super) struct LibraryPreferenceDefaults {
    hidden_columns: Vec<String>,
    density: String,
    view: String,
    sort_stack: String,
}

#[derive(SimpleObject)]
pub(super) struct LibraryPreferences {
    hidden_columns: Option<Vec<String>>,
    density: Option<String>,
    view: Option<String>,
    sort_stack: Option<String>,
    defaults: LibraryPreferenceDefaults,
}

impl From<crate::services::preferences::PreferencesResponse> for LibraryPreferences {
    fn from(value: crate::services::preferences::PreferencesResponse) -> Self {
        Self {
            hidden_columns: value.overrides.hidden_columns,
            density: value.overrides.density.map(density_name),
            view: value.overrides.view.map(view_name),
            sort_stack: value.overrides.sort_stack,
            defaults: LibraryPreferenceDefaults {
                hidden_columns: value
                    .defaults
                    .hidden_columns
                    .iter()
                    .map(|key| (*key).to_owned())
                    .collect(),
                density: density_name(value.defaults.density),
                view: view_name(value.defaults.view),
                sort_stack: value.defaults.sort_stack.to_owned(),
            },
        }
    }
}

input_enum!(LibraryDensityInput { Comfortable => "COMFORTABLE", Compact => "COMPACT" });
impl From<LibraryDensityInput> for crate::models::user_preferences::LibraryDensity {
    fn from(value: LibraryDensityInput) -> Self {
        match value {
            LibraryDensityInput::Comfortable => Self::Comfortable,
            LibraryDensityInput::Compact => Self::Compact,
        }
    }
}
input_enum!(LibraryViewInput { Grid => "GRID", Table => "TABLE" });
impl From<LibraryViewInput> for crate::models::user_preferences::LibraryView {
    fn from(value: LibraryViewInput) -> Self {
        match value {
            LibraryViewInput::Grid => Self::Grid,
            LibraryViewInput::Table => Self::Table,
        }
    }
}
fn density_name(value: crate::models::user_preferences::LibraryDensity) -> String {
    match value {
        crate::models::user_preferences::LibraryDensity::Comfortable => "comfortable",
        crate::models::user_preferences::LibraryDensity::Compact => "compact",
    }
    .to_owned()
}
fn view_name(value: crate::models::user_preferences::LibraryView) -> String {
    match value {
        crate::models::user_preferences::LibraryView::Grid => "grid",
        crate::models::user_preferences::LibraryView::Table => "table",
    }
    .to_owned()
}

#[derive(InputObject)]
struct LibraryPreferencesPatch {
    hidden_columns: MaybeUndefined<Vec<String>>,
    density: MaybeUndefined<LibraryDensityInput>,
    view: MaybeUndefined<LibraryViewInput>,
    sort_stack: MaybeUndefined<String>,
}

#[derive(SimpleObject)]
struct MetadataRefresh {
    manifestation_id: ID,
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
input_enum!(ReadingStatusInput { WantToRead => "WANT_TO_READ", Reading => "READING", OnHold => "ON_HOLD", Finished => "FINISHED", Abandoned => "ABANDONED" });
impl From<ReadingStatusInput> for crate::models::reading_status::ReadingStatus {
    fn from(value: ReadingStatusInput) -> Self {
        match value {
            ReadingStatusInput::WantToRead => Self::WantToRead,
            ReadingStatusInput::Reading => Self::Reading,
            ReadingStatusInput::OnHold => Self::OnHold,
            ReadingStatusInput::Finished => Self::Finished,
            ReadingStatusInput::Abandoned => Self::Abandoned,
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
    fn domain(
        self,
        request_id: Option<String>,
    ) -> async_graphql::Result<editing::UpdateMetadataFields> {
        Ok(editing::UpdateMetadataFields {
            request_id: request_id.map(|value| uuid(&value)).transpose()?,
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
        })
    }
}
#[derive(InputObject)]
struct ReadingPatch {
    status: MaybeUndefined<ReadingStatusInput>,
    rating: MaybeUndefined<i16>,
    notes: MaybeUndefined<String>,
}

#[derive(SimpleObject)]
struct FieldOperation {
    operation_id: String,
    field_name: String,
    expires_at: String,
    status: String,
}
impl From<undo::UndoOperation> for FieldOperation {
    fn from(value: undo::UndoOperation) -> Self {
        Self {
            operation_id: value.operation_id.to_string(),
            field_name: value.field_name,
            expires_at: value.expires_at.to_string(),
            status: match value.status {
                undo::OperationStatus::Eligible => "eligible",
                undo::OperationStatus::Locked => "locked",
                undo::OperationStatus::Unavailable => "unavailable",
                undo::OperationStatus::Consumed => "consumed",
            }
            .to_owned(),
        }
    }
}
#[derive(SimpleObject)]
struct MetadataSave {
    request_id: Option<String>,
    #[graphql(complexity = "super::complexity::weighted(144, child_complexity)?")]
    operations: Vec<FieldOperation>,
    metadata: library::MetadataSnapshot,
}
#[derive(SimpleObject)]
pub(super) struct MetadataOperation {
    outcome: String,
    #[graphql(complexity = "super::complexity::weighted(144, child_complexity)?")]
    operations: Vec<FieldOperation>,
    metadata: library::MetadataSnapshot,
}
impl MetadataOperation {
    fn new(result: undo::OperationResult) -> async_graphql::Result<Self> {
        Ok(Self {
            outcome: match result.outcome {
                undo::OperationOutcome::Saved => "saved",
                undo::OperationOutcome::Restored => "restored",
                undo::OperationOutcome::Unknown => "unknown",
            }
            .to_owned(),
            operations: result.operations.into_iter().map(Into::into).collect(),
            metadata: library::MetadataSnapshot::from_domain(&result.snapshot)?,
        })
    }
}
#[derive(SimpleObject)]
struct MetadataReview {
    manifestation_id: ID,
    metadata: Option<library::MetadataSnapshot>,
}
impl MetadataReview {
    fn new(
        edition: Uuid,
        snapshot: Option<&editing::MetadataSnapshot>,
    ) -> async_graphql::Result<Self> {
        Ok(Self {
            manifestation_id: id::Manifestation::encode(edition),
            metadata: snapshot
                .map(library::MetadataSnapshot::from_domain)
                .transpose()?,
        })
    }
}
#[derive(SimpleObject)]
struct ShelfChange {
    shelf_id: ID,
    manifestation_id: Option<ID>,
}

fn target<K: id::Kind>(value: &ID) -> async_graphql::Result<Uuid> {
    K::decode(value)
        .ok_or_else(|| safe_error(AppError::Validation(format!("Invalid {} ID.", K::NAME))))
}
fn uuid(value: &str) -> async_graphql::Result<Uuid> {
    value.parse().map_err(|_| {
        safe_error(AppError::Validation(
            "Invalid operation or request UUID.".into(),
        ))
    })
}
fn baseline(value: Option<String>) -> async_graphql::Result<Option<StrongEntityTag>> {
    value
        .map(|value| parse_strong_entity_tag(value.as_bytes()).map_err(safe_error))
        .transpose()
}
fn matched<T>(value: Conditional<T>) -> async_graphql::Result<T> {
    match value {
        Conditional::Matched(value) => Ok(value),
        Conditional::Mismatch(tag) => {
            let current = tag
                .to_str()
                .map_err(|e| safe_error(AppError::Internal(e.into())))?;
            Err(safe_error(AppError::IfMatchMismatch)
                .extend_with(|_, extensions| extensions.set("currentEtag", current)))
        }
    }
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
        patch: LibraryPreferencesPatch,
    ) -> async_graphql::Result<LibraryPreferences> {
        let patch = crate::services::preferences::UpdatePreferencesRequest {
            hidden_columns: patch.hidden_columns.into(),
            density: patch.density.map_value(Into::into).into(),
            view: patch.view.map_value(Into::into).into(),
            sort_stack: patch.sort_stack.into(),
        };
        crate::services::preferences::update_preferences(ctx.data()?, ctx.data()?, patch)
            .await
            .map(Into::into)
            .map_err(safe_error)
    }

    #[graphql(guard = "WriteScope")]
    async fn refresh_metadata(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<MetadataRefresh> {
        let user = ctx.data::<CurrentUser>()?;
        user.require_not_child().map_err(safe_error)?;
        let mut tx = crate::db::acquire_with_rls(ctx.data()?, user.user_id)
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        let changed = crate::services::enrichment::reset_retries(
            &mut tx,
            &[target::<id::Manifestation>(&id)?],
            crate::services::enrichment::RetryTiming::ClearProviderDelay,
        )
        .await
        .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        if changed == 0 {
            return Err(safe_error(AppError::NotFound));
        }
        tx.commit()
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        Ok(MetadataRefresh {
            manifestation_id: id,
        })
    }

    #[graphql(guard = "WriteScope")]
    async fn update_metadata(
        &self,
        ctx: &Context<'_>,
        id: ID,
        baseline: Option<String>,
        request_id: Option<String>,
        patch: MetadataPatch,
    ) -> async_graphql::Result<MetadataSave> {
        adult(ctx)?;
        let supplied = self::baseline(baseline)?;
        let selection = library::snapshot_selection(&ctx.look_ahead().field("metadata"))?;
        let result = matched(
            editing::update_book_metadata(
                ctx.data()?,
                ctx.data()?,
                ctx.data()?,
                target::<id::Manifestation>(&id)?,
                supplied,
                patch.domain(request_id)?,
                &selection,
            )
            .await
            .map_err(safe_error)?,
        )?;
        Ok(MetadataSave {
            request_id: result.request_id.map(|id| id.to_string()),
            operations: result.operations.into_iter().map(Into::into).collect(),
            metadata: library::MetadataSnapshot::from_domain(&result.snapshot)?,
        })
    }
    #[graphql(guard = "WriteScope")]
    async fn update_reading_state(
        &self,
        ctx: &Context<'_>,
        id: ID,
        baseline: Option<String>,
        patch: ReadingPatch,
    ) -> async_graphql::Result<library::ReadingState> {
        let supplied = self::baseline(baseline)?;
        let row = matched(
            reading::patch_reading(
                ctx.data()?,
                ctx.data()?,
                target::<id::Manifestation>(&id)?,
                supplied,
                reading::UpdateReadingRequest {
                    status: patch.status.map_value(Into::into).into(),
                    rating: patch.rating.into(),
                    notes: patch.notes.into(),
                },
            )
            .await
            .map_err(safe_error)?,
        )?;
        library::ReadingState::from_row(&row)
    }
    #[graphql(guard = "WriteScope")]
    async fn create_shelf(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<library::Shelf> {
        library::Shelf::from_model(
            shelves::create_shelf(ctx.data()?, ctx.data()?, &name)
                .await
                .map_err(safe_error)?,
        )
    }
    #[graphql(guard = "WriteScope")]
    async fn rename_shelf(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<library::Shelf> {
        library::Shelf::from_model(
            shelves::rename_shelf(ctx.data()?, ctx.data()?, target::<id::Shelf>(&id)?, &name)
                .await
                .map_err(safe_error)?,
        )
    }
    #[graphql(guard = "WriteScope")]
    async fn delete_shelf(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<ShelfChange> {
        let target_id = target::<id::Shelf>(&id)?;
        shelves::delete_shelf(ctx.data()?, ctx.data()?, target_id)
            .await
            .map_err(safe_error)?;
        Ok(ShelfChange {
            shelf_id: id,
            manifestation_id: None,
        })
    }
    #[graphql(guard = "WriteScope")]
    async fn add_shelf_item(
        &self,
        ctx: &Context<'_>,
        shelf_id: ID,
        manifestation_id: ID,
    ) -> async_graphql::Result<ShelfChange> {
        shelves::add_shelf_item(
            ctx.data()?,
            ctx.data()?,
            target::<id::Shelf>(&shelf_id)?,
            target::<id::Manifestation>(&manifestation_id)?,
        )
        .await
        .map_err(safe_error)?;
        Ok(ShelfChange {
            shelf_id,
            manifestation_id: Some(manifestation_id),
        })
    }
    #[graphql(guard = "WriteScope")]
    async fn remove_shelf_item(
        &self,
        ctx: &Context<'_>,
        shelf_id: ID,
        manifestation_id: ID,
    ) -> async_graphql::Result<ShelfChange> {
        shelves::remove_shelf_item(
            ctx.data()?,
            ctx.data()?,
            target::<id::Shelf>(&shelf_id)?,
            target::<id::Manifestation>(&manifestation_id)?,
        )
        .await
        .map_err(safe_error)?;
        Ok(ShelfChange {
            shelf_id,
            manifestation_id: Some(manifestation_id),
        })
    }
    #[graphql(guard = "WriteScope")]
    async fn accept_metadata_version(
        &self,
        ctx: &Context<'_>,
        id: ID,
        version_id: ID,
    ) -> async_graphql::Result<MetadataReview> {
        adult(ctx)?;
        let edition = target::<id::Manifestation>(&id)?;
        let look = ctx.look_ahead().field("metadata");
        let selection = if look.exists() {
            Some(library::snapshot_selection(&look)?)
        } else {
            None
        };
        let snapshot = editing::accept_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::VersionPayload {
                version_id: target::<id::MetadataVersion>(&version_id)?,
            },
            selection.as_ref(),
        )
        .await
        .map_err(safe_error)?;
        MetadataReview::new(edition, snapshot.as_ref())
    }
    #[graphql(guard = "WriteScope")]
    async fn reject_metadata_version(
        &self,
        ctx: &Context<'_>,
        id: ID,
        version_id: ID,
    ) -> async_graphql::Result<MetadataReview> {
        adult(ctx)?;
        let edition = target::<id::Manifestation>(&id)?;
        let look = ctx.look_ahead().field("metadata");
        let selection = if look.exists() {
            Some(library::snapshot_selection(&look)?)
        } else {
            None
        };
        let snapshot = editing::reject_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::VersionPayload {
                version_id: target::<id::MetadataVersion>(&version_id)?,
            },
            selection.as_ref(),
        )
        .await
        .map_err(safe_error)?;
        MetadataReview::new(edition, snapshot.as_ref())
    }
    #[graphql(guard = "WriteScope")]
    async fn revert_metadata_field(
        &self,
        ctx: &Context<'_>,
        id: ID,
        field_name: String,
        version_id: Option<ID>,
    ) -> async_graphql::Result<MetadataReview> {
        adult(ctx)?;
        let edition = target::<id::Manifestation>(&id)?;
        let look = ctx.look_ahead().field("metadata");
        let selection = if look.exists() {
            Some(library::snapshot_selection(&look)?)
        } else {
            None
        };
        let snapshot = editing::revert_manifestation(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::RevertPayload {
                field_name,
                version_id: version_id
                    .map(|value| target::<id::MetadataVersion>(&value))
                    .transpose()?,
            },
            selection.as_ref(),
        )
        .await
        .map_err(safe_error)?;
        MetadataReview::new(edition, snapshot.as_ref())
    }
    #[graphql(guard = "WriteScope")]
    async fn lock_metadata_field(
        &self,
        ctx: &Context<'_>,
        id: ID,
        field_name: String,
        entity_type: String,
    ) -> async_graphql::Result<MetadataReview> {
        adult(ctx)?;
        let edition = target::<id::Manifestation>(&id)?;
        let look = ctx.look_ahead().field("metadata");
        let selection = if look.exists() {
            Some(library::snapshot_selection(&look)?)
        } else {
            None
        };
        let snapshot = editing::lock_field(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::LockPayload {
                field_name,
                entity_type,
            },
            selection.as_ref(),
        )
        .await
        .map_err(safe_error)?;
        MetadataReview::new(edition, snapshot.as_ref())
    }
    #[graphql(guard = "WriteScope")]
    async fn unlock_metadata_field(
        &self,
        ctx: &Context<'_>,
        id: ID,
        field_name: String,
        entity_type: String,
    ) -> async_graphql::Result<MetadataReview> {
        adult(ctx)?;
        let edition = target::<id::Manifestation>(&id)?;
        let look = ctx.look_ahead().field("metadata");
        let selection = if look.exists() {
            Some(library::snapshot_selection(&look)?)
        } else {
            None
        };
        let snapshot = editing::unlock_field(
            ctx.data()?,
            ctx.data()?,
            edition,
            editing::LockPayload {
                field_name,
                entity_type,
            },
            selection.as_ref(),
        )
        .await
        .map_err(safe_error)?;
        MetadataReview::new(edition, snapshot.as_ref())
    }
    #[graphql(guard = "WriteScope")]
    async fn undo_metadata(
        &self,
        ctx: &Context<'_>,
        id: ID,
        baseline: Option<String>,
        operation_id: String,
    ) -> async_graphql::Result<MetadataOperation> {
        adult(ctx)?;
        undo::session_id(ctx.data()?, ctx.data()?).map_err(safe_error)?;
        let supplied = self::baseline(baseline)?;
        let selection = library::snapshot_selection(&ctx.look_ahead().field("metadata"))?;
        MetadataOperation::new(matched(
            undo::undo_metadata(
                ctx.data()?,
                ctx.data()?,
                ctx.data()?,
                target::<id::Manifestation>(&id)?,
                supplied,
                undo::UndoRequest {
                    operation_id: uuid(&operation_id)?,
                },
                &selection,
            )
            .await
            .map_err(safe_error)?,
        )?)
    }
}
pub(super) async fn metadata_operation(
    ctx: &Context<'_>,
    id: ID,
    request_id: Option<String>,
    operation_id: Option<String>,
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
                request_id: request_id.map(|value| uuid(&value)).transpose()?,
                operation_id: operation_id.map(|value| uuid(&value)).transpose()?,
            },
            &selection,
        )
        .await
        .map_err(safe_error)?,
    )
}
