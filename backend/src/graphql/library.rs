use super::loaders::{RequestLoader, SeriesKey, ShelfKey, WorkKey, batch_error};
use super::{Viewer, id, safe_error};
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::routes::library::ListParams;
use crate::services::reader::{
    self, BookRecord, RelationKey, RelationKind, RelationPage, RelationRow, SeriesRecord,
    ShelfRecord, WorkRecord,
};
use crate::state::AppState;
use async_graphql::connection::{Connection, Edge, EmptyFields};
use async_graphql::{ComplexObject, Context, ID, InputObject, Interface, SimpleObject};
use chrono::SecondsFormat;
use std::sync::Arc;
use uuid::Uuid;

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
#[graphql(complex)]
pub(super) struct Work {
    id: ID,
    #[graphql(skip)]
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
        )
    }
}

#[derive(Clone, SimpleObject)]
#[graphql(complex)]
pub(super) struct Manifestation {
    id: ID,
    #[graphql(skip)]
    pub(super) record: BookRecord,
}

impl Manifestation {
    pub(super) fn new(record: BookRecord) -> Self {
        Self {
            id: id::encode("Manifestation", record.detail.id),
            record,
        }
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
        self.record
            .detail
            .created_at
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
    }
    async fn updated_at(&self) -> String {
        self.record
            .detail
            .updated_at
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
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
            started_at: row.started_at.map(timestamp),
            finished_at: row.finished_at.map(timestamp),
            last_read_at: row.last_read_at.map(timestamp),
        }
    }
    async fn metadata(&self) -> MetadataSnapshot {
        let row = &self.record.detail;
        MetadataSnapshot {
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
        }
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
                    id: id::encode("Genre", required(row.id)?),
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
                    id: id::encode("Mood", required(row.id)?),
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
                    id: id::encode("Tag", required(row.id)?),
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
                    fetched_at: row.created_at.map(timestamp),
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
#[derive(SimpleObject)]
pub(super) struct MetadataSnapshot {
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
#[graphql(complex)]
pub(super) struct Shelf {
    id: ID,
    item_count: i32,
    #[graphql(skip)]
    record: Arc<ShelfRecord>,
}
impl Shelf {
    fn new(record: Arc<ShelfRecord>) -> async_graphql::Result<Self> {
        Ok(Self {
            id: id::encode("Shelf", record.id),
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
        self.record
            .created_at
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
    }
    async fn updated_at(&self) -> String {
        self.record
            .updated_at
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
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
#[graphql(complex)]
pub(super) struct Series {
    id: ID,
    #[graphql(skip)]
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

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
fn required<T>(value: Option<T>) -> async_graphql::Result<T> {
    value.ok_or_else(|| {
        safe_error(AppError::Internal(anyhow::anyhow!(
            "Missing reader projection field"
        )))
    })
}

pub(super) fn page_size(first: i32) -> async_graphql::Result<i32> {
    super::complexity::effective_page(first, 100).map_err(safe_error)
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
fn contributor_connection(
    page: RelationPage,
) -> async_graphql::Result<Connection<String, Author, EmptyFields, ContributorFields>> {
    let mut connection = Connection::new(page.has_previous, page.has_next);
    for (cursor, row) in page.edges {
        let position = required(row.position_index)?;
        connection.edges.push(Edge::with_additional_fields(
            cursor,
            Author {
                id: id::encode("Author", required(row.id)?),
                name: required(row.name)?,
            },
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
    let Some(id) = id::decode("Manifestation", id) else {
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
    let Some(id) = id::decode("Work", id) else {
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
    let Some(id) = id::decode("Shelf", id) else {
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
    let Some(id) = id::decode("Series", id) else {
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
                .map(|value| filter_id("Author", value))
                .collect::<async_graphql::Result<Vec<_>>>()?,
            author_any: self
                .author_any
                .iter()
                .map(|value| filter_id("Author", value))
                .collect::<async_graphql::Result<Vec<_>>>()?,
            author_none: self
                .author_none
                .iter()
                .map(|value| filter_id("Author", value))
                .collect::<async_graphql::Result<Vec<_>>>()?,
            series: self
                .series
                .as_ref()
                .map(|value| filter_id("Series", value))
                .transpose()?,
            series_any: self
                .series_any
                .iter()
                .map(|value| filter_id("Series", value))
                .collect::<async_graphql::Result<Vec<_>>>()?,
            shelf: self
                .shelf
                .as_ref()
                .map(|value| filter_id("Shelf", value))
                .transpose()?,
            shelf_any: self
                .shelf_any
                .iter()
                .map(|value| filter_id("Shelf", value))
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

fn filter_id(kind: &str, value: &ID) -> async_graphql::Result<Uuid> {
    id::decode(kind, value)
        .ok_or_else(|| safe_error(AppError::Validation("Invalid filter identity.".into())))
}
fn filter_timestamp(value: &str) -> async_graphql::Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| safe_error(AppError::Validation("Invalid timestamp bound.".into())))
}

macro_rules! input_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub(super) enum $name { $($variant),+ }
        impl async_graphql::resolver_utils::EnumType for $name {
            fn items() -> &'static [async_graphql::resolver_utils::EnumItem<Self>] {
                &[$(async_graphql::resolver_utils::EnumItem { name: $wire, value: Self::$variant }),+]
            }
        }
        impl async_graphql::InputType for $name {
            type RawValueType = Self;
            fn type_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
                registry.create_input_type::<Self, _>(async_graphql::registry::MetaTypeId::Enum, |_| {
                    async_graphql::registry::MetaType::Enum {
                        name: stringify!($name).into(), description: None,
                        enum_values: [$(($wire.into(), async_graphql::registry::MetaEnumValue {
                            name: $wire.into(), description: None,
                            deprecation: async_graphql::registry::Deprecation::NoDeprecated,
                            visible: None, inaccessible: false, tags: Vec::new(), directive_invocations: Vec::new(),
                        })),+].into_iter().collect(),
                        visible: None, inaccessible: false, tags: Vec::new(),
                        rust_typename: Some(std::any::type_name::<Self>()),
                        directive_invocations: Vec::new(), requires_scopes: Vec::new(),
                    }
                })
            }
            fn parse(value: Option<async_graphql::Value>) -> async_graphql::InputValueResult<Self> {
                async_graphql::resolver_utils::parse_enum(value.unwrap_or(async_graphql::Value::Null))
            }
            fn to_value(&self) -> async_graphql::Value { async_graphql::resolver_utils::enum_value(*self) }
            fn as_raw_value(&self) -> Option<&Self> { Some(self) }
        }
    };
}
input_enum!(SortField { Title => "title", Author => "author", CreatedAt => "created_at", Pages => "pages" });
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
    let (rows, _, has_next) = reader::shelves(user, state, after.as_deref(), i64::from(first))
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

macro_rules! suggestion {
    ($name:ident, $function:ident, $operation:ident, $kind:literal) => {
        #[derive(SimpleObject)]
        pub(super) struct $name {
            entity_id: Option<ID>,
            value: String,
        }
        pub(super) async fn $function(
            ctx: &Context<'_>,
            q: &str,
            limit: Option<i32>,
        ) -> async_graphql::Result<Vec<$name>> {
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
    ctx: &Context<'_>,
    ids: &[ID],
) -> async_graphql::Result<Vec<AuthorSuggestion>> {
    let ids = ids
        .iter()
        .map(|id| filter_id("Author", id))
        .collect::<async_graphql::Result<Vec<_>>>()?;
    let rows = reader::author_labels(ctx.data::<CurrentUser>()?, ctx.data::<AppState>()?, &ids)
        .await
        .map_err(safe_error)?;
    Ok(rows
        .into_iter()
        .map(|row| AuthorSuggestion {
            entity_id: row.id.map(|id| id::encode("Author", id)),
            value: row.value,
        })
        .collect())
}
pub(super) async fn series_labels(
    ctx: &Context<'_>,
    ids: &[ID],
) -> async_graphql::Result<Vec<SeriesSuggestion>> {
    let ids = ids
        .iter()
        .map(|id| filter_id("Series", id))
        .collect::<async_graphql::Result<Vec<_>>>()?;
    let rows = reader::series_labels(ctx.data::<CurrentUser>()?, ctx.data::<AppState>()?, &ids)
        .await
        .map_err(safe_error)?;
    Ok(rows
        .into_iter()
        .map(|row| SeriesSuggestion {
            entity_id: Some(id::encode("Series", row.id)),
            value: row.value,
        })
        .collect())
}
