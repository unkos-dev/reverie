use super::{GraphResult, RequestContext, Viewer, id, safe_error};
use crate::error::AppError;
use crate::routes::etag::MetadataRevision;
use crate::routes::library::ListParams;
use crate::services::metadata::editing::{self, SnapshotKey, SnapshotPage, SnapshotSelection};
use crate::services::reader::{
    self, BookRecord, RelationKey, RelationKind, RelationPage, RelationRow, SeriesRecord,
    ShelfRecord, WorkRecord,
};
use chrono::SecondsFormat;
use juniper::{
    DefaultScalarValue, Executor, GraphQLEnum, GraphQLInputObject, GraphQLObject, ID,
    LookAheadSelection, LookAheadValue, ScalarValue, graphql_interface, graphql_object,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[graphql_interface]
#[graphql(
    for = [Work, Manifestation, Shelf, Series, Viewer],
    scalar = juniper::DefaultScalarValue,
    context = RequestContext
)]
#[expect(
    dead_code,
    reason = "Juniper reads this trait as the interface definition; values use NodeValue."
)]
pub(super) trait Node {
    fn id(&self) -> &ID;
}

connection!(ManifestationConnection, ManifestationEdge, Manifestation);
connection!(ShelfConnection, ShelfEdge, Shelf);
connection!(WorkConnection, WorkEdge, Work, position: Option<f64>);
connection!(AuthorConnection, AuthorEdge, Author, role: String, position: i32);
connection!(GenreConnection, GenreEdge, Genre);
connection!(MoodConnection, MoodEdge, Mood);
connection!(TagConnection, TagEdge, Tag);
connection!(
    ExternalIdentifierConnection,
    ExternalIdentifierEdge,
    ExternalIdentifier
);
connection!(ExternalRatingConnection, ExternalRatingEdge, ExternalRating);

#[derive(Clone)]
pub(super) struct Work {
    id: ID,
    record: Arc<WorkRecord>,
}

