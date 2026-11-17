//! Write-scoped adapters for the canonical editing operations.

use super::{GraphResult, RequestContext, id, library, safe_error, safe_error_with};
use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::error::AppError;
use crate::routes::etag::{Conditional, StrongEntityTag, parse_strong_entity_tag};
use crate::services::{
    metadata::editing::{self, SnapshotSelection, undo},
    reading, shelves,
};
use juniper::{
    DefaultScalarValue, Executor, GraphQLEnum, GraphQLInputObject, GraphQLObject, ID,
    LookAheadSelection, Nullable, Value, graphql_object,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Root mutations, each requiring write scope.
pub struct Mutation;

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct LibraryPreferenceDefaults {
    hidden_columns: Vec<String>,
    density: String,
    view: String,
    sort_stack: String,
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
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

#[derive(Clone, Copy, GraphQLEnum)]
enum LibraryDensityInput {
    Comfortable,
    Compact,
}
impl From<LibraryDensityInput> for crate::models::user_preferences::LibraryDensity {
    fn from(value: LibraryDensityInput) -> Self {
        match value {
            LibraryDensityInput::Comfortable => Self::Comfortable,
            LibraryDensityInput::Compact => Self::Compact,
        }
    }
}
#[derive(Clone, Copy, GraphQLEnum)]
enum LibraryViewInput {
    Grid,
    Table,
}
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

#[derive(GraphQLInputObject)]
struct LibraryPreferencesPatch {
    hidden_columns: Nullable<Vec<String>>,
    density: Nullable<LibraryDensityInput>,
    view: Nullable<LibraryViewInput>,
    sort_stack: Nullable<String>,
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
struct MetadataRefresh {
    manifestation_id: ID,
}

#[derive(Clone, Copy, GraphQLEnum)]
enum ContentRatingInput {
    Everyone,
    Teen,
    Mature,
    Adult,
    Explicit,
}
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
#[derive(Clone, Copy, GraphQLEnum)]
enum ReadingStatusInput {
    WantToRead,
    Reading,
    OnHold,
    Finished,
    Abandoned,
}
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
#[derive(GraphQLInputObject)]
struct ContributorPatch {
    author: Nullable<Vec<String>>,
    editor: Nullable<Vec<String>>,
    translator: Nullable<Vec<String>>,
}
impl From<ContributorPatch> for editing::ContributorsPatch {
    fn from(value: ContributorPatch) -> Self {
        Self {
            author: value.author.explicit(),
            editor: value.editor.explicit(),
            translator: value.translator.explicit(),
        }
    }
}
#[derive(GraphQLInputObject)]
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
#[derive(GraphQLInputObject)]
struct MetadataPatch {
    title: Nullable<String>,
    subtitle: Nullable<String>,
    description: Nullable<String>,
    language: Nullable<String>,
    publisher: Nullable<String>,
    publication_date: Nullable<String>,
    isbn_10: Nullable<String>,
    isbn_13: Nullable<String>,
    pages: Nullable<i32>,
    content_rating: Nullable<ContentRatingInput>,
    genres: Nullable<Vec<String>>,
    moods: Nullable<Vec<String>>,
    tags: Nullable<Vec<String>>,
    contributors: Nullable<ContributorPatch>,
    work_identifiers: Option<Vec<IdentifierPatch>>,
    manifestation_identifiers: Option<Vec<IdentifierPatch>>,
}
impl MetadataPatch {
    fn domain(self, request_id: Option<String>) -> GraphResult<editing::UpdateMetadataFields> {
        Ok(editing::UpdateMetadataFields {
            request_id: request_id.map(|value| uuid(&value)).transpose()?,
            title: self.title.explicit(),
            subtitle: self.subtitle.explicit(),
            description: self.description.explicit(),
            language: self.language.explicit(),
            publisher: self.publisher.explicit(),
            pub_date: self.publication_date.explicit(),
            isbn_10: self.isbn_10.explicit(),
            isbn_13: self.isbn_13.explicit(),
            pages: self.pages.explicit(),
            content_rating: self.content_rating.map(Into::into).explicit(),
            genres: self.genres.explicit(),
            moods: self.moods.explicit(),
            tags: self.tags.explicit(),
            contributors: self.contributors.map(Into::into).explicit(),
            work_identifiers: self.work_identifiers.map(identifiers),
            manifestation_identifiers: self.manifestation_identifiers.map(identifiers),
        })
    }
}
#[derive(GraphQLInputObject)]
struct ReadingPatch {
    status: Nullable<ReadingStatusInput>,
    rating: Nullable<i32>,
    notes: Nullable<String>,
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
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
            expires_at: value
                .expires_at
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
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
#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
struct MetadataSave {
    request_id: Option<String>,
    operations: Vec<FieldOperation>,
    metadata: library::MetadataSnapshot,
}
#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct MetadataOperation {
    outcome: String,
    operations: Vec<FieldOperation>,
    metadata: library::MetadataSnapshot,
}
impl MetadataOperation {
    fn new(result: undo::OperationResult) -> GraphResult<Self> {
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
#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
struct MetadataReview {
    manifestation_id: ID,
    metadata: Option<library::MetadataSnapshot>,
}
impl MetadataReview {
    fn new(edition: Uuid, snapshot: Option<&editing::MetadataSnapshot>) -> GraphResult<Self> {
        Ok(Self {
            manifestation_id: id::encode("Manifestation", edition),
            metadata: snapshot
                .map(library::MetadataSnapshot::from_domain)
                .transpose()?,
        })
    }
}
#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
struct ShelfChange {
    shelf_id: ID,
    manifestation_id: Option<ID>,
}

fn target(kind: &str, value: &ID) -> GraphResult<Uuid> {
    id::decode(kind, value)
        .ok_or_else(|| safe_error(AppError::Validation(format!("Invalid {kind} ID."))))
}
fn uuid(value: &str) -> GraphResult<Uuid> {
    value.parse().map_err(|_| {
        safe_error(AppError::Validation(
            "Invalid operation or request UUID.".into(),
        ))
    })
}
fn baseline(value: Option<String>) -> GraphResult<Option<StrongEntityTag>> {
    Ok(value
        .map(|value| parse_strong_entity_tag(value.as_bytes()))
        .transpose()?)
}
fn matched<T>(value: Conditional<T>) -> GraphResult<T> {
    match value {
        Conditional::Matched(value) => Ok(value),
        Conditional::Mismatch(tag) => {
            let current = tag
                .to_str()
                .map_err(|e| AppError::Internal(e.into()))?
                .to_owned();
            Err(safe_error_with(
                AppError::IfMatchMismatch,
                vec![("currentEtag", Value::scalar(current))],
            ))
        }
    }
}
fn adult(context: &RequestContext) -> GraphResult<&CurrentUser> {
    // THREAT: scope and child status are separate checks; write scope never admits a child to shared edits.
    let user = context.require(Scope::Write)?;
    user.require_not_child()?;
    Ok(user)
}
fn metadata_fields<'a>(
    executor: &'a Executor<'_, '_, RequestContext>,
) -> Vec<LookAheadSelection<'a, DefaultScalarValue>> {
    executor
        .look_ahead()
        .children()
        .iter()
        .filter(|field| field.field_original_name() == "metadata")
        .copied()
        .collect()
}
fn selection(executor: &Executor<'_, '_, RequestContext>) -> GraphResult<SnapshotSelection> {
    library::snapshot_selection(
        metadata_fields(executor)
            .into_iter()
            .flat_map(|field| field.children()),
    )
}
fn optional_selection(
    executor: &Executor<'_, '_, RequestContext>,
) -> GraphResult<Option<SnapshotSelection>> {
    if metadata_fields(executor).is_empty() {
        return Ok(None);
    }
    selection(executor).map(Some)
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
impl Mutation {
    async fn update_library_preferences(
        context: &RequestContext,
        patch: LibraryPreferencesPatch,
    ) -> GraphResult<LibraryPreferences> {
        let user = context.require(Scope::Write)?;
        let patch = crate::services::preferences::UpdatePreferencesRequest {
            hidden_columns: patch.hidden_columns.explicit(),
            density: patch.density.map(Into::into).explicit(),
            view: patch.view.map(Into::into).explicit(),
            sort_stack: patch.sort_stack.explicit(),
        };
        Ok(
            crate::services::preferences::update_preferences(&context.state.pool, user, patch)
                .await?
                .into(),
        )
    }

    async fn refresh_metadata(context: &RequestContext, id: ID) -> GraphResult<MetadataRefresh> {
        let user = adult(context)?;
        let mut tx = crate::db::acquire_with_rls(&context.state.pool, user.user_id)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        let changed = crate::services::enrichment::reset_retries(
            &mut tx,
            &[target("Manifestation", &id)?],
            crate::services::enrichment::RetryTiming::ClearProviderDelay,
        )
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
        if changed == 0 {
            return Err(safe_error(AppError::NotFound));
        }
        tx.commit()
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        Ok(MetadataRefresh {
            manifestation_id: id,
        })
    }

    async fn update_metadata(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        baseline: Option<String>,
        request_id: Option<String>,
        patch: MetadataPatch,
    ) -> GraphResult<MetadataSave> {
        let user = adult(context)?;
        let supplied = self::baseline(baseline)?;
        let selection = selection(executor)?;
        let result = matched(
            editing::update_book_metadata(
                user,
                &context.state,
                &context.session,
                target("Manifestation", &id)?,
                supplied,
                patch.domain(request_id)?,
                &selection,
            )
            .await?,
        )?;
        Ok(MetadataSave {
            request_id: result.request_id.map(|id| id.to_string()),
            operations: result.operations.into_iter().map(Into::into).collect(),
            metadata: library::MetadataSnapshot::from_domain(&result.snapshot)?,
        })
    }

    async fn update_reading_state(
        context: &RequestContext,
        id: ID,
        baseline: Option<String>,
        patch: ReadingPatch,
    ) -> GraphResult<library::ReadingState> {
        let user = context.require(Scope::Write)?;
        let supplied = self::baseline(baseline)?;
        let rating = match patch.rating {
            Nullable::Some(value) => Nullable::Some(library::small_int(value)?),
            Nullable::ExplicitNull => Nullable::ExplicitNull,
            Nullable::ImplicitNull => Nullable::ImplicitNull,
        };
        let row = matched(
            reading::patch_reading(
                user,
                &context.state,
                target("Manifestation", &id)?,
                supplied,
                reading::UpdateReadingRequest {
                    status: patch.status.map(Into::into).explicit(),
                    rating: rating.explicit(),
                    notes: patch.notes.explicit(),
                },
            )
            .await?,
        )?;
        library::ReadingState::from_row(&row)
    }

    async fn create_shelf(context: &RequestContext, name: String) -> GraphResult<library::Shelf> {
        let user = context.require(Scope::Write)?;
        library::Shelf::from_model(shelves::create_shelf(user, &context.state, &name).await?)
    }

    async fn rename_shelf(
        context: &RequestContext,
        id: ID,
        name: String,
    ) -> GraphResult<library::Shelf> {
        let user = context.require(Scope::Write)?;
        library::Shelf::from_model(
            shelves::rename_shelf(user, &context.state, target("Shelf", &id)?, &name).await?,
        )
    }

    async fn delete_shelf(context: &RequestContext, id: ID) -> GraphResult<ShelfChange> {
        let user = context.require(Scope::Write)?;
        shelves::delete_shelf(user, &context.state, target("Shelf", &id)?).await?;
        Ok(ShelfChange {
            shelf_id: id,
            manifestation_id: None,
        })
    }

    async fn add_shelf_item(
        context: &RequestContext,
        shelf_id: ID,
        manifestation_id: ID,
    ) -> GraphResult<ShelfChange> {
        let user = context.require(Scope::Write)?;
        shelves::add_shelf_item(
            user,
            &context.state,
            target("Shelf", &shelf_id)?,
            target("Manifestation", &manifestation_id)?,
        )
        .await?;
        Ok(ShelfChange {
            shelf_id,
            manifestation_id: Some(manifestation_id),
        })
    }

    async fn remove_shelf_item(
        context: &RequestContext,
        shelf_id: ID,
        manifestation_id: ID,
    ) -> GraphResult<ShelfChange> {
        let user = context.require(Scope::Write)?;
        shelves::remove_shelf_item(
            user,
            &context.state,
            target("Shelf", &shelf_id)?,
            target("Manifestation", &manifestation_id)?,
        )
        .await?;
        Ok(ShelfChange {
            shelf_id,
            manifestation_id: Some(manifestation_id),
        })
    }

    async fn accept_metadata_version(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        version_id: ID,
    ) -> GraphResult<MetadataReview> {
        let user = adult(context)?;
        let edition = target("Manifestation", &id)?;
        let selection = optional_selection(executor)?;
        let snapshot = editing::accept_manifestation(
            user,
            &context.state,
            edition,
            editing::VersionPayload {
                version_id: target("MetadataVersion", &version_id)?,
            },
            selection.as_ref(),
        )
        .await?;
        MetadataReview::new(edition, snapshot.as_ref())
    }

    async fn reject_metadata_version(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        version_id: ID,
    ) -> GraphResult<MetadataReview> {
        let user = adult(context)?;
        let edition = target("Manifestation", &id)?;
        let selection = optional_selection(executor)?;
        let snapshot = editing::reject_manifestation(
            user,
            &context.state,
            edition,
            editing::VersionPayload {
                version_id: target("MetadataVersion", &version_id)?,
            },
            selection.as_ref(),
        )
        .await?;
        MetadataReview::new(edition, snapshot.as_ref())
    }

    async fn revert_metadata_field(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        field_name: String,
        version_id: Option<ID>,
    ) -> GraphResult<MetadataReview> {
        let user = adult(context)?;
        let edition = target("Manifestation", &id)?;
        let selection = optional_selection(executor)?;
        let snapshot = editing::revert_manifestation(
            user,
            &context.state,
            edition,
            editing::RevertPayload {
                field_name,
                version_id: version_id
                    .map(|value| target("MetadataVersion", &value))
                    .transpose()?,
            },
            selection.as_ref(),
        )
        .await?;
        MetadataReview::new(edition, snapshot.as_ref())
    }

    async fn lock_metadata_field(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        field_name: String,
        entity_type: String,
    ) -> GraphResult<MetadataReview> {
        let user = adult(context)?;
        let edition = target("Manifestation", &id)?;
        let selection = optional_selection(executor)?;
        let snapshot = editing::lock_field(
            user,
            &context.state,
            edition,
            editing::LockPayload {
                field_name,
                entity_type,
            },
            selection.as_ref(),
        )
        .await?;
        MetadataReview::new(edition, snapshot.as_ref())
    }

    async fn unlock_metadata_field(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        field_name: String,
        entity_type: String,
    ) -> GraphResult<MetadataReview> {
        let user = adult(context)?;
        let edition = target("Manifestation", &id)?;
        let selection = optional_selection(executor)?;
        let snapshot = editing::unlock_field(
            user,
            &context.state,
            edition,
            editing::LockPayload {
                field_name,
                entity_type,
            },
            selection.as_ref(),
        )
        .await?;
        MetadataReview::new(edition, snapshot.as_ref())
    }

    async fn undo_metadata(
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
        id: ID,
        baseline: Option<String>,
        operation_id: String,
    ) -> GraphResult<MetadataOperation> {
        let user = adult(context)?;
        undo::session_id(user, &context.session)?;
        let supplied = self::baseline(baseline)?;
        let selection = selection(executor)?;
        MetadataOperation::new(matched(
            undo::undo_metadata(
                user,
                &context.state,
                &context.session,
                target("Manifestation", &id)?,
                supplied,
                undo::UndoRequest {
                    operation_id: uuid(&operation_id)?,
                },
                &selection,
            )
            .await?,
        )?)
    }
}

pub(super) async fn metadata_operation(
    context: &RequestContext,
    executor: &Executor<'_, '_, RequestContext>,
    id: &ID,
    request_id: Option<String>,
    operation_id: Option<String>,
) -> GraphResult<MetadataOperation> {
    let user = adult(context)?;
    let selection = selection(executor)?;
    MetadataOperation::new(
        undo::resolve_metadata(
            user,
            &context.state,
            &context.session,
            target("Manifestation", id)?,
            undo::ResolveRequest {
                request_id: request_id.map(|value| uuid(&value)).transpose()?,
                operation_id: operation_id.map(|value| uuid(&value)).transpose()?,
            },
            &selection,
        )
        .await?,
    )
}
