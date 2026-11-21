use super::id::Kind as _;
use super::loaders::{RequestLoader, SeriesKey, ShelfKey, WorkKey, batch_error};
use super::{Viewer, id, safe_error};
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::routes::cursor::CursorApi;
use crate::routes::etag::MetadataRevision as Revision;
use crate::routes::library::ListParams;
use crate::services::metadata::editing::{self, SnapshotKey, SnapshotPage, SnapshotSelection};
use crate::services::reader::{
    self, BookRecord, RelationKey, RelationKind, RelationPage, RelationRow, SeriesRecord,
    ShelfRecord, WorkRecord,
};
use crate::state::AppState;
use async_graphql::connection::{Connection, Edge, EmptyFields};
use async_graphql::{ComplexObject, Context, ID, InputObject, Interface, SimpleObject};
use jiff::Timestamp;

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// A strong RFC 9110 entity-tag in its quoted wire form.
#[derive(Clone, Debug)]
pub(super) struct ETag(String);

impl ETag {
    pub(super) fn from_wire(value: String) -> async_graphql::Result<Self> {
        crate::routes::etag::parse_strong_entity_tag(value.as_bytes()).map_err(|_| {
            safe_error(AppError::Internal(anyhow::anyhow!(
                "server produced a malformed entity-tag"
            )))
        })?;
        Ok(Self(value))
    }

    pub(super) fn strong(&self) -> async_graphql::Result<crate::routes::etag::StrongEntityTag> {
        crate::routes::etag::parse_strong_entity_tag(self.0.as_bytes()).map_err(safe_error)
    }
}