impl Work {
    fn new(record: Arc<WorkRecord>) -> Self {
        Self {
            id: id::encode("Work", record.id),
            record,
        }
    }
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext, impl = NodeValue)]
impl Work {
    const fn id(&self) -> &ID {
        &self.id
    }
    fn title(&self) -> &str {
        &self.record.title
    }
    fn subtitle(&self) -> Option<&str> {
        self.record.subtitle.as_deref()
    }
    fn description(&self) -> Option<&str> {
        self.record.description.as_deref()
    }
    fn language(&self) -> Option<&str> {
        self.record.language.as_deref()
    }
    async fn selected_series(
        &self,
        context: &RequestContext,
    ) -> GraphResult<Option<SelectedSeries>> {
        let Some(series_id) = self.record.series_id else {
            return Ok(None);
        };
        Ok(context
            .loaders
            .series(series_id)
            .await?
            .map(|record| SelectedSeries {
                node: Series::new(record),
                position: self.record.series_position,
            }))
    }
    async fn identifiers(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ExternalIdentifierConnection> {
        let page = relation(
            context,
            RelationKind::WorkIdentifiers,
            self.record.id,
            first,
            after,
        )
        .await?;
        identifier_connection(page, |_| Ok("work"))
    }
    async fn manifestations(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ManifestationConnection> {
        let page = relation(
            context,
            RelationKind::Manifestations,
            self.record.id,
            first,
            after,
        )
        .await?;
        manifestation_connection(context, page).await
    }
    async fn authors(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<AuthorConnection> {
        contributor_connection(
            relation(context, RelationKind::Authors, self.record.id, first, after).await?,
        )
    }
    async fn contributors(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<AuthorConnection> {
        contributor_connection(
            relation(
                context,
                RelationKind::Contributors,
                self.record.id,
                first,
                after,
            )
            .await?,
        )
    }
}

#[derive(Clone)]
pub(super) struct Manifestation {
    id: ID,
    record: BookRecord,
}

impl Manifestation {
    pub(super) fn new(record: BookRecord) -> Self {
        Self {
            id: id::encode("Manifestation", record.detail.id),
            record,
        }
    }
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext, impl = NodeValue)]
impl Manifestation {
    const fn id(&self) -> &ID {
        &self.id
    }
    async fn versions(
        &self,
        context: &RequestContext,
    ) -> GraphResult<super::versions::MetadataVersions> {
        super::versions::resolve(context, self.record.detail.id).await
    }
    async fn work(&self, context: &RequestContext) -> GraphResult<Option<Work>> {
        Ok(context
            .loaders
            .work(self.record.detail.work_id)
            .await?
            .map(Work::new))
    }
    fn isbn_13(&self) -> Option<&str> {
        self.record.detail.isbn_13.as_deref()
    }
    fn isbn_10(&self) -> Option<&str> {
        self.record.detail.isbn_10.as_deref()
    }
    fn isbn_mismatch(&self) -> bool {
        self.record.detail.isbn_mismatch()
    }
    fn pages(&self) -> Option<i32> {
        self.record.detail.pages
    }
    fn publisher(&self) -> Option<&str> {
        self.record.detail.publisher.as_deref()
    }
    fn publication_date(&self) -> Option<&str> {
        self.record.detail.pub_date.as_deref()
    }
    fn created_at(&self) -> String {
        timestamp(self.record.detail.created_at)
    }
    fn updated_at(&self) -> String {
        timestamp(self.record.detail.updated_at)
    }
    fn cover_url(&self) -> String {
        format!("/api/v1/books/{}/cover/thumb", self.record.detail.id)
    }
    fn content_rating(&self) -> Option<&'static str> {
        self.record
            .detail
            .content_rating
            .map(crate::models::content_rating::ContentRating::as_str)
    }
    fn ingestion_status(&self) -> &str {
        &self.record.detail.ingestion_status
    }
    fn validation_status(&self) -> &'static str {
        self.record.detail.validation_status.as_str()
    }
    fn enrichment_status(&self) -> &str {
        &self.record.detail.enrichment_status
    }
    fn reading_state(&self) -> ReadingState {
        let row = &self.record.reading;
        ReadingState {
            etag: self.record.reading_etag.clone(),
            status: row.status.map(|value| value.as_str().to_owned()),
            rating: row.rating.map(i32::from),
            notes: row.notes.clone(),
            progress_pct: row.progress_pct.map(f64::from),
            started_at: row.started_at.map(timestamp),
            finished_at: row.finished_at.map(timestamp),
            last_read_at: row.last_read_at.map(timestamp),
        }
    }
    async fn metadata(
        &self,
        context: &RequestContext,
        executor: &Executor<'_, '_, RequestContext>,
    ) -> GraphResult<MetadataSnapshot> {
        let pages = snapshot_pages(executor.look_ahead().children())?;
        if !pages.is_empty() {
            let snapshot = required(
                context
                    .loaders
                    .snapshot(SnapshotKey {
                        manifestation_id: self.record.detail.id,
                        pages,
                    })
                    .await?,
            )?;
            return MetadataSnapshot::from_domain(&snapshot);
        }
        let row = &self.record.detail;
        Ok(MetadataSnapshot {
            revision: MetadataRevision {
                work_id: row.work_id,
                manifestation_id: row.id,
                work_revision: row.work_revision,
                manifestation_revision: row.manifestation_revision,
            },
            relation_pages: HashMap::new(),
            isbn_mismatch: row.isbn_mismatch(),
            work_id: id::encode("Work", row.work_id),
            manifestation_id: id::encode("Manifestation", row.id),
            etag: self.record.metadata_etag.clone(),
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
        })
    }
    async fn genres(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<GenreConnection> {
        let page = relation(
            context,
            RelationKind::Genres,
            self.record.detail.id,
            first,
            after,
        )
        .await?;
        genre_connection(page)
    }
    async fn moods(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<MoodConnection> {
        let page = relation(
            context,
            RelationKind::Moods,
            self.record.detail.id,
            first,
            after,
        )
        .await?;
        mood_connection(page)
    }
    async fn tags(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<TagConnection> {
        let page = relation(
            context,
            RelationKind::Tags,
            self.record.detail.id,
            first,
            after,
        )
        .await?;
        tag_connection(page)
    }
    async fn identifiers(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ExternalIdentifierConnection> {
        let page = relation(
            context,
            RelationKind::Identifiers,
            self.record.detail.id,
            first,
            after,
        )
        .await?;
        identifier_connection(page, |row| {
            Ok(if required(row.level)? == 0 {
                "work"
            } else {
                "manifestation"
            })
        })
    }
    async fn ratings(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ExternalRatingConnection> {
        let page = relation(
            context,
            RelationKind::Ratings,
            self.record.detail.id,
            first,
            after,
        )
        .await?;
        let mut edges = Vec::with_capacity(page.edges.len());
        for (cursor, row) in page.edges {
            edges.push(ExternalRatingEdge::new(
                cursor,
                ExternalRating {
                    source: required(row.name)?,
                    rating: required(row.rating)?,
                    scale: required(row.scale)?,
                    review_count: row.reviews,
                    fetched_at: row.created_at.map(timestamp),
                },
            ));
        }
        Ok(ExternalRatingConnection::new(
            page.has_previous,
            page.has_next,
            edges,
        ))
    }
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
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
    pub(super) fn from_row(row: &crate::services::reading::ReadingStateRow) -> GraphResult<Self> {
        Ok(Self {
            etag: row
                .etag()?
                .to_str()
                .map_err(|e| AppError::Internal(e.into()))?
                .to_owned(),
            status: row.status.map(|v| v.as_str().to_owned()),
            rating: row.rating.map(i32::from),
            notes: row.notes.clone(),
            progress_pct: row.progress_pct.map(f64::from),
            started_at: row.started_at.map(timestamp),
            finished_at: row.finished_at.map(timestamp),
            last_read_at: row.last_read_at.map(timestamp),
        })
    }
}

pub(super) struct MetadataSnapshot {
    revision: MetadataRevision,
    relation_pages: HashMap<RelationKey, RelationPage>,
    isbn_mismatch: bool,
    work_id: ID,
    manifestation_id: ID,
    etag: String,
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

pub(super) fn snapshot_pages<'a>(
    fields: impl IntoIterator<Item = LookAheadSelection<'a, DefaultScalarValue>>,
) -> GraphResult<Vec<SnapshotPage>> {
    let mut pages = Vec::new();
    for field in fields {
        let kind = match field.field_original_name() {
            "authors" => RelationKind::Authors,
            "contributors" => RelationKind::Contributors,
            "genres" => RelationKind::Genres,
            "moods" => RelationKind::Moods,
            "tags" => RelationKind::Tags,
            "workIdentifiers" => RelationKind::WorkIdentifiers,
            "manifestationIdentifiers" => RelationKind::Identifiers,
            _ => continue,
        };
        let first = field
            .argument("first")
            .and_then(|argument| match argument.value() {
                LookAheadValue::Scalar(value) => value.try_to_int(),
                _ => None,
            })
            .ok_or_else(|| AppError::Validation("Missing page size.".into()))?;
        let after = field
            .argument("after")
            .and_then(|argument| match argument.value() {
                LookAheadValue::Scalar(value) => value.try_to_string(),
                _ => None,
            });
        let page = SnapshotPage {
            kind,
            first: super::complexity::nested_size(kind, first)?,
            after,
        };
        if !pages.contains(&page) {
            pages.push(page);
        }
    }
    Ok(pages)
}

pub(super) fn snapshot_selection<'a>(
    fields: impl IntoIterator<Item = LookAheadSelection<'a, DefaultScalarValue>>,
) -> GraphResult<SnapshotSelection> {
    snapshot_pages(fields).map(SnapshotSelection::Selected)
}

impl MetadataSnapshot {
    pub(super) fn from_domain(snapshot: &editing::MetadataSnapshot) -> GraphResult<Self> {
        let row = &snapshot.values;
        Ok(Self {
            revision: MetadataRevision {
                work_id: snapshot.revision.work_id,
                manifestation_id: snapshot.revision.manifestation_id,
                work_revision: snapshot.revision.work_revision,
                manifestation_revision: snapshot.revision.manifestation_revision,
            },
            relation_pages: snapshot.pages.clone(),
            work_id: id::encode("Work", snapshot.revision.work_id),
            manifestation_id: id::encode("Manifestation", snapshot.revision.manifestation_id),
            etag: snapshot
                .revision
                .etag()?
                .to_str()
                .map_err(|e| AppError::Internal(e.into()))?
                .to_owned(),
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
    ) -> GraphResult<RelationPage> {
        let key = SnapshotPage {
            kind,
            first: super::complexity::nested_size(kind, first)?,
            after,
        }
        .key(&self.revision);
        self.relation_pages.get(&key).cloned().ok_or_else(|| {
            safe_error(AppError::Internal(anyhow::anyhow!(
                "Snapshot page was not materialised"
            )))
        })
    }
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
impl MetadataSnapshot {
    const fn isbn_mismatch(&self) -> bool {
        self.isbn_mismatch
    }
    const fn work_id(&self) -> &ID {
        &self.work_id
    }
    const fn manifestation_id(&self) -> &ID {
        &self.manifestation_id
    }
    fn etag(&self) -> &str {
        &self.etag
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }
    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }
    fn isbn_10(&self) -> Option<&str> {
        self.isbn_10.as_deref()
    }
    fn isbn_13(&self) -> Option<&str> {
        self.isbn_13.as_deref()
    }
    const fn pages(&self) -> Option<i32> {
        self.pages
    }
    fn publisher(&self) -> Option<&str> {
        self.publisher.as_deref()
    }
    fn publication_date(&self) -> Option<&str> {
        self.publication_date.as_deref()
    }
    fn content_rating(&self) -> Option<&str> {
        self.content_rating.as_deref()
    }
    fn authors(&self, first: i32, after: Option<String>) -> GraphResult<AuthorConnection> {
        contributor_connection(self.page(RelationKind::Authors, first, after)?)
    }
    fn contributors(&self, first: i32, after: Option<String>) -> GraphResult<AuthorConnection> {
        contributor_connection(self.page(RelationKind::Contributors, first, after)?)
    }
    fn genres(&self, first: i32, after: Option<String>) -> GraphResult<GenreConnection> {
        genre_connection(self.page(RelationKind::Genres, first, after)?)
    }
    fn moods(&self, first: i32, after: Option<String>) -> GraphResult<MoodConnection> {
        mood_connection(self.page(RelationKind::Moods, first, after)?)
    }
    fn tags(&self, first: i32, after: Option<String>) -> GraphResult<TagConnection> {
        tag_connection(self.page(RelationKind::Tags, first, after)?)
    }
    fn work_identifiers(
        &self,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ExternalIdentifierConnection> {
        identifier_connection(
            self.page(RelationKind::WorkIdentifiers, first, after)?,
            |_| Ok("work"),
        )
    }
    fn manifestation_identifiers(
        &self,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ExternalIdentifierConnection> {
        identifier_connection(self.page(RelationKind::Identifiers, first, after)?, |_| {
            Ok("manifestation")
        })
    }
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct SelectedSeries {
    node: Series,
    position: Option<f64>,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct Author {
    id: ID,
    name: String,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct Genre {
    id: ID,
    name: String,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct Mood {
    id: ID,
    name: String,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct Tag {
    id: ID,
    name: String,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct ExternalIdentifier {
    level: String,
    scheme: String,
    value: String,
}
#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct ExternalRating {
    source: String,
    rating: f64,
    scale: f64,
    review_count: Option<i32>,
    fetched_at: Option<String>,
}

#[derive(Clone)]
pub(super) struct Shelf {
    id: ID,
    item_count: i32,
    record: Arc<ShelfRecord>,
}
impl Shelf {
    pub(super) fn from_model(row: crate::models::shelf::Shelf) -> GraphResult<Self> {
        Self::new(Arc::new(ShelfRecord {
            id: row.id,
            name: row.name,
            is_system: row.is_system,
            created_at: row.created_at,
            updated_at: row.updated_at,
            item_count: row.item_count,
        }))
    }
    fn new(record: Arc<ShelfRecord>) -> GraphResult<Self> {
        Ok(Self {
            id: id::encode("Shelf", record.id),
            item_count: i32::try_from(record.item_count)
                .map_err(|e| AppError::Internal(e.into()))?,
            record,
        })
    }
}
#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext, impl = NodeValue)]
impl Shelf {
    const fn id(&self) -> &ID {
        &self.id
    }
    const fn item_count(&self) -> i32 {
        self.item_count
    }
    fn name(&self) -> &str {
        &self.record.name
    }
    fn is_system(&self) -> bool {
        self.record.is_system
    }
    fn created_at(&self) -> String {
        timestamp(self.record.created_at)
    }
    fn updated_at(&self) -> String {
        timestamp(self.record.updated_at)
    }
    async fn items(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<ManifestationConnection> {
        let page = relation(
            context,
            RelationKind::ShelfItems,
            self.record.id,
            first,
            after,
        )
        .await?;
        manifestation_connection(context, page).await
    }
}

#[derive(Clone)]
pub(super) struct Series {
    id: ID,
    record: Arc<SeriesRecord>,
}
impl Series {
    fn new(record: Arc<SeriesRecord>) -> Self {
        Self {
            id: id::encode("Series", record.id),
            record,
        }
    }
}
#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext, impl = NodeValue)]
impl Series {
    const fn id(&self) -> &ID {
        &self.id
    }
    fn name(&self) -> &str {
        &self.record.name
    }
    fn sort_name(&self) -> &str {
        &self.record.sort_name
    }
    async fn works(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<WorkConnection> {
        let page = relation(
            context,
            RelationKind::SeriesWorks,
            self.record.id,
            first,
            after,
        )
        .await?;
        let keys = page
            .edges
            .iter()
            .map(|(_, row)| required(row.id))
            .collect::<GraphResult<Vec<_>>>()?;
        let records = context.loaders.works(&keys).await?;
        let mut edges = Vec::with_capacity(page.edges.len());
        for (cursor, row) in page.edges {
            if let Some(record) = records.get(&required(row.id)?) {
                edges.push(WorkEdge::new(
                    cursor,
                    Work::new(record.clone()),
                    row.position,
                ));
            }
        }
        Ok(WorkConnection::new(page.has_previous, page.has_next, edges))
    }
}

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
fn required<T>(value: Option<T>) -> GraphResult<T> {
    value.ok_or_else(|| {
        safe_error(AppError::Internal(anyhow::anyhow!(
            "Missing reader projection field"
        )))
    })
}

pub(super) fn page_size(first: i32) -> GraphResult<i32> {
    Ok(super::complexity::effective_page(first, reader::PAGE_MAX)?)
}
async fn relation(
    context: &RequestContext,
    kind: RelationKind,
    parent: Uuid,
    first: i32,
    after: Option<String>,
) -> GraphResult<RelationPage> {
    let key = RelationKey {
        kind,
        parent,
        first: super::complexity::nested_size(kind, first)?,
        after,
    };
    required(context.loaders.relation(key).await?)
}
fn genre_connection(page: RelationPage) -> GraphResult<GenreConnection> {
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        edges.push(GenreEdge::new(
            cursor,
            Genre {
                id: id::encode("Genre", required(row.id)?),
                name: required(row.name)?,
            },
        ));
    }
    Ok(GenreConnection::new(
        page.has_previous,
        page.has_next,
        edges,
    ))
}
fn mood_connection(page: RelationPage) -> GraphResult<MoodConnection> {
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        edges.push(MoodEdge::new(
            cursor,
            Mood {
                id: id::encode("Mood", required(row.id)?),
                name: required(row.name)?,
            },
        ));
    }
    Ok(MoodConnection::new(page.has_previous, page.has_next, edges))
}
fn tag_connection(page: RelationPage) -> GraphResult<TagConnection> {
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        edges.push(TagEdge::new(
            cursor,
            Tag {
                id: id::encode("Tag", required(row.id)?),
                name: required(row.name)?,
            },
        ));
    }
    Ok(TagConnection::new(page.has_previous, page.has_next, edges))
}
fn identifier_connection(
    page: RelationPage,
    level: impl Fn(&RelationRow) -> GraphResult<&'static str>,
) -> GraphResult<ExternalIdentifierConnection> {
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        let level = level(&row)?.to_owned();
        edges.push(ExternalIdentifierEdge::new(
            cursor,
            ExternalIdentifier {
                level,
                scheme: required(row.name)?,
                value: required(row.value)?,
            },
        ));
    }
    Ok(ExternalIdentifierConnection::new(
        page.has_previous,
        page.has_next,
        edges,
    ))
}
fn contributor_connection(page: RelationPage) -> GraphResult<AuthorConnection> {
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        let position = required(row.position_index)?;
        let role = match required(row.role)? {
            crate::models::contributor_role::ContributorRole::Author => "author",
            crate::models::contributor_role::ContributorRole::Editor => "editor",
            crate::models::contributor_role::ContributorRole::Translator => "translator",
            crate::models::contributor_role::ContributorRole::Narrator => "narrator",
        };
        edges.push(AuthorEdge::new(
            cursor,
            Author {
                id: id::encode("Author", required(row.id)?),
                name: required(row.name)?,
            },
            role.to_owned(),
            position,
        ));
    }
    Ok(AuthorConnection::new(
        page.has_previous,
        page.has_next,
        edges,
    ))
}
async fn manifestation_connection(
    context: &RequestContext,
    page: RelationPage,
) -> GraphResult<ManifestationConnection> {
    let ids = page
        .edges
        .iter()
        .map(|(_, row)| required(row.id))
        .collect::<GraphResult<Vec<_>>>()?;
    let records = context.loaders.books(&ids).await?;
    let mut edges = Vec::with_capacity(page.edges.len());
    for (cursor, row) in page.edges {
        if let Some(record) = records.get(&required(row.id)?) {
            edges.push(ManifestationEdge::new(
                cursor,
                Manifestation::new(record.clone()),
            ));
        }
    }
    Ok(ManifestationConnection::new(
        page.has_previous,
        page.has_next,
        edges,
    ))
}

pub(super) async fn manifestation(
    context: &RequestContext,
    id: &ID,
) -> GraphResult<Option<Manifestation>> {
    let Some(id) = id::decode("Manifestation", id) else {
        return Ok(None);
    };
    Ok(context.loaders.book(id).await?.map(Manifestation::new))
}
pub(super) async fn work(context: &RequestContext, id: &ID) -> GraphResult<Option<Work>> {
    let Some(id) = id::decode("Work", id) else {
        return Ok(None);
    };
    Ok(context.loaders.work(id).await?.map(Work::new))
}
pub(super) async fn shelf(context: &RequestContext, id: &ID) -> GraphResult<Option<Shelf>> {
    let Some(id) = id::decode("Shelf", id) else {
        return Ok(None);
    };
    context.loaders.shelf(id).await?.map(Shelf::new).transpose()
}
pub(super) async fn series(context: &RequestContext, id: &ID) -> GraphResult<Option<Series>> {
    let Some(id) = id::decode("Series", id) else {
        return Ok(None);
    };
    Ok(context.loaders.series(id).await?.map(Series::new))
}

#[derive(Default, GraphQLInputObject)]
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
    rating_gte: Option<i32>,
    rating_lte: Option<i32>,
    rating_empty: Option<bool>,
    q: Option<String>,
}
impl BooksFilter {
    fn params(self) -> GraphResult<ListParams> {
        Ok(ListParams {
            author: filter_ids("Author", &self.author)?,
            author_any: filter_ids("Author", &self.author_any)?,
            author_none: filter_ids("Author", &self.author_none)?,
            series: self
                .series
                .as_ref()
                .map(|value| filter_id("Series", value))
                .transpose()?,
            series_any: filter_ids("Series", &self.series_any)?,
            shelf: self
                .shelf
                .as_ref()
                .map(|value| filter_id("Shelf", value))
                .transpose()?,
            shelf_any: filter_ids("Shelf", &self.shelf_any)?,
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
            rating_gte: self.rating_gte.map(small_int).transpose()?,
            rating_lte: self.rating_lte.map(small_int).transpose()?,
            rating_empty: self.rating_empty,
            q: self.q,
            ..ListParams::default()
        })
    }
}

pub(super) fn small_int(value: i32) -> GraphResult<i16> {
    i16::try_from(value).map_err(|_| {
        safe_error(AppError::Validation(
            "Only integers from -32768 to 32767 are accepted.".into(),
        ))
    })
}
fn filter_ids(kind: &str, values: &[ID]) -> GraphResult<Vec<Uuid>> {
    values.iter().map(|value| filter_id(kind, value)).collect()
}
fn filter_id(kind: &str, value: &ID) -> GraphResult<Uuid> {
    id::decode(kind, value)
        .ok_or_else(|| safe_error(AppError::Validation("Invalid filter identity.".into())))
}
fn filter_timestamp(value: &str) -> GraphResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| safe_error(AppError::Validation("Invalid timestamp bound.".into())))
}

#[derive(Clone, Copy, PartialEq, Eq, GraphQLEnum)]
pub(super) enum SortField {
    #[graphql(name = "title")]
    Title,
    #[graphql(name = "author")]
    Author,
    #[graphql(name = "created_at")]
    CreatedAt,
    #[graphql(name = "pages")]
    Pages,
}
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
#[derive(Clone, Copy, PartialEq, Eq, GraphQLEnum)]
pub(super) enum Direction {
    Asc,
    Desc,
}
#[derive(GraphQLInputObject)]
pub(super) struct SortLevel {
    column: SortField,
    direction: Direction,
}

pub(super) async fn books(
    context: &RequestContext,
    first: i32,
    after: Option<String>,
    filter: Option<BooksFilter>,
    sort: Option<Vec<SortLevel>>,
) -> GraphResult<ManifestationConnection> {
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
    let page =
        reader::books_connection(&context.user, &context.state, &params, i64::from(first)).await?;
    context.loaders.prime_books(
        page.edges
            .iter()
            .map(|(_, record)| (record.detail.id, record.clone())),
    );
    let edges = page
        .edges
        .into_iter()
        .map(|(cursor, record)| ManifestationEdge::new(cursor, Manifestation::new(record)))
        .collect();
    Ok(ManifestationConnection::new(
        page.has_previous,
        page.has_next,
        edges,
    ))
}

pub(super) async fn shelves(
    context: &RequestContext,
    first: i32,
    after: Option<String>,
) -> GraphResult<ShelfConnection> {
    let first = page_size(first)?;
    let (user, state) = (&context.user, &context.state);
    let (rows, _, has_next) =
        reader::shelves(user, state, after.as_deref(), i64::from(first)).await?;
    let previous = reader::shelf_has_previous(user, state, after.as_deref()).await?;
    let records: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let cursor = crate::routes::cursor::ShelfCursor {
                is_system: row.is_system,
                name: row.name.clone(),
                id: row.id,
            }
            .encode();
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
    context.loaders.prime_shelves(
        records
            .iter()
            .map(|(_, record)| (record.id, record.clone())),
    );
    let mut edges = Vec::with_capacity(records.len());
    for (cursor, record) in records {
        edges.push(ShelfEdge::new(cursor, Shelf::new(record)?));
    }
    Ok(ShelfConnection::new(previous, has_next, edges))
}

#[derive(GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct SearchHit {
    manifestation: Option<Manifestation>,
    snippet: Option<String>,
}
pub(super) async fn search(context: &RequestContext, q: &str) -> GraphResult<Vec<SearchHit>> {
    let response = reader::search(&context.user, &context.state, Some(q)).await?;
    let ids: Vec<_> = response.items.iter().map(|item| item.id).collect();
    let records = context.loaders.books(&ids).await?;
    Ok(response
        .items
        .into_iter()
        .map(|item| SearchHit {
            manifestation: records.get(&item.id).cloned().map(Manifestation::new),
            snippet: item.snippet,
        })
        .collect())
}

macro_rules! suggestion {
    ($name:ident, $function:ident, $operation:ident, $kind:literal) => {
        #[derive(GraphQLObject)]
        #[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
        pub(super) struct $name {
            entity_id: Option<ID>,
            value: String,
        }
        pub(super) async fn $function(
            context: &RequestContext,
            q: &str,
            limit: Option<i32>,
        ) -> GraphResult<Vec<$name>> {
            let rows =
                reader::$operation(&context.user, &context.state, q, limit.map(i64::from)).await?;
            Ok(rows
                .into_iter()
                .map(|row| $name {
                    entity_id: row.id.map(|id| id::encode($kind, id)),
                    value: row.value,
                })
                .collect())
        }
    };
}
suggestion!(
    AuthorSuggestion,
    author_suggestions,
    suggest_authors,
    "Author"
);
suggestion!(
    SeriesSuggestion,
    series_suggestions,
    suggest_series,
    "Series"
);
suggestion!(GenreSuggestion, genre_suggestions, suggest_genres, "Genre");
suggestion!(MoodSuggestion, mood_suggestions, suggest_moods, "Mood");
suggestion!(TagSuggestion, tag_suggestions, suggest_tags, "Tag");
suggestion!(
    PublisherSuggestion,
    publisher_suggestions,
    suggest_publishers,
    "Publisher"
);

pub(super) async fn author_labels(
    context: &RequestContext,
    ids: &[ID],
) -> GraphResult<Vec<AuthorSuggestion>> {
    let ids = filter_ids("Author", ids)?;
    let rows = reader::author_labels(&context.user, &context.state, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| AuthorSuggestion {
            entity_id: row.id.map(|id| id::encode("Author", id)),
            value: row.value,
        })
        .collect())
}
pub(super) async fn series_labels(
    context: &RequestContext,
    ids: &[ID],
) -> GraphResult<Vec<SeriesSuggestion>> {
    let ids = filter_ids("Series", ids)?;
    let rows = reader::series_labels(&context.user, &context.state, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| SeriesSuggestion {
            entity_id: Some(id::encode("Series", row.id)),
            value: row.value,
        })
        .collect())
}
