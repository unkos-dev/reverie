use super::{GraphResult, RequestContext, safe_error};
use crate::error::AppError;
use crate::services::reader::{self, VersionSet};
use juniper::{
    ExecutionResult, Executor, FromInputValue, GraphQLObject, GraphQLType, GraphQLValue,
    GraphQLValueAsync, ID, InputValue, ParseScalarResult, ParseScalarValue, Registry, ScalarToken,
    ScalarValue, Selection, Value, graphql_object, marker, meta::MetaType,
};
use std::sync::Arc;
use uuid::Uuid;

connection!(
    MetadataVersionConnection,
    MetadataVersionEdge,
    MetadataVersion
);

pub(super) struct MetadataVersions {
    pending_count: i32,
    parent: Uuid,
    record: Arc<VersionSet>,
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
impl MetadataVersions {
    const fn pending_count(&self) -> i32 {
        self.pending_count
    }
    async fn accepted_count(&self, context: &RequestContext) -> GraphResult<i32> {
        if let Some(count) = self.record.accepted {
            return Ok(count);
        }
        context
            .loaders
            .canonical_count(self.parent)
            .await?
            .ok_or_else(|| safe_error(AppError::NotFound))
    }
    async fn pending(
        &self,
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<MetadataVersionConnection> {
        let first = super::complexity::pending_size(first)?;
        let page = context
            .loaders
            .pending(reader::PendingKey {
                parent: self.parent,
                first,
                after,
            })
            .await?
            .ok_or_else(|| safe_error(AppError::NotFound))?;
        let edges = page
            .edges
            .into_iter()
            .map(|(cursor, row)| {
                MetadataVersionEdge::new(
                    cursor,
                    MetadataVersion {
                        id: super::id::encode("MetadataVersion", row.id),
                        field_name: row.field_name,
                        source: row.source,
                        new_value: Json(row.new_value),
                        status: row.status,
                        confidence_score: f64::from(row.confidence_score),
                        match_type: row.match_type,
                        observation_count: row.observation_count,
                    },
                )
            })
            .collect();
        Ok(MetadataVersionConnection::new(
            page.has_previous,
            page.has_next,
            edges,
        ))
    }
}

#[derive(Clone, GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
pub(super) struct MetadataVersion {
    id: ID,
    field_name: String,
    source: String,
    new_value: Json,
    status: String,
    confidence_score: f64,
    match_type: String,
    observation_count: i32,
}

#[derive(Clone)]
pub(super) struct Json(serde_json::Value);

fn json_output<S: ScalarValue>(value: &serde_json::Value) -> Value<S> {
    match value {
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Bool(value) => Value::scalar(*value),
        serde_json::Value::Number(number) => number
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map_or_else(
                || number.as_f64().map_or_else(Value::null, Value::scalar),
                Value::scalar,
            ),
        serde_json::Value::String(value) => Value::scalar(value.clone()),
        serde_json::Value::Array(items) => Value::list(items.iter().map(json_output).collect()),
        serde_json::Value::Object(fields) => Value::object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), json_output(value)))
                .collect(),
        ),
    }
}

impl<S: ScalarValue> marker::IsInputType<S> for Json {}
impl<S: ScalarValue> marker::IsOutputType<S> for Json {}

impl<S: ScalarValue> GraphQLType<S> for Json {
    fn name(&(): &Self::TypeInfo) -> Option<juniper::ArcStr> {
        Some(juniper::arcstr::literal!("JSON"))
    }

    fn meta(info: &Self::TypeInfo, registry: &mut Registry<S>) -> MetaType<S> {
        registry
            .build_scalar_type::<Self>(info)
            .description("A scalar that can represent any JSON value.")
            .into_meta()
    }
}

impl<S: ScalarValue> GraphQLValue<S> for Json {
    type Context = ();
    type TypeInfo = ();

    fn type_name(&self, info: &Self::TypeInfo) -> Option<juniper::ArcStr> {
        <Self as GraphQLType<S>>::name(info)
    }

    fn resolve(
        &self,
        &(): &(),
        _: Option<&[Selection<'_, S>]>,
        _: &Executor<'_, '_, (), S>,
    ) -> ExecutionResult<S> {
        Ok(json_output(&self.0))
    }
}

impl<S: ScalarValue + Send + Sync> GraphQLValueAsync<S> for Json {
    fn resolve_async<'b>(
        &'b self,
        info: &'b (),
        selection_set: Option<&'b [Selection<'_, S>]>,
        executor: &'b Executor<'_, '_, (), S>,
    ) -> juniper::BoxFuture<'b, ExecutionResult<S>> {
        let value = self.resolve(info, selection_set, executor);
        Box::pin(std::future::ready(value))
    }
}

impl<S: ScalarValue> FromInputValue<S> for Json {
    type Error = juniper::FieldError<S>;

    fn from_input_value(input: &InputValue<S>) -> Result<Self, Self::Error> {
        serde_json::to_value(input)
            .map(Self)
            .map_err(|_| juniper::FieldError::from("Invalid JSON value."))
    }
}

impl<S: ScalarValue> ParseScalarValue<S> for Json {
    fn from_str(token: ScalarToken<'_>) -> ParseScalarResult<S> {
        <String as ParseScalarValue<S>>::from_str(token)
    }
}

impl<S> juniper::macros::reflect::BaseType<S> for Json {
    const NAME: juniper::macros::reflect::Type = "JSON";
}

impl<S> juniper::macros::reflect::BaseSubTypes<S> for Json {
    const NAMES: juniper::macros::reflect::Types = &["JSON"];
}

impl<S> juniper::macros::reflect::WrappedType<S> for Json {
    const VALUE: juniper::macros::reflect::WrappedValue = 1;
}

pub(super) async fn resolve(
    context: &RequestContext,
    parent: Uuid,
) -> GraphResult<MetadataVersions> {
    let record = context
        .loaders
        .versions(parent)
        .await?
        .ok_or_else(|| safe_error(AppError::NotFound))?;
    Ok(MetadataVersions {
        pending_count: record.pending_count,
        parent,
        record,
    })
}