#[async_graphql::Scalar(
    name = "ETag",
    specified_by_url = "https://github.com/unkos-dev/reverie/blob/main/docs/specs/api/design/0031-graphql-execution-and-relay-browser-data.md#custom-scalars"
)]
impl async_graphql::ScalarType for ETag {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        match value {
            async_graphql::Value::String(text)
                if crate::routes::etag::parse_strong_entity_tag(text.as_bytes()).is_ok() =>
            {
                Ok(Self(text))
            }
            async_graphql::Value::String(_) => Err(async_graphql::InputValueError::custom(
                "Expected a strong entity-tag.",
            )),
            other => Err(async_graphql::InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::String(self.0.clone())
    }
}

/// A UUID in lower-case hyphenated form.
#[derive(Clone, Copy, Debug)]
pub(super) struct UuidValue(pub(super) Uuid);

#[async_graphql::Scalar(
    name = "UUID",
    specified_by_url = "https://github.com/unkos-dev/reverie/blob/main/docs/specs/api/design/0031-graphql-execution-and-relay-browser-data.md#custom-scalars"
)]
impl async_graphql::ScalarType for UuidValue {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        match value {
            async_graphql::Value::String(text) if text.len() == 36 => Uuid::try_parse(&text)
                .map(Self)
                .map_err(|_| async_graphql::InputValueError::custom("Expected a hyphenated UUID.")),
            async_graphql::Value::String(_) => Err(async_graphql::InputValueError::custom(
                "Expected a hyphenated UUID.",
            )),
            other => Err(async_graphql::InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::String(self.0.hyphenated().to_string())
    }
}

/// An instant as RFC 3339 text in UTC with the `Z` designator.
#[derive(Clone, Copy, Debug)]
pub(super) struct DateTime(pub(super) Timestamp);

#[async_graphql::Scalar(
    name = "DateTime",
    specified_by_url = "https://github.com/unkos-dev/reverie/blob/main/docs/specs/api/design/0031-graphql-execution-and-relay-browser-data.md#custom-scalars"
)]
impl async_graphql::ScalarType for DateTime {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        match value {
            async_graphql::Value::String(text) if text.ends_with('Z') => {
                text.parse::<Timestamp>().map(Self).map_err(|_| {
                    async_graphql::InputValueError::custom("Expected an RFC 3339 instant.")
                })
            }
            async_graphql::Value::String(_) => Err(async_graphql::InputValueError::custom(
                "Expected an RFC 3339 instant in UTC.",
            )),
            other => Err(async_graphql::InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::String(self.0.to_string())
    }
}

#[derive(Interface)]
#[graphql(field(name = "id", ty = "&ID"))]
pub(super) enum Node {
    Work(Work),
    Manifestation(Manifestation),
    Shelf(Shelf),
    Series(Series),
    Viewer(Viewer),
}

#[derive(Clone, SimpleObject)]
#[graphql(complex, directive = super::fetchable::apply("id".to_owned()))]
pub(super) struct Work {
    id: ID,
    #[graphql(skip)]
    record: Arc<WorkRecord>,
}

impl Work {
    fn new(record: Arc<WorkRecord>) -> Self {
        Self {
            id: id::Work::encode(record.id),
            record,
        }
    }
}

#[ComplexObject]
impl Work {
    async fn title(&self) -> &str {
        &self.record.title
    }
    async fn subtitle(&self) -> Option<&str> {
        self.record.subtitle.as_deref()
    }
    async fn description(&self) -> Option<&str> {
        self.record.description.as_deref()
    }
    async fn language(&self) -> Option<&str> {
        self.record.language.as_deref()
    }
    async fn selected_series(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SelectedSeries>> {
        let Some(series_id) = self.record.series_id else {
            return Ok(None);
        };
        Ok(ctx
            .data::<RequestLoader>()?
            .load_one(SeriesKey(series_id))
            .await
            .map_err(|error| batch_error(&error))?
            .map(|record| SelectedSeries {
                node: Series::new(record),
                position: self.record.series_position,
            }))
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::WorkIdentifiers, first, child_complexity)?"
    )]
    async fn identifiers(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, ExternalIdentifier>> {
        term_connection(
            relation(
                ctx,
                RelationKind::WorkIdentifiers,
                self.record.id,
                first,
                after,
            )
            .await?,
            |row| {
                Ok(ExternalIdentifier {
                    level: "work".into(),
                    scheme: required(row.name)?,
                    value: required(row.value)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Manifestations, first, child_complexity)?"
    )]
    async fn manifestations(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Manifestation>> {
        let page = relation(
            ctx,
            RelationKind::Manifestations,
            self.record.id,
            first,
            after,
        )
        .await?;
        manifestation_connection(ctx, page).await
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Authors, first, child_complexity)?"
    )]
    async fn authors(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Author, EmptyFields, ContributorFields>> {
        contributor_connection(
            relation(ctx, RelationKind::Authors, self.record.id, first, after).await?,
            std::convert::identity,
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Contributors, first, child_complexity)?"
    )]
    async fn contributors(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Author, EmptyFields, ContributorFields>> {
        contributor_connection(
            relation(
                ctx,
                RelationKind::Contributors,
                self.record.id,
                first,
                after,
            )
            .await?,
            std::convert::identity,
        )
    }
}

#[derive(Clone, SimpleObject)]
#[graphql(complex, directive = super::fetchable::apply("id".to_owned()))]
pub(super) struct Manifestation {
    id: ID,
    #[graphql(skip)]
    pub(super) record: BookRecord,
    #[graphql(skip)]
    committed: Option<Arc<MetadataRevision>>,
}

impl Manifestation {
    pub(super) fn new(record: BookRecord) -> Self {
        Self {
            id: id::Manifestation::encode(record.detail.id),
            record,
            committed: None,
        }
    }

    /// Answers `metadata` with the revision a write projected inside its own transaction, so a mutation
    /// acknowledgement pairs its values with the validator that write produced.
    pub(super) fn with_metadata(mut self, revision: MetadataRevision) -> Self {
        self.committed = Some(Arc::new(revision));
        self
    }
}

#[ComplexObject]
impl Manifestation {
    async fn versions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<super::versions::MetadataVersions> {
        super::versions::resolve(ctx, self.record.detail.id).await
    }
    async fn work(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Work>> {
        Ok(ctx
            .data::<RequestLoader>()?
            .load_one(WorkKey(self.record.detail.work_id))
            .await
            .map_err(|error| batch_error(&error))?
            .map(Work::new))
    }
    async fn isbn_13(&self) -> Option<&str> {
        self.record.detail.isbn_13.as_deref()
    }
    async fn isbn_10(&self) -> Option<&str> {
        self.record.detail.isbn_10.as_deref()
    }
    async fn isbn_mismatch(&self) -> bool {
        self.record.detail.isbn_mismatch()
    }
    async fn pages(&self) -> Option<i32> {
        self.record.detail.pages
    }
    async fn publisher(&self) -> Option<&str> {
        self.record.detail.publisher.as_deref()
    }
    async fn publication_date(&self) -> Option<&str> {
        self.record.detail.pub_date.as_deref()
    }
    async fn created_at(&self) -> String {
        self.record.detail.created_at.to_string()
    }
    async fn updated_at(&self) -> String {
        self.record.detail.updated_at.to_string()
    }
    async fn cover_url(&self) -> String {
        format!("/api/v1/books/{}/cover/thumb", self.record.detail.id)
    }
    async fn content_rating(&self) -> Option<&'static str> {
        self.record
            .detail
            .content_rating
            .map(crate::models::content_rating::ContentRating::as_str)
    }
    async fn ingestion_status(&self) -> &str {
        &self.record.detail.ingestion_status
    }
    async fn validation_status(&self) -> &'static str {
        self.record.detail.validation_status.as_str()
    }
    async fn enrichment_status(&self) -> &str {
        &self.record.detail.enrichment_status
    }
    async fn reading_state(&self) -> ReadingState {
        let row = &self.record.reading;
        ReadingState {
            etag: self.record.reading_etag.clone(),
            status: row.status.map(|value| value.as_str().to_owned()),
            rating: row.rating.map(i32::from),
            notes: row.notes.clone(),
            progress_pct: row.progress_pct.map(f64::from),
            started_at: row.started_at.map(|value| timestamp(value.to_jiff())),
            finished_at: row.finished_at.map(|value| timestamp(value.to_jiff())),
            last_read_at: row.last_read_at.map(|value| timestamp(value.to_jiff())),
        }
    }
    async fn metadata(&self, ctx: &Context<'_>) -> async_graphql::Result<Arc<MetadataRevision>> {
        if let Some(committed) = &self.committed {
            return Ok(Arc::clone(committed));
        }
        let pages = snapshot_pages(&ctx.look_ahead())?;
        if !pages.is_empty() {
            let snapshot = required(
                ctx.data::<RequestLoader>()?
                    .load_one(SnapshotKey {
                        manifestation_id: self.record.detail.id,
                        pages,
                    })
                    .await
                    .map_err(|error| batch_error(&error))?,
            )?;
            return MetadataRevision::from_domain(&snapshot).map(Arc::new);
        }
        let row = &self.record.detail;
        let revision = Revision {
            work_id: row.work_id,
            manifestation_id: row.id,
            work_revision: row.work_revision,
            manifestation_revision: row.manifestation_revision,
        };
        Ok(Arc::new(MetadataRevision {
            id: id::metadata_revision(&revision).map_err(safe_error)?,
            revision,
            relation_pages: HashMap::new(),
            isbn_mismatch: row.isbn_mismatch(),
            work_id: id::Work::encode(row.work_id),
            manifestation_id: id::Manifestation::encode(row.id),
            etag: ETag::from_wire(self.record.metadata_etag.clone())?,
            title: row.title.clone(),
            subtitle: row.subtitle.clone(),
            description: row.description.clone(),
            language: row.language.clone(),
            isbn_10: row.isbn_10.clone(),
            isbn_13: row.isbn_13.clone(),
            pages: row.pages,
            publisher: row.publisher.clone(),
            publication_date: row.pub_date.clone(),
            content_rating: row.content_rating.map(|value| value.as_str().to_owned()),
        }))
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Genres, first, child_complexity)?"
    )]
    async fn genres(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Genre>> {
        term_connection(
            relation(
                ctx,
                RelationKind::Genres,
                self.record.detail.id,
                first,
                after,
            )
            .await?,
            |row| {
                Ok(Genre {
                    id: id::Genre::encode(required(row.id)?),
                    name: required(row.name)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Moods, first, child_complexity)?"
    )]
    async fn moods(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Mood>> {
        term_connection(
            relation(
                ctx,
                RelationKind::Moods,
                self.record.detail.id,
                first,
                after,
            )
            .await?,
            |row| {
                Ok(Mood {
                    id: id::Mood::encode(required(row.id)?),
                    name: required(row.name)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Tags, first, child_complexity)?"
    )]
    async fn tags(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Tag>> {
        term_connection(
            relation(ctx, RelationKind::Tags, self.record.detail.id, first, after).await?,
            |row| {
                Ok(Tag {
                    id: id::Tag::encode(required(row.id)?),
                    name: required(row.name)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Identifiers, first, child_complexity)?"
    )]
    async fn identifiers(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, ExternalIdentifier>> {
        term_connection(
            relation(
                ctx,
                RelationKind::Identifiers,
                self.record.detail.id,
                first,
                after,
            )
            .await?,
            |row| {
                Ok(ExternalIdentifier {
                    level: if required(row.level)? == 0 {
                        "work".into()
                    } else {
                        "manifestation".into()
                    },
                    scheme: required(row.name)?,
                    value: required(row.value)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Ratings, first, child_complexity)?"
    )]
    async fn ratings(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, ExternalRating>> {
        term_connection(
            relation(
                ctx,
                RelationKind::Ratings,
                self.record.detail.id,
                first,
                after,
            )
            .await?,
            |row| {
                Ok(ExternalRating {
                    source: required(row.name)?,
                    rating: required(row.rating)?,
                    scale: required(row.scale)?,
                    review_count: row.reviews,
                    fetched_at: row.created_at.map(|value| timestamp(value.to_jiff())),
                })
            },
        )
    }
}

#[derive(SimpleObject)]
pub(super) struct ReadingState {
    etag: String,
    status: Option<String>,
    rating: Option<i32>,
    notes: Option<String>,
    progress_pct: Option<f64>,
    started_at: Option<String>,
    finished_at: Option<String>,
    last_read_at: Option<String>,
}
impl ReadingState {
    pub(super) fn from_row(
        row: &crate::services::reading::ReadingStateRow,
    ) -> async_graphql::Result<Self> {
        Ok(Self {
            etag: row
                .etag()
                .map_err(safe_error)?
                .to_str()
                .map_err(|e| safe_error(AppError::Internal(e.into())))?
                .to_owned(),
            status: row.status.map(|v| v.as_str().to_owned()),
            rating: row.rating.map(i32::from),
            notes: row.notes.clone(),
            progress_pct: row.progress_pct.map(f64::from),
            started_at: row.started_at.map(|value| timestamp(value.to_jiff())),
            finished_at: row.finished_at.map(|value| timestamp(value.to_jiff())),
            last_read_at: row.last_read_at.map(|value| timestamp(value.to_jiff())),
        })
    }
}
#[derive(SimpleObject)]
#[graphql(complex)]
pub(super) struct MetadataRevision {
    id: ID,
    #[graphql(skip)]
    revision: Revision,
    #[graphql(skip)]
    relation_pages: HashMap<RelationKey, RelationPage>,
    isbn_mismatch: bool,
    work_id: ID,
    manifestation_id: ID,
    etag: ETag,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    language: Option<String>,
    isbn_10: Option<String>,
    isbn_13: Option<String>,
    pages: Option<i32>,
    publisher: Option<String>,
    publication_date: Option<String>,
    content_rating: Option<String>,
}
pub(super) fn snapshot_pages(
    look: &async_graphql::Lookahead<'_>,
) -> async_graphql::Result<Vec<SnapshotPage>> {
    let mut pages = Vec::new();
    for root in look.selection_fields() {
        for field in root.selection_set() {
            let kind = match field.name() {
                "authors" => RelationKind::Authors,
                "contributors" => RelationKind::Contributors,
                "genres" => RelationKind::Genres,
                "moods" => RelationKind::Moods,
                "tags" => RelationKind::Tags,
                "workIdentifiers" => RelationKind::WorkIdentifiers,
                "manifestationIdentifiers" => RelationKind::Identifiers,
                _ => continue,
            };
            let arguments = field.arguments()?;
            let first = arguments
                .iter()
                .find(|(name, _)| name == "first")
                .and_then(|(_, value)| match value {
                    async_graphql::Value::Number(n) => {
                        n.as_i64().and_then(|v| i32::try_from(v).ok())
                    }
                    _ => None,
                })
                .ok_or_else(|| safe_error(AppError::Validation("Missing page size.".into())))?;
            let after = arguments.iter().find(|(name, _)| name == "after").and_then(
                |(_, value)| match value {
                    async_graphql::Value::String(value) => Some(value.clone()),
                    _ => None,
                },
            );
            let page = SnapshotPage {
                kind,
                first: super::complexity::nested_size(kind, first).map_err(safe_error)?,
                after,
            };
            if !pages.contains(&page) {
                pages.push(page);
            }
        }
    }
    Ok(pages)
}
pub(super) fn snapshot_selection(
    look: &async_graphql::Lookahead<'_>,
) -> async_graphql::Result<SnapshotSelection> {
    snapshot_pages(look).map(SnapshotSelection::Selected)
}
impl MetadataRevision {
    pub(super) fn from_domain(snapshot: &editing::MetadataSnapshot) -> async_graphql::Result<Self> {
        let row = &snapshot.values;
        let revision = Revision {
            work_id: snapshot.revision.work_id,
            manifestation_id: snapshot.revision.manifestation_id,
            work_revision: snapshot.revision.work_revision,
            manifestation_revision: snapshot.revision.manifestation_revision,
        };
        Ok(Self {
            id: id::metadata_revision(&revision).map_err(safe_error)?,
            revision,
            relation_pages: snapshot.pages.clone(),
            work_id: id::Work::encode(snapshot.revision.work_id),
            manifestation_id: id::Manifestation::encode(snapshot.revision.manifestation_id),
            etag: ETag::from_wire(
                snapshot
                    .revision
                    .etag()
                    .map_err(safe_error)?
                    .to_str()
                    .map_err(|e| safe_error(AppError::Internal(e.into())))?
                    .to_owned(),
            )?,
            title: row.title.clone(),
            subtitle: row.subtitle.clone(),
            description: row.description.clone(),
            language: row.language.clone(),
            isbn_10: row.isbn_10.clone(),
            isbn_13: row.isbn_13.clone(),
            isbn_mismatch: row.isbn_mismatch,
            pages: row.pages,
            publisher: row.publisher.clone(),
            publication_date: row.pub_date.clone(),
            content_rating: row.content_rating.map(|v| v.as_str().to_owned()),
        })
    }
    fn page(
        &self,
        kind: RelationKind,
        first: i32,
        after: Option<String>,
    ) -> impl std::future::Future<Output = async_graphql::Result<RelationPage>> {
        std::future::ready((|| {
            let key = SnapshotPage {
                kind,
                first: super::complexity::nested_size(kind, first).map_err(safe_error)?,
                after,
            }
            .key(&self.revision);
            self.relation_pages.get(&key).cloned().ok_or_else(|| {
                safe_error(AppError::Internal(anyhow::anyhow!(
                    "Snapshot page was not materialised"
                )))
            })
        })())
    }
}
#[ComplexObject]
impl MetadataRevision {
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Authors, first, child_complexity)?"
    )]
    async fn authors(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<
        Connection<String, RevisionContributor, EmptyFields, ContributorFields>,
    > {
        contributor_connection(
            self.page(RelationKind::Authors, first, after).await?,
            RevisionContributor::new,
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Contributors, first, child_complexity)?"
    )]
    async fn contributors(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<
        Connection<String, RevisionContributor, EmptyFields, ContributorFields>,
    > {
        contributor_connection(
            self.page(RelationKind::Contributors, first, after).await?,
            RevisionContributor::new,
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Genres, first, child_complexity)?"
    )]
    async fn genres(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, RevisionGenre>> {
        term_connection(
            self.page(RelationKind::Genres, first, after).await?,
            |row| {
                let name = required(row.name)?;
                Ok(RevisionGenre {
                    genre: Genre {
                        id: id::Genre::encode(required(row.id)?),
                        name: name.clone(),
                    },
                    name,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Moods, first, child_complexity)?"
    )]
    async fn moods(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, RevisionMood>> {
        term_connection(self.page(RelationKind::Moods, first, after).await?, |row| {
            let name = required(row.name)?;
            Ok(RevisionMood {
                mood: Mood {
                    id: id::Mood::encode(required(row.id)?),
                    name: name.clone(),
                },
                name,
            })
        })
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Tags, first, child_complexity)?"
    )]
    async fn tags(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, RevisionTag>> {
        term_connection(self.page(RelationKind::Tags, first, after).await?, |row| {
            let name = required(row.name)?;
            Ok(RevisionTag {
                tag: Tag {
                    id: id::Tag::encode(required(row.id)?),
                    name: name.clone(),
                },
                name,
            })
        })
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::WorkIdentifiers, first, child_complexity)?"
    )]
    async fn work_identifiers(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, ExternalIdentifier>> {
        term_connection(
            self.page(RelationKind::WorkIdentifiers, first, after)
                .await?,
            |row| {
                Ok(ExternalIdentifier {
                    level: "work".to_owned(),
                    scheme: required(row.name)?,
                    value: required(row.value)?,
                })
            },
        )
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::Identifiers, first, child_complexity)?"
    )]
    async fn manifestation_identifiers(
        &self,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, ExternalIdentifier>> {
        term_connection(
            self.page(RelationKind::Identifiers, first, after).await?,
            |row| {
                Ok(ExternalIdentifier {
                    level: "manifestation".to_owned(),
                    scheme: required(row.name)?,
                    value: required(row.value)?,
                })
            },
        )
    }
}
#[derive(SimpleObject)]
pub(super) struct RevisionContributor {
    name: String,
    author: Author,
}
impl RevisionContributor {
    fn new(author: Author) -> Self {
        Self {
            name: author.name.clone(),
            author,
        }
    }
}
#[derive(SimpleObject)]
pub(super) struct RevisionGenre {
    name: String,
    genre: Genre,
}
#[derive(SimpleObject)]
pub(super) struct RevisionMood {
    name: String,
    mood: Mood,
}
#[derive(SimpleObject)]
pub(super) struct RevisionTag {
    name: String,
    tag: Tag,
}
#[derive(SimpleObject)]
pub(super) struct SelectedSeries {
    node: Series,
    position: Option<f64>,
}
#[derive(SimpleObject)]
pub(super) struct Author {
    id: ID,
    name: String,
}
#[derive(SimpleObject)]
pub(super) struct ContributorFields {
    role: String,
    position: i32,
}
#[derive(SimpleObject)]
pub(super) struct MembershipFields {
    position: Option<f64>,
}
#[derive(SimpleObject)]
pub(super) struct Genre {
    id: ID,
    name: String,
}
#[derive(SimpleObject)]
pub(super) struct Mood {
    id: ID,
    name: String,
}
#[derive(SimpleObject)]
pub(super) struct Tag {
    id: ID,
    name: String,
}
#[derive(SimpleObject)]
pub(super) struct ExternalIdentifier {
    level: String,
    scheme: String,
    value: String,
}
#[derive(SimpleObject)]
pub(super) struct ExternalRating {
    source: String,
    rating: f64,
    scale: f64,
    review_count: Option<i32>,
    fetched_at: Option<String>,
}

#[derive(Clone, SimpleObject)]
#[graphql(complex, directive = super::fetchable::apply("id".to_owned()))]
pub(super) struct Shelf {
    id: ID,
    item_count: i32,
    #[graphql(skip)]
    record: Arc<ShelfRecord>,
}
impl Shelf {
    pub(super) fn from_model(row: crate::models::shelf::Shelf) -> async_graphql::Result<Self> {
        Self::new(Arc::new(ShelfRecord {
            id: row.id,
            name: row.name,
            is_system: row.is_system,
            created_at: row.created_at,
            updated_at: row.updated_at,
            item_count: row.item_count,
        }))
    }
    fn new(record: Arc<ShelfRecord>) -> async_graphql::Result<Self> {
        Ok(Self {
            id: id::Shelf::encode(record.id),
            item_count: i32::try_from(record.item_count)
                .map_err(|e| safe_error(AppError::Internal(e.into())))?,
            record,
        })
    }
}
#[ComplexObject]
impl Shelf {
    async fn name(&self) -> &str {
        &self.record.name
    }
    async fn is_system(&self) -> bool {
        self.record.is_system
    }
    async fn created_at(&self) -> String {
        self.record.created_at.to_string()
    }
    async fn updated_at(&self) -> String {
        self.record.updated_at.to_string()
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::ShelfItems, first, child_complexity)?"
    )]
    async fn items(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Manifestation>> {
        manifestation_connection(
            ctx,
            relation(ctx, RelationKind::ShelfItems, self.record.id, first, after).await?,
        )
        .await
    }
}

#[derive(Clone, SimpleObject)]
#[graphql(complex, directive = super::fetchable::apply("id".to_owned()))]
pub(super) struct Series {
    id: ID,
    #[graphql(skip)]
    record: Arc<SeriesRecord>,
}
impl Series {
    fn new(record: Arc<SeriesRecord>) -> Self {
        Self {
            id: id::Series::encode(record.id),
            record,
        }
    }
}
#[ComplexObject]
impl Series {
    async fn name(&self) -> &str {
        &self.record.name
    }
    async fn sort_name(&self) -> &str {
        &self.record.sort_name
    }
    #[graphql(
        complexity = "super::complexity::nested(RelationKind::SeriesWorks, first, child_complexity)?"
    )]
    async fn works(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<String, Work, EmptyFields, MembershipFields>> {
        let page = relation(ctx, RelationKind::SeriesWorks, self.record.id, first, after).await?;
        let keys = page
            .edges
            .iter()
            .map(|(_, row)| required(row.id).map(WorkKey))
            .collect::<async_graphql::Result<Vec<_>>>()?;
        let records = ctx
            .data::<RequestLoader>()?
            .load_many(keys)
            .await
            .map_err(|error| batch_error(&error))?;
        let mut connection = Connection::new(page.has_previous, page.has_next);
        for (cursor, row) in page.edges {
            if let Some(record) = records.get(&WorkKey(required(row.id)?)) {
                connection.edges.push(Edge::with_additional_fields(
                    cursor,
                    Work::new(record.clone()),
                    MembershipFields {
                        position: row.position,
                    },
                ));
            }
        }
        Ok(connection)
    }
}

fn timestamp(value: Timestamp) -> String {
    value.to_string()
}
fn required<T>(value: Option<T>) -> async_graphql::Result<T> {
    value.ok_or_else(|| {
        safe_error(AppError::Internal(anyhow::anyhow!(
            "Missing reader projection field"
        )))
    })
}

pub(super) fn page_size(first: i32) -> async_graphql::Result<i32> {
    super::complexity::effective_page(first, reader::PAGE_MAX).map_err(safe_error)
}
async fn relation(
    ctx: &Context<'_>,
    kind: RelationKind,
    parent: Uuid,
    first: i32,
    after: Option<String>,
) -> async_graphql::Result<RelationPage> {
    let key = RelationKey {
        kind,
        parent,
        first: super::complexity::nested_size(kind, first).map_err(safe_error)?,
        after,
    };
    required(
        ctx.data::<RequestLoader>()?
            .load_one(key)
            .await
            .map_err(|error| batch_error(&error))?,
    )
}
fn term_connection<T: async_graphql::OutputType>(
    page: RelationPage,
    convert: impl Fn(RelationRow) -> async_graphql::Result<T>,
) -> async_graphql::Result<Connection<String, T>> {
    let mut connection = Connection::new(page.has_previous, page.has_next);
    for (cursor, row) in page.edges {
        connection.edges.push(Edge::new(cursor, convert(row)?));
    }
    Ok(connection)
}
fn contributor_connection<T: async_graphql::OutputType>(
    page: RelationPage,
    node: impl Fn(Author) -> T,
) -> async_graphql::Result<Connection<String, T, EmptyFields, ContributorFields>> {
    let mut connection = Connection::new(page.has_previous, page.has_next);
    for (cursor, row) in page.edges {
        let position = required(row.position_index)?;
        connection.edges.push(Edge::with_additional_fields(
            cursor,
            node(Author {
                id: id::Author::encode(required(row.id)?),
                name: required(row.name)?,
            }),
            ContributorFields {
                role: match required(row.role)? {
                    crate::models::contributor_role::ContributorRole::Author => "author",
                    crate::models::contributor_role::ContributorRole::Editor => "editor",
                    crate::models::contributor_role::ContributorRole::Translator => "translator",
                    crate::models::contributor_role::ContributorRole::Narrator => "narrator",
                }
                .to_owned(),
                position,
            },
        ));
    }
    Ok(connection)
}
async fn manifestation_connection(
    ctx: &Context<'_>,
    page: RelationPage,
) -> async_graphql::Result<Connection<String, Manifestation>> {
    let ids = page
        .edges
        .iter()
        .map(|(_, row)| required(row.id))
        .collect::<async_graphql::Result<Vec<_>>>()?;
    let records = ctx
        .data::<RequestLoader>()?
        .load_many(ids)
        .await
        .map_err(|error| batch_error(&error))?;
    let mut connection = Connection::new(page.has_previous, page.has_next);
    for (cursor, row) in page.edges {
        if let Some(record) = records.get(&required(row.id)?) {
            connection
                .edges
                .push(Edge::new(cursor, Manifestation::new(record.clone())));
        }
    }
    Ok(connection)
}

pub(super) async fn manifestation(
    ctx: &Context<'_>,
    id: &ID,
) -> async_graphql::Result<Option<Manifestation>> {
    let Some(id) = id::Manifestation::decode(id) else {
        return Ok(None);
    };
    Ok(ctx
        .data::<RequestLoader>()?
        .load_one(id)
        .await
        .map_err(|error| batch_error(&error))?
        .map(Manifestation::new))
}
pub(super) async fn work(ctx: &Context<'_>, id: &ID) -> async_graphql::Result<Option<Work>> {
    let Some(id) = id::Work::decode(id) else {
        return Ok(None);
    };
    let loader = ctx.data::<RequestLoader>()?;
    Ok(loader
        .load_one(WorkKey(id))
        .await
        .map_err(|error| batch_error(&error))?
        .map(Work::new))
}
pub(super) async fn shelf(ctx: &Context<'_>, id: &ID) -> async_graphql::Result<Option<Shelf>> {
    let Some(id) = id::Shelf::decode(id) else {
        return Ok(None);
    };
    ctx.data::<RequestLoader>()?
        .load_one(ShelfKey(id))
        .await
        .map_err(|error| batch_error(&error))?
        .map(Shelf::new)
        .transpose()
}
pub(super) async fn series(ctx: &Context<'_>, id: &ID) -> async_graphql::Result<Option<Series>> {
    let Some(id) = id::Series::decode(id) else {
        return Ok(None);
    };
    Ok(ctx
        .data::<RequestLoader>()?
        .load_one(SeriesKey(id))
        .await
        .map_err(|error| batch_error(&error))?
        .map(Series::new))
}

#[derive(Default, InputObject)]
pub(super) struct BooksFilter {
    #[graphql(default)]
    author: Vec<ID>,
    #[graphql(default)]
    author_any: Vec<ID>,
    #[graphql(default)]
    author_none: Vec<ID>,
    series: Option<ID>,
    #[graphql(default)]
    series_any: Vec<ID>,
    shelf: Option<ID>,
    #[graphql(default)]
    shelf_any: Vec<ID>,
    #[graphql(default)]
    tag: Vec<String>,
    #[graphql(default)]
    genre: Vec<String>,
    #[graphql(default)]
    genre_any: Vec<String>,
    #[graphql(default)]
    genre_none: Vec<String>,
    #[graphql(default)]
    mood: Vec<String>,
    #[graphql(default)]
    mood_any: Vec<String>,
    #[graphql(default)]
    mood_none: Vec<String>,
    #[graphql(default)]
    tag_any: Vec<String>,
    #[graphql(default)]
    tag_none: Vec<String>,
    title_contains: Option<String>,
    title_eq: Option<String>,
    title_ne: Option<String>,
    subtitle_contains: Option<String>,
    subtitle_empty: Option<bool>,
    isbn_13_contains: Option<String>,
    isbn_13_eq: Option<String>,
    isbn_13_empty: Option<bool>,
    pages_gte: Option<i32>,
    pages_lte: Option<i32>,
    pages_empty: Option<bool>,
    created_at_gte: Option<String>,
    created_at_lt: Option<String>,
    #[graphql(default)]
    status_any: Vec<String>,
    #[graphql(default)]
    status_none: Vec<String>,
    rating_gte: Option<i16>,
    rating_lte: Option<i16>,
    rating_empty: Option<bool>,
    q: Option<String>,
}
impl BooksFilter {
    fn params(self) -> async_graphql::Result<ListParams> {
        Ok(ListParams {
            author: self
                .author
                .iter()
                .map(filter_id::<id::Author>)
                .collect::<async_graphql::Result<Vec<_>>>()?,
            author_any: self
                .author_any
                .iter()
                .map(filter_id::<id::Author>)
                .collect::<async_graphql::Result<Vec<_>>>()?,
            author_none: self
                .author_none
                .iter()
                .map(filter_id::<id::Author>)
                .collect::<async_graphql::Result<Vec<_>>>()?,
            series: self
                .series
                .as_ref()
                .map(filter_id::<id::Series>)
                .transpose()?,
            series_any: self
                .series_any
                .iter()
                .map(filter_id::<id::Series>)
                .collect::<async_graphql::Result<Vec<_>>>()?,
            shelf: self
                .shelf
                .as_ref()
                .map(filter_id::<id::Shelf>)
                .transpose()?,
            shelf_any: self
                .shelf_any
                .iter()
                .map(filter_id::<id::Shelf>)
                .collect::<async_graphql::Result<Vec<_>>>()?,
            tag: self.tag,
            genre: self.genre,
            genre_any: self.genre_any,
            genre_none: self.genre_none,
            mood: self.mood,
            mood_any: self.mood_any,
            mood_none: self.mood_none,
            tag_any: self.tag_any,
            tag_none: self.tag_none,
            title_contains: self.title_contains,
            title_eq: self.title_eq,
            title_ne: self.title_ne,
            subtitle_contains: self.subtitle_contains,
            subtitle_empty: self.subtitle_empty,
            isbn_13_contains: self.isbn_13_contains,
            isbn_13_eq: self.isbn_13_eq,
            isbn_13_empty: self.isbn_13_empty,
            pages_gte: self.pages_gte,
            pages_lte: self.pages_lte,
            pages_empty: self.pages_empty,
            created_at_gte: self
                .created_at_gte
                .as_deref()
                .map(filter_timestamp)
                .transpose()?,
            created_at_lt: self
                .created_at_lt
                .as_deref()
                .map(filter_timestamp)
                .transpose()?,
            status_any: self.status_any,
            status_none: self.status_none,
            rating_gte: self.rating_gte,
            rating_lte: self.rating_lte,
            rating_empty: self.rating_empty,
            q: self.q,
            ..ListParams::default()
        })
    }
}

fn filter_id<K: id::Kind>(value: &ID) -> async_graphql::Result<Uuid> {
    K::decode(value)
        .ok_or_else(|| safe_error(AppError::Validation("Invalid filter identity.".into())))
}
fn filter_timestamp(value: &str) -> async_graphql::Result<Timestamp> {
    value
        .parse::<jiff::Timestamp>()
        .map_err(|_| safe_error(AppError::Validation("Invalid timestamp bound.".into())))
}

input_enum!(SortField { Title => "TITLE", Author => "AUTHOR", CreatedAt => "CREATED_AT", Pages => "PAGES" });
impl SortField {
    const fn wire(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Author => "author",
            Self::CreatedAt => "created_at",
            Self::Pages => "pages",
        }
    }
}
input_enum!(Direction { Asc => "ASC", Desc => "DESC" });
#[derive(InputObject)]
pub(super) struct SortLevel {
    column: SortField,
    direction: Direction,
}

pub(super) async fn books(
    ctx: &Context<'_>,
    first: i32,
    after: Option<String>,
    filter: Option<BooksFilter>,
    sort: Option<Vec<SortLevel>>,
) -> async_graphql::Result<Connection<String, Manifestation>> {
    let first = page_size(first)?;
    let mut params = filter.unwrap_or_default().params()?;
    params.cursor = after;
    params.sort = sort.map(|levels| {
        levels
            .into_iter()
            .map(|level| {
                format!(
                    "{}{}",
                    if level.direction == Direction::Desc {
                        "-"
                    } else {
                        ""
                    },
                    level.column.wire()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    });
    let page = reader::books_connection(
        ctx.data::<CurrentUser>()?,
        ctx.data::<AppState>()?,
        &params,
        i64::from(first),
    )
    .await
    .map_err(safe_error)?;
    let mut connection = Connection::new(page.has_previous, page.has_next);
    ctx.data::<RequestLoader>()?
        .feed_many(
            page.edges
                .iter()
                .map(|(_, record)| (record.detail.id, record.clone())),
        )
        .await;
    for (cursor, record) in page.edges {
        connection
            .edges
            .push(Edge::new(cursor, Manifestation::new(record)));
    }
    Ok(connection)
}

pub(super) async fn shelves(
    ctx: &Context<'_>,
    first: i32,
    after: Option<String>,
) -> async_graphql::Result<Connection<String, Shelf>> {
    let first = page_size(first)?;
    let state = ctx.data::<AppState>()?;
    let user = ctx.data::<CurrentUser>()?;
    let (rows, _, has_next) = reader::shelves(
        user,
        state,
        after.as_deref(),
        i64::from(first),
        CursorApi::Graphql,
    )
    .await
    .map_err(safe_error)?;
    let previous = reader::shelf_has_previous(user, state, after.as_deref())
        .await
        .map_err(safe_error)?;
    let mut connection = Connection::new(previous, has_next);
    let records: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let cursor = crate::routes::cursor::ShelfCursor {
                is_system: row.is_system,
                name: row.name.clone(),
                id: row.id,
            }
            .encode(CursorApi::Graphql);
            let record = Arc::new(ShelfRecord {
                id: row.id,
                name: row.name,
                is_system: row.is_system,
                created_at: row.created_at,
                updated_at: row.updated_at,
                item_count: row.item_count,
            });
            (cursor, record)
        })
        .collect();
    ctx.data::<RequestLoader>()?
        .feed_many(
            records
                .iter()
                .map(|(_, record)| (ShelfKey(record.id), record.clone())),
        )
        .await;
    for (cursor, record) in records {
        connection
            .edges
            .push(Edge::new(cursor, Shelf::new(record)?));
    }
    Ok(connection)
}

#[derive(SimpleObject)]
pub(super) struct SearchHit {
    manifestation: Option<Manifestation>,
    snippet: Option<String>,
}
pub(super) async fn search(ctx: &Context<'_>, q: &str) -> async_graphql::Result<Vec<SearchHit>> {
    let response = reader::search(ctx.data::<CurrentUser>()?, ctx.data::<AppState>()?, Some(q))
        .await
        .map_err(safe_error)?;
    let records = ctx
        .data::<RequestLoader>()?
        .load_many(response.items.iter().map(|item| item.id))
        .await
        .map_err(|error| batch_error(&error))?;
    Ok(response
        .items
        .into_iter()
        .map(|item| SearchHit {
            manifestation: records.get(&item.id).cloned().map(Manifestation::new),
            snippet: item.snippet,
        })
        .collect())
}

fn label_ids<K: id::Kind>(ids: &[ID]) -> async_graphql::Result<Vec<Uuid>> {
    ids.iter().map(filter_id::<K>).collect()
}

async fn series_records(ctx: &Context<'_>, ids: &[Uuid]) -> async_graphql::Result<Vec<Series>> {
    let records = ctx
        .data::<RequestLoader>()?
        .load_many(ids.iter().copied().map(SeriesKey))
        .await
        .map_err(|error| batch_error(&error))?;
    Ok(ids
        .iter()
        .filter_map(|id| records.get(&SeriesKey(*id)).cloned().map(Series::new))
        .collect())
}

macro_rules! vocabulary_suggestions {
    ($function:ident, $operation:ident, $record:ident, $kind:ident) => {
        pub(super) async fn $function(
            ctx: &Context<'_>,
            q: &str,
            limit: Option<i32>,
        ) -> async_graphql::Result<Vec<$record>> {
            let rows = reader::$operation(
                ctx.data::<CurrentUser>()?,
                ctx.data::<AppState>()?,
                q,
                limit.map(i64::from),
            )
            .await
            .map_err(safe_error)?;
            Ok(rows
                .into_iter()
                .filter_map(|row| {
                    row.id.map(|row_id| $record {
                        id: id::$kind::encode(row_id),
                        name: row.value,
                    })
                })
                .collect())
        }
    };
}
vocabulary_suggestions!(author_suggestions, suggest_authors, Author, Author);
vocabulary_suggestions!(genre_suggestions, suggest_genres, Genre, Genre);
vocabulary_suggestions!(mood_suggestions, suggest_moods, Mood, Mood);
vocabulary_suggestions!(tag_suggestions, suggest_tags, Tag, Tag);

pub(super) async fn series_suggestions(
    ctx: &Context<'_>,
    q: &str,
    limit: Option<i32>,
) -> async_graphql::Result<Vec<Series>> {
    let rows = reader::suggest_series(
        ctx.data::<CurrentUser>()?,
        ctx.data::<AppState>()?,
        q,
        limit.map(i64::from),
    )
    .await
    .map_err(safe_error)?;
    let ids: Vec<_> = rows.iter().filter_map(|row| row.id).collect();
    series_records(ctx, &ids).await
}

pub(super) async fn author_labels(
    ctx: &Context<'_>,
    ids: &[ID],
) -> async_graphql::Result<Vec<Author>> {
    let ids = label_ids::<id::Author>(ids)?;
    let rows = reader::author_labels(ctx.data::<CurrentUser>()?, ctx.data::<AppState>()?, &ids)
        .await
        .map_err(safe_error)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            row.id.map(|row_id| Author {
                id: id::Author::encode(row_id),
                name: row.value,
            })
        })
        .collect())
}

pub(super) async fn series_labels(
    ctx: &Context<'_>,
    ids: &[ID],
) -> async_graphql::Result<Vec<Series>> {
    let ids = label_ids::<id::Series>(ids)?;
    let rows = reader::series_labels(ctx.data::<CurrentUser>()?, ctx.data::<AppState>()?, &ids)
        .await
        .map_err(safe_error)?;
    let ids: Vec<_> = rows.iter().map(|row| row.id).collect();
    series_records(ctx, &ids).await
}

pub(super) async fn shelf_labels(
    ctx: &Context<'_>,
    ids: &[ID],
) -> async_graphql::Result<Vec<Shelf>> {
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<_> = label_ids::<id::Shelf>(ids)?
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect();
    // THREAT: the shelf loader filters on the caller's user ID, so another account's shelf is omitted like a missing one.
    let records = ctx
        .data::<RequestLoader>()?
        .load_many(ids.iter().copied().map(ShelfKey))
        .await
        .map_err(|error| batch_error(&error))?;
    ids.iter()
        .filter_map(|id| records.get(&ShelfKey(*id)).cloned().map(Shelf::new))
        .collect()
}
