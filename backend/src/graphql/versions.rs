use super::id::Kind as _;
use super::loaders::{ReaderLoader, RequestLoader, batch_error};
use crate::error::AppError;
use crate::services::reader::{self, VersionSet};
use async_graphql::dataloader::Loader;
use async_graphql::{ComplexObject, Context, ID, SimpleObject, Union};
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
                    new_value: MetadataProposalValue::from_json(&row.field_name, row.new_value)
                        .map_err(super::safe_error)?,
                    field_name: row.field_name,
                    source: row.source,
                    status: MetadataVersionStatus::try_from(row.status.as_str())
                        .map_err(super::safe_error)?,
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
    new_value: MetadataProposalValue,
    status: MetadataVersionStatus,
    confidence_score: f64,
    match_type: String,
    observation_count: i32,
}

output_enum!(MetadataVersionStatus { Pending => "PENDING", Rejected => "REJECTED" });

impl TryFrom<&str> for MetadataVersionStatus {
    type Error = AppError;

    fn try_from(status: &str) -> Result<Self, Self::Error> {
        match status {
            "pending" => Ok(Self::Pending),
            "rejected" => Ok(Self::Rejected),
            _ => Err(AppError::Internal(anyhow::anyhow!(
                "Unsupported metadata review status"
            ))),
        }
    }
}

#[derive(Union)]
enum MetadataProposalValue {
    Text(MetadataProposalText),
    Integer(MetadataProposalInteger),
    StringList(MetadataProposalStringList),
    Series(MetadataProposalSeries),
    Contributors(MetadataProposalContributors),
}

#[derive(SimpleObject)]
struct MetadataProposalText {
    text: String,
}

#[derive(SimpleObject)]
struct MetadataProposalInteger {
    integer: SignedInteger,
}

#[derive(SimpleObject)]
struct MetadataProposalStringList {
    strings: Vec<String>,
}

#[derive(SimpleObject, serde::Deserialize)]
struct MetadataProposalSeries {
    name: String,
    position: Option<f64>,
}

#[derive(SimpleObject)]
struct MetadataProposalContributors {
    contributors: Vec<MetadataProposalContributor>,
}

#[derive(SimpleObject, serde::Deserialize)]
struct MetadataProposalContributor {
    name: String,
    code: Option<String>,
}

struct SignedInteger(i64);

#[async_graphql::Scalar(
    name = "SignedInteger",
    specified_by_url = "https://github.com/unkos-dev/reverie/blob/main/docs/specs/api/design/0031-graphql-execution-and-relay-browser-data.md#custom-scalars"
)]
impl async_graphql::ScalarType for SignedInteger {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        match value {
            async_graphql::Value::Number(number) => number.as_i64().map(Self).ok_or_else(|| {
                async_graphql::InputValueError::custom("Expected a signed 64-bit integer.")
            }),
            other => Err(async_graphql::InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::Number(self.0.into())
    }
}

impl MetadataProposalValue {
    fn from_json(field: &str, value: serde_json::Value) -> Result<Self, AppError> {
        use serde_json::Value;

        let invalid = || AppError::Internal(anyhow::anyhow!("Unsupported metadata proposal shape"));
        match field {
            "series" if value.is_object() => {
                return serde_json::from_value(value)
                    .map(Self::Series)
                    .map_err(|_| invalid());
            }
            "contributors.other" if value.is_array() => {
                return serde_json::from_value(value)
                    .map(|contributors| {
                        Self::Contributors(MetadataProposalContributors { contributors })
                    })
                    .map_err(|_| invalid());
            }
            _ => {}
        }
        match value {
            Value::String(text) => Ok(Self::Text(MetadataProposalText { text })),
            Value::Number(number) => Ok(Self::Integer(MetadataProposalInteger {
                integer: SignedInteger(number.as_i64().ok_or_else(invalid)?),
            })),
            Value::Array(values) => {
                let strings = values
                    .into_iter()
                    .map(|value| match value {
                        Value::String(text) => Ok(text),
                        _ => Err(invalid()),
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Self::StringList(MetadataProposalStringList { strings }))
            }
            _ => Err(invalid()),
        }
    }
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
