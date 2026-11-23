use super::id::Kind as _;
use super::loaders::{ReaderLoader, RequestLoader, batch_error};
use crate::error::AppError;
use crate::services::reader::{self, VersionSet};
use async_graphql::dataloader::Loader;
use async_graphql::{ComplexObject, Context, ID, Json, SimpleObject};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct VersionsKey(Uuid);
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct CanonicalKey(Uuid);

impl Loader<VersionsKey> for ReaderLoader {
    type Value = Arc<VersionSet>;
    type Error = Arc<AppError>;
    async fn load(
        &self,
        keys: &[VersionsKey],
    ) -> Result<HashMap<VersionsKey, Self::Value>, Self::Error> {
        let ids: Vec<_> = keys.iter().map(|key| key.0).collect();
        reader::versions(&self.user, &self.state, &ids)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|(id, row)| (VersionsKey(id), Arc::new(row)))
                    .collect()
            })
            .map_err(Arc::new)
    }
}
impl Loader<CanonicalKey> for ReaderLoader {
    type Value = i32;
    type Error = Arc<AppError>;
    async fn load(
        &self,
        keys: &[CanonicalKey],
    ) -> Result<HashMap<CanonicalKey, Self::Value>, Self::Error> {
        let ids: Vec<_> = keys.iter().map(|key| key.0).collect();
        reader::canonical_counts(&self.user, &self.state, &ids)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|(id, row)| (CanonicalKey(id), row))
                    .collect()
            })
            .map_err(Arc::new)
    }
}

impl Loader<reader::PendingKey> for ReaderLoader {
    type Value = Result<reader::PendingPage, reader::InvalidCursor>;
    type Error = Arc<AppError>;
    async fn load(
        &self,
        keys: &[reader::PendingKey],
    ) -> Result<HashMap<reader::PendingKey, Self::Value>, Self::Error> {
        reader::pending_pages(&self.user, &self.state, keys)
            .await
            .map_err(Arc::new)
    }
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(super) struct MetadataVersions {
    pending_count: i32,
    #[graphql(skip)]
    parent: Uuid,
    #[graphql(skip)]
    record: Arc<VersionSet>,
}
#[ComplexObject]
impl MetadataVersions {
    async fn accepted_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        if let Some(count) = self.record.accepted {
            return Ok(count);
        }
        ctx.data::<RequestLoader>()?
            .load_one(CanonicalKey(self.parent))
            .await
            .map_err(|error| batch_error(&error))?
            .ok_or_else(|| super::safe_error(AppError::NotFound))
    }
    #[graphql(complexity = "super::complexity::pending(first, child_complexity)?")]
    async fn pending(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<async_graphql::connection::Connection<String, MetadataVersion>> {
        let first = super::complexity::pending_size(first).map_err(super::safe_error)?;
        let page = ctx
            .data::<RequestLoader>()?
            .load_one(reader::PendingKey {
                parent: self.parent,
                first,
                after,
            })
            .await
            .map_err(|error| batch_error(&error))?
            .ok_or_else(|| super::safe_error(AppError::NotFound))?
            .map_err(|_| super::safe_error(AppError::InvalidCursor))?;
        let mut connection =
            async_graphql::connection::Connection::new(page.has_previous, page.has_next);
        for (cursor, row) in page.edges {
            connection.edges.push(async_graphql::connection::Edge::new(
                cursor,
                MetadataVersion {
                    id: super::id::MetadataVersion::encode(row.id),
                    field_name: row.field_name,
                    source: row.source,
                    new_value: Json(row.new_value),
                    status: row.status,
                    confidence_score: f64::from(row.confidence_score),
                    match_type: row.match_type,
                    observation_count: row.observation_count,
                },
            ));
        }
        Ok(connection)
    }
}
#[derive(SimpleObject)]
struct MetadataVersion {
    id: ID,
    field_name: String,
    source: String,
    new_value: Json<serde_json::Value>,
    status: String,
    confidence_score: f64,
    match_type: String,
    observation_count: i32,
}

pub(super) async fn resolve(
    ctx: &Context<'_>,
    parent: Uuid,
) -> async_graphql::Result<MetadataVersions> {
    let record = ctx
        .data::<RequestLoader>()?
        .load_one(VersionsKey(parent))
        .await
        .map_err(|error| batch_error(&error))?
        .ok_or_else(|| super::safe_error(AppError::NotFound))?;
    let pending_count = record.pending_count;
    Ok(MetadataVersions {
        pending_count,
        parent,
        record,
    })
}
