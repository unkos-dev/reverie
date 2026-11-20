//! Authenticated GraphQL execution with build-generated document admission.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_graphql::{Context, EmptySubscription, ErrorExtensions, Object, Schema};
use async_graphql_axum::GraphQLResponse;
use axum::extract::{Extension, State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::state::AppState;
use id::Kind as _;

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
macro_rules! output_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub(super) enum $name { $($variant),+ }
        impl async_graphql::resolver_utils::EnumType for $name {
            fn items() -> &'static [async_graphql::resolver_utils::EnumItem<Self>] {
                &[$(async_graphql::resolver_utils::EnumItem { name: $wire, value: Self::$variant }),+]
            }
        }
        impl async_graphql::OutputType for $name {
            fn type_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
                registry.create_output_type::<Self, _>(async_graphql::registry::MetaTypeId::Enum, |_| {
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
            fn resolve(
                &self,
                _: &async_graphql::ContextSelectionSet<'_>,
                _: &async_graphql::Positioned<async_graphql::parser::types::Field>,
            ) -> impl std::future::Future<Output = async_graphql::ServerResult<async_graphql::Value>> + Send {
                std::future::ready(Ok(async_graphql::resolver_utils::enum_value(*self)))
            }
        }
    };
}
mod complexity;
mod id;
mod library;
mod loaders;
mod mutations;
mod versions;

/// Build-profile policy for document admission and schema introspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionPolicy {
    /// Developer documents and introspection are available.
    Development,
    /// Only embedded documents execute, with schema introspection disabled.
    Production,
}

impl ExecutionPolicy {
    const fn for_build() -> Self {
        if cfg!(debug_assertions) {
            Self::Development
        } else {
            Self::Production
        }
    }
}

/// Root queries, with scopes declared on each field.
pub struct Query;

struct ReadScope;
struct WriteScope;
impl async_graphql::Guard for WriteScope {
    fn check(
        &self,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = async_graphql::Result<()>> {
        std::future::ready(
            ctx.data::<CurrentUser>()
                .and_then(|user| user.require_scope(Scope::Write).map_err(safe_error)),
        )
    }
}

impl async_graphql::Guard for ReadScope {
    fn check(
        &self,
        ctx: &Context<'_>,
    ) -> impl std::future::Future<Output = async_graphql::Result<()>> {
        std::future::ready(
            ctx.data::<CurrentUser>()
                .and_then(|user| user.require_scope(Scope::Read).map_err(safe_error)),
        )
    }
}

#[async_graphql::TypeDirective(location = "Object", rename_args = "snake_case")]
fn fetchable(field_name: String) {}

/// Caller-owned identity, independent of existing browser account projections.
#[derive(async_graphql::SimpleObject)]
pub struct Viewer {
    id: async_graphql::ID,
    is_child: bool,
    display_name: String,
}

#[Object]
impl Query {
    #[graphql(guard = "ReadScope")]
    async fn library_preferences(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<mutations::LibraryPreferences> {
        crate::services::preferences::get_preferences(ctx.data()?, ctx.data()?)
            .await
            .map(Into::into)
            .map_err(safe_error)
    }

    #[graphql(guard = "WriteScope")]
    async fn metadata_operation(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        request_id: Option<library::UuidValue>,
        operation_id: Option<library::UuidValue>,
    ) -> async_graphql::Result<mutations::MetadataOperation> {
        mutations::metadata_operation(ctx, id, request_id, operation_id).await
    }

    #[graphql(guard = "ReadScope")]
    async fn viewer(&self, ctx: &Context<'_>) -> async_graphql::Result<Viewer> {
        let user = ctx.data::<CurrentUser>()?;
        let pool = ctx.data::<sqlx::PgPool>()?;
        let mut tx = crate::db::acquire_with_rls(pool, user.user_id)
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        let display_name =
            sqlx::query_scalar!("SELECT display_name FROM users WHERE id = $1", user.user_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        tx.commit()
            .await
            .map_err(|error| safe_error(AppError::Internal(error.into())))?;
        Ok(Viewer {
            display_name,
            id: id::User::encode(user.user_id),
            is_child: user.is_child(),
        })
    }
    #[graphql(guard = "ReadScope")]
    async fn node(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Node>> {
        if let Some(user_id) = id::User::decode(&id) {
            if user_id == ctx.data::<CurrentUser>()?.user_id {
                return self
                    .viewer(ctx)
                    .await
                    .map(|viewer| Some(library::Node::Viewer(viewer)));
            }
            return Ok(None);
        }
        if id::Work::decode(&id).is_some() {
            return library::work(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Work));
        }
        if id::Manifestation::decode(&id).is_some() {
            return library::manifestation(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Manifestation));
        }
        if id::Shelf::decode(&id).is_some() {
            return library::shelf(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Shelf));
        }
        if id::Series::decode(&id).is_some() {
            return library::series(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Series));
        }
        Ok(None)
    }
    #[graphql(guard = "ReadScope")]
    async fn work(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Work>> {
        library::work(ctx, &id).await
    }
    #[graphql(guard = "ReadScope")]
    async fn manifestation(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Manifestation>> {
        library::manifestation(ctx, &id).await
    }
    #[graphql(guard = "ReadScope")]
    async fn shelf(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Shelf>> {
        library::shelf(ctx, &id).await
    }
    #[graphql(guard = "ReadScope")]
    async fn series(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Series>> {
        library::series(ctx, &id).await
    }
    #[graphql(name = "fetch__Series", guard = "ReadScope")]
    async fn fetch_series(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Series>> {
        library::series(ctx, &id).await
    }
    #[graphql(name = "fetch__Work", guard = "ReadScope")]
    async fn fetch_work(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Work>> {
        library::work(ctx, &id).await
    }
    #[graphql(name = "fetch__Shelf", guard = "ReadScope")]
    async fn fetch_shelf(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Shelf>> {
        library::shelf(ctx, &id).await
    }
    #[graphql(name = "fetch__Manifestation", guard = "ReadScope")]
    async fn fetch_manifestation(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Manifestation>> {
        library::manifestation(ctx, &id).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::page(first, child_complexity)?"
    )]
    async fn books(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
        filter: Option<library::BooksFilter>,
        #[graphql(desc = "Omitted or null sort uses the installation default.")] sort: Option<
            Vec<library::SortLevel>,
        >,
    ) -> async_graphql::Result<async_graphql::connection::Connection<String, library::Manifestation>>
    {
        library::books(ctx, first, after, filter, sort).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::page(first, child_complexity)?"
    )]
    async fn shelves(
        &self,
        ctx: &Context<'_>,
        first: i32,
        after: Option<String>,
    ) -> async_graphql::Result<async_graphql::connection::Connection<String, library::Shelf>> {
        library::shelves(ctx, first, after).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::weighted(usize::from(crate::services::reader::SEARCH_LIMIT), child_complexity)?"
    )]
    async fn search(
        &self,
        ctx: &Context<'_>,
        q: String,
    ) -> async_graphql::Result<Vec<library::SearchHit>> {
        library::search(ctx, &q).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::labels(ids.len(), child_complexity)?"
    )]
    async fn author_labels(
        &self,
        ctx: &Context<'_>,
        ids: Vec<async_graphql::ID>,
    ) -> async_graphql::Result<Vec<library::AuthorSuggestion>> {
        library::author_labels(ctx, &ids).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::labels(ids.len(), child_complexity)?"
    )]
    async fn series_labels(
        &self,
        ctx: &Context<'_>,
        ids: Vec<async_graphql::ID>,
    ) -> async_graphql::Result<Vec<library::SeriesSuggestion>> {
        library::series_labels(ctx, &ids).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn author_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::AuthorSuggestion>> {
        library::author_suggestions(ctx, &q, limit).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn series_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::SeriesSuggestion>> {
        library::series_suggestions(ctx, &q, limit).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn genre_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::GenreSuggestion>> {
        library::genre_suggestions(ctx, &q, limit).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn mood_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::MoodSuggestion>> {
        library::mood_suggestions(ctx, &q, limit).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn tag_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::TagSuggestion>> {
        library::tag_suggestions(ctx, &q, limit).await
    }
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::suggestions(limit, child_complexity)?"
    )]
    async fn publisher_suggestions(
        &self,
        ctx: &Context<'_>,
        q: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<library::PublisherSuggestion>> {
        library::publisher_suggestions(ctx, &q, limit).await
    }
}

/// Schema for the authenticated browser API.
pub type AppSchema = Schema<Query, mutations::Mutation, EmptySubscription>;

/// Construct a schema with finite execution limits and an explicit admission policy.
#[must_use]
pub fn schema(policy: ExecutionPolicy) -> AppSchema {
    schema_builder(policy).finish()
}

fn schema_builder(
    policy: ExecutionPolicy,
) -> async_graphql::SchemaBuilder<Query, mutations::Mutation, EmptySubscription> {
    let builder = Schema::build(Query, mutations::Mutation, EmptySubscription)
        .limit_depth(complexity::DEPTH)
        .limit_complexity(complexity::LIMIT)
        .data(policy);
    if policy == ExecutionPolicy::Production {
        builder.disable_introspection()
    } else {
        builder
    }
}

fn safe_error(error: AppError) -> async_graphql::Error {
    // THREAT: internal Display text can contain credentials and database diagnostics.
    let (message, code) = match error {
        AppError::FieldValidation { detail, errors } => {
            let fields = errors
                .into_iter()
                .map(|(name, message)| {
                    (
                        async_graphql::Name::new(name),
                        async_graphql::Value::String(message),
                    )
                })
                .collect();
            return async_graphql::Error::new(detail).extend_with(|_, extensions| {
                extensions.set("code", "validation");
                extensions.set("errors", async_graphql::Value::Object(fields));
            });
        }
        AppError::Validation(detail) => {
            return async_graphql::Error::new(detail)
                .extend_with(|_, extensions| extensions.set("code", "validation"));
        }
        AppError::Forbidden => ("Forbidden", "forbidden"),
        AppError::NotFound => ("Not found", "not-found"),
        AppError::IfMatchRequired => ("A baseline is required.", "if-match-required"),
        AppError::IfMatchMismatch => ("The baseline is stale.", "if-match-mismatch"),
        AppError::MalformedHeader(detail) => {
            return async_graphql::Error::new(detail)
                .extend_with(|_, extensions| extensions.set("code", "malformed-header"));
        }
        AppError::UndoUnavailable => ("Metadata Undo is unavailable.", "undo-unavailable"),
        AppError::UndoConflict => ("Metadata Undo state conflicts.", "undo-conflict"),
        AppError::EditTooLarge => (crate::error::EDIT_TOO_LARGE, "edit-too-large"),
        AppError::SystemShelfImmutable => {
            ("System shelf cannot be modified.", "system-shelf-immutable")
        }
        AppError::FieldLocked(fields) => {
            return async_graphql::Error::new("Metadata fields are locked.").extend_with(
                |_, extensions| {
                    extensions.set("code", "field-locked");
                    extensions.set("lockedFields", fields);
                },
            );
        }
        error => {
            tracing::error!(error = ?error, "GraphQL execution failed");
            ("An internal error occurred.", "internal")
        }
    };
    async_graphql::Error::new(message).extend_with(|_, extensions| extensions.set("code", code))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    id: Option<String>,
    query: Option<String>,
    operation_name: Option<String>,
    #[serde(default)]
    variables: serde_json::Map<String, serde_json::Value>,
}

fn document(
    envelope: &Envelope,
    policy: ExecutionPolicy,
    documents: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    // THREAT: submitted text never registers itself in the production document map.
    if policy == ExecutionPolicy::Production && envelope.query.is_some() {
        return Err(AppError::Validation(
            "Only trusted document IDs are accepted.".into(),
        ));
    }
    if let Some(id) = &envelope.id {
        return documents
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Validation("Unknown document ID.".into()));
    }
    if policy == ExecutionPolicy::Development
        && let Some(query) = &envelope.query
    {
        return Ok(query.clone());
    }
    Err(AppError::Validation(
        "A trusted document ID is required.".into(),
    ))
}

#[derive(Clone)]
struct Service {
    schema: AppSchema,
    policy: ExecutionPolicy,
    documents: Arc<BTreeMap<String, String>>,
}

#[utoipa::path(
    post, path = "/api/graphql", tag = "graphql",
    request_body = Object,
    responses(
        (status = 200, description = "GraphQL execution response", body = Object),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json")
    ),
    security(("session_cookie" = []), ("device_token_bearer" = []), ("opds_basic" = []))
)]
async fn execute(
    State(state): State<AppState>,
    Extension(service): Extension<Service>,
    user: CurrentUser,
    session: tower_sessions::Session,
    ApiJson(envelope): ApiJson<Envelope>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let text = document(&envelope, service.policy, &service.documents)?;
    let mut request = async_graphql::Request::new(text)
        .variables(async_graphql::Variables::from_json(
            serde_json::Value::Object(envelope.variables),
        ))
        .data(loaders::request_loader(user.clone(), state.clone()))
        .data(state.clone())
        .data(user)
        .data(session)
        .data(state.pool);
    if let Some(name) = envelope.operation_name {
        request = request.operation_name(name);
    }
    let mut response = service.schema.execute(request).await;
    for error in &mut response.errors {
        if error.extensions.is_none() {
            let mut extensions = async_graphql::ErrorExtensionValues::default();
            extensions.set("code", "validation");
            error.extensions = Some(extensions);
        }
    }
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        GraphQLResponse::from(response),
    ))
}

pub(crate) fn router() -> OpenApiRouter<AppState> {
    let policy = ExecutionPolicy::for_build();
    #[expect(
        clippy::expect_used,
        reason = "The embedded trusted-document map is generated as valid JSON and drift-checked."
    )]
    let documents = serde_json::from_str(include_str!("../../graphql-documents.json"))
        .expect("The embedded trusted-document map must be valid JSON.");
    OpenApiRouter::new()
        .routes(routes!(execute))
        .layer(Extension(Service {
            schema: schema(policy),
            policy,
            documents: Arc::new(documents),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token;
    use crate::models::device_token;
    use crate::test_support::{self, db};
    use axum_test::TestServer;
    use serde_json::json;
    use sqlx::PgPool;
    use tower_sessions::{MemoryStore, Session};

    async fn server(pool: &PgPool) -> (TestServer, String, uuid::Uuid) {
        let app = db::app_pool_for(pool).await;
        let (user_id, auth) = db::create_admin_and_basic_auth(&app).await;
        let mut state = test_support::test_state();
        state.pool = app;
        state.ingestion_pool = db::ingestion_pool_for(pool).await;
        let router = crate::build_router_with_session_store(state, MemoryStore::default());
        (TestServer::new(router), auth, user_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_request_boundary(pool: PgPool) {
        let (server, auth, user_id) = server(&pool).await;
        let query = json!({"query":"{ viewer { id displayName isChild } }"});
        server
            .post("/api/graphql")
            .json(&query)
            .await
            .assert_status_unauthorized();
        server
            .post("/api/graphql")
            .add_header("authorization", "Bearer invalid")
            .json(&query)
            .await
            .assert_status_unauthorized();
        let app = db::app_pool_for(&pool).await;
        let (secret, hash) = token::generate_device_token();
        let token =
            device_token::create_with_limit(&app, user_id, "graphql", &hash, &[Scope::Read], None)
                .await
                .unwrap();
        let bearer = format!("Bearer {}{}.{}", token::TOKEN_PREFIX, token.id, secret);
        let response = server
            .post("/api/graphql")
            .add_header("authorization", bearer)
            .json(&query)
            .await;
        response.assert_status_ok();
        assert_eq!(
            response.json::<serde_json::Value>()["data"]["viewer"]["id"],
            id::User::encode(user_id).as_str()
        );
        assert!(
            response
                .header("content-security-policy")
                .to_str()
                .unwrap()
                .contains("default-src 'none'")
        );
        for (body, content_type, status) in [
            ("{".to_owned(), "application/json", 400),
            ("[]".to_owned(), "application/json", 422),
            (serde_json::to_string(&json!({"query": format!("{{ {} }}", (0..150_000).map(|i| format!("alias{i}: __typename")).collect::<Vec<_>>().join(" "))})).unwrap(), "application/json", 413),
            ("{}".to_owned(), "text/plain", 415),
        ] {
            let response = server
                .post("/api/graphql")
                .add_header("authorization", auth.clone())
                .text(body)
                .content_type(content_type)
                .await;
            assert_eq!(response.status_code().as_u16(), status);
            assert_eq!(response.content_type(), "application/problem+json");
        }
        let store = MemoryStore::default();
        let session = Session::new(None, std::sync::Arc::new(store.clone()), None);
        session.insert("user_id", user_id).await.unwrap();
        session.insert("session_version", 0_i32).await.unwrap();
        session
            .insert("csrf_token", "graphql-test-csrf")
            .await
            .unwrap();
        session.save().await.unwrap();
        let mut state = test_support::test_state();
        state.pool = app;
        let session_server = TestServer::new(crate::build_router_with_session_store(state, store));
        let cookie = format!("id={}", session.id().unwrap());
        for (csrf, status) in [
            (None, 428),
            (Some("wrong"), 403),
            (Some("graphql-test-csrf"), 200),
        ] {
            let request = session_server
                .post("/api/graphql")
                .add_header("cookie", cookie.clone())
                .json(&query);
            let request = if let Some(csrf) = csrf {
                request.add_header("x-csrf-token", csrf)
            } else {
                request
            };
            assert_eq!(request.await.status_code().as_u16(), status);
        }
        mutation_scope_boundary(&pool).await;
        let error = safe_error(AppError::Internal(anyhow::anyhow!(
            "private-database-credential"
        )));
        assert_eq!(error.message, "An internal error occurred.");
        assert!(!format!("{error:?}").contains("private-database-credential"));
        let denied = safe_error(AppError::Forbidden);
        assert_eq!(
            denied.extensions.unwrap().get("code"),
            Some(&async_graphql::Value::from("forbidden"))
        );
        let fields = safe_error(AppError::FieldValidation {
            detail: "Invalid input".into(),
            errors: BTreeMap::from([("name".into(), "Required".into())]),
        });
        assert_eq!(fields.message, "Invalid input");
        assert_eq!(
            fields.extensions.unwrap().get("errors"),
            Some(&async_graphql::value!({"name": "Required"}))
        );
    }

    fn assert_admitted(name: &str, admitted: &serde_json::Value) {
        let code = &admitted["errors"][0]["extensions"]["code"];
        assert_ne!(code, "forbidden", "{name}: {admitted}");
        assert_ne!(code, "validation", "{name}: {admitted}");
        assert_ne!(
            admitted["data"][name]["__typename"], "MetadataInvalid",
            "{name}: {admitted}"
        );
    }

    async fn mutation_scope_boundary(pool: &PgPool) {
        let fixture = reader_fixture(pool).await;
        let app = db::app_pool_for(pool).await;
        let mut credentials = Vec::new();
        for scope in [Scope::Read, Scope::Write] {
            let (secret, hash) = token::generate_device_token();
            let record = device_token::create_with_limit(
                &app,
                fixture.admin_id,
                "graph-scope",
                &hash,
                &[scope],
                None,
            )
            .await
            .unwrap();
            credentials.push(format!(
                "Bearer {}{}.{}",
                token::TOKEN_PREFIX,
                record.id,
                secret
            ));
        }
        let pending=sqlx::query_scalar!("SELECT id FROM metadata_versions WHERE manifestation_id=$1 AND status='pending'::metadata_review_status LIMIT 1",fixture.visible).fetch_one(pool).await.unwrap();
        let pending = id::MetadataVersion::encode(pending).as_str().to_owned();
        let id = id::Manifestation::encode(fixture.visible)
            .as_str()
            .to_owned();
        let baseline=reader_query(&fixture,&fixture.admin,TITLE_READ,json!({"id":id})).await["data"]["manifestation"]["metadata"]["etag"].clone();
        let mutations = mutation_shapes(&fixture, &pending, &baseline);
        let inventory = schema(ExecutionPolicy::Development)
            .execute("{ __type(name:\"Mutation\") { fields { name } } }")
            .await;
        assert_eq!(inventory.errors, []);
        let inventory = serde_json::to_value(inventory.data).unwrap();
        let roots: std::collections::BTreeSet<_> = inventory["__type"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["name"].as_str().unwrap())
            .collect();
        let covered: std::collections::BTreeSet<_> =
            mutations.iter().map(|(name, _)| *name).collect();
        assert!(!roots.is_empty());
        assert_eq!(roots, covered);
        for (name, body) in &mutations {
            let query = format!("mutation {{ {body} }}");
            let refused = reader_query(&fixture, &credentials[0], &query, json!({})).await;
            assert_graph_code(&refused, "forbidden");
            for auth in [&credentials[1], &fixture.admin]
                .into_iter()
                .filter(|_| *name != "undoMetadata")
            {
                let admitted = reader_query(&fixture, auth, &query, json!({})).await;
                assert_admitted(name, &admitted);
                if *name == "createShelf" {
                    assert_graph_ok(&admitted);
                    assert_eq!(admitted["data"][name]["shelf"]["name"], "Scoped shelf");
                }
            }
            if matches!(
                *name,
                "updateMetadata"
                    | "refreshMetadata"
                    | "createShelf"
                    | "renameShelf"
                    | "deleteShelf"
                    | "acceptMetadataVersion"
                    | "rejectMetadataVersion"
                    | "revertMetadataField"
                    | "lockMetadataField"
                    | "unlockMetadataField"
                    | "undoMetadata"
            ) {
                assert_graph_code(
                    &reader_query(&fixture, &fixture.child, &query, json!({})).await,
                    "forbidden",
                );
            }
        }
        assert_graph_code(
            &reader_query(
                &fixture,
                &credentials[0],
                RESOLVE,
                json!({"id":id,"request":uuid::Uuid::new_v4()}),
            )
            .await,
            "forbidden",
        );
        mutation_session_controls(pool, &id).await;
        mutation_child_controls(pool, &fixture, &id).await;

        preference_scope_boundary(&fixture, &credentials[0]).await;
        work_scope_boundary(&fixture).await;
    }

    async fn work_scope_boundary(fixture: &ReaderFixture) {
        let query = "query($id:ID!) { fetch__Work(id:$id) { id } }";
        let denied = schema(ExecutionPolicy::Development)
            .execute(
                async_graphql::Request::new(query)
                    .variables(async_graphql::Variables::from_json(
                        json!({"id":id::Work::encode(fixture.visible_work)}),
                    ))
                    .data(CurrentUser::without_scopes_for_test(fixture.admin_id)),
            )
            .await;
        assert_eq!(denied.errors.len(), 1);
        assert_eq!(
            denied.errors[0].extensions.as_ref().unwrap().get("code"),
            Some(&async_graphql::Value::from("forbidden"))
        );
        let hidden = reader_query(
            fixture,
            &fixture.child,
            query,
            json!({"id":id::Work::encode(fixture.hidden_work)}),
        )
        .await;
        assert_graph_ok(&hidden);
        assert!(hidden["data"]["fetch__Work"].is_null());
    }

    async fn preference_scope_boundary(fixture: &ReaderFixture, read_credential: &str) {
        let preference_read = "{ libraryPreferences { hiddenColumns density view sortStack defaults { hiddenColumns density view sortStack } } }";
        let denied = schema(ExecutionPolicy::Development)
            .execute(
                async_graphql::Request::new(preference_read)
                    .data(CurrentUser::without_scopes_for_test(fixture.admin_id)),
            )
            .await;
        assert_eq!(denied.errors.len(), 1);
        assert_eq!(
            denied.errors[0].extensions.as_ref().unwrap().get("code"),
            Some(&async_graphql::Value::from("forbidden"))
        );
        let read = reader_query(fixture, read_credential, preference_read, json!({})).await;
        assert_graph_ok(&read);
        assert_eq!(read["data"]["libraryPreferences"]["density"], "compact");
        assert_eq!(
            read["data"]["libraryPreferences"]["defaults"]["hiddenColumns"],
            json!(["subtitle"])
        );
        let child_initial = reader_query(fixture, &fixture.child, preference_read, json!({})).await;
        assert_graph_ok(&child_initial);
        assert!(child_initial["data"]["libraryPreferences"]["density"].is_null());
        let patch = "mutation($patch:LibraryPreferencesPatch!) { updateLibraryPreferences(patch:$patch) { hiddenColumns density view sortStack } }";
        let child_write = reader_query(
            fixture,
            &fixture.child,
            patch,
            json!({"patch":{"density":"COMFORTABLE","hiddenColumns":[]}}),
        )
        .await;
        assert_graph_ok(&child_write);
        assert_eq!(
            child_write["data"]["updateLibraryPreferences"]["density"],
            "comfortable"
        );
        assert_eq!(
            child_write["data"]["updateLibraryPreferences"]["hiddenColumns"],
            json!([])
        );
        let unchanged = reader_query(fixture, &fixture.admin, preference_read, json!({})).await;
        assert_eq!(
            unchanged["data"]["libraryPreferences"]["density"],
            "compact"
        );
        assert!(unchanged["data"]["libraryPreferences"]["hiddenColumns"].is_null());
        let reset = reader_query(
            fixture,
            &fixture.child,
            patch,
            json!({"patch":{"density":null}}),
        )
        .await;
        assert_graph_ok(&reset);
        assert!(reset["data"]["updateLibraryPreferences"]["density"].is_null());
        assert_eq!(
            reset["data"]["updateLibraryPreferences"]["hiddenColumns"],
            json!([])
        );
        for patch_value in [
            json!({}),
            json!({"density":"COMPACT","sortStack":"bad_column"}),
            json!({"hiddenColumns":["bad column"]}),
        ] {
            assert_graph_code(
                &reader_query(fixture, &fixture.child, patch, json!({"patch":patch_value})).await,
                "validation",
            );
        }
        let after_refusal = reader_query(fixture, &fixture.child, preference_read, json!({})).await;
        assert!(after_refusal["data"]["libraryPreferences"]["density"].is_null());
        let missing = id::Manifestation::encode(uuid::Uuid::new_v4());
        assert_graph_code(
            &reader_query(
                fixture,
                &fixture.admin,
                "mutation($id:ID!) { refreshMetadata(id:$id) { manifestationId } }",
                json!({"id":missing}),
            )
            .await,
            "not-found",
        );
    }

    fn mutation_shapes(
        fixture: &ReaderFixture,
        pending: &str,
        baseline: &serde_json::Value,
    ) -> Vec<(&'static str, String)> {
        let id = id::Manifestation::encode(fixture.visible)
            .as_str()
            .to_owned();
        let shelf = id::Shelf::encode(fixture.shelf_a).as_str().to_owned();
        let delete_shelf = id::Shelf::encode(fixture.shelf_b).as_str().to_owned();
        let tag = serde_json::to_string(&baseline).unwrap();
        vec![
            (
                "updateLibraryPreferences",
                "updateLibraryPreferences(patch: { density: COMPACT }) { density }".to_owned(),
            ),
            (
                "refreshMetadata",
                format!("refreshMetadata(id:\"{id}\") {{ manifestationId }}"),
            ),
            (
                "updateMetadata",
                format!(
                    "updateMetadata(input:{{id:\"{id}\",baseline:{tag},patch:{{title:\"Scoped\"}}}}) {{ __typename }}"
                ),
            ),
            (
                "updateReadingState",
                format!("updateReadingState(id:\"{id}\",patch:{{notes:\"Scoped\"}}) {{ etag }}"),
            ),
            (
                "createShelf",
                "createShelf(input:{name:\"Scoped shelf\"}) { ... on ShelfCreated { shelf { id name } } }".to_owned(),
            ),
            (
                "renameShelf",
                format!("renameShelf(input:{{id:\"{shelf}\",name:\"Scoped rename\"}}) {{ __typename }}"),
            ),
            (
                "deleteShelf",
                format!("deleteShelf(input:{{id:\"{delete_shelf}\"}}) {{ __typename }}"),
            ),
            (
                "addShelfItem",
                format!(
                    "addShelfItem(input:{{shelfId:\"{shelf}\",manifestationId:\"{id}\"}}) {{ __typename }}"
                ),
            ),
            (
                "removeShelfItem",
                format!(
                    "removeShelfItem(input:{{shelfId:\"{shelf}\",manifestationId:\"{id}\"}}) {{ __typename }}"
                ),
            ),
            (
                "acceptMetadataVersion",
                format!(
                    "acceptMetadataVersion(input:{{id:\"{id}\",versionId:\"{pending}\"}}) {{ __typename }}"
                ),
            ),
            (
                "rejectMetadataVersion",
                format!(
                    "rejectMetadataVersion(input:{{id:\"{id}\",versionId:\"{pending}\"}}) {{ __typename }}"
                ),
            ),
            (
                "revertMetadataField",
                format!(
                    "revertMetadataField(input:{{id:\"{id}\",fieldName:\"subtitle\"}}) {{ __typename }}"
                ),
            ),
            (
                "lockMetadataField",
                format!(
                    "lockMetadataField(input:{{id:\"{id}\",fieldName:\"subtitle\",entityType:WORK}}) {{ __typename }}"
                ),
            ),
            (
                "unlockMetadataField",
                format!(
                    "unlockMetadataField(input:{{id:\"{id}\",fieldName:\"subtitle\",entityType:WORK}}) {{ __typename }}"
                ),
            ),
            (
                "undoMetadata",
                format!(
                    "undoMetadata(input:{{id:\"{id}\",baseline:{tag},operationId:\"{}\"}}) {{ __typename }}",
                    uuid::Uuid::new_v4()
                ),
            ),
        ]
    }
    async fn mutation_session_controls(pool: &PgPool, id: &str) {
        for (name, admin) in [
            ("graph-write-session", false),
            ("graph-admin-session", true),
        ] {
            let (user, browser, csrf) = editing_browser(pool, name).await;
            if admin {
                sqlx::query!("UPDATE users SET role = 'admin' WHERE id = $1", user)
                    .execute(pool)
                    .await
                    .unwrap();
            }
            let current = browser_graph(&browser, &csrf, TITLE_READ, json!({"id":id})).await;
            let saved = browser_graph(&browser, &csrf, TITLE_SAVE, json!({"id":id,"baseline":current["data"]["manifestation"]["metadata"]["etag"],"patch":{"title":name}})).await;
            assert_graph_ok(&saved);
            let operation = &saved["data"]["updateMetadata"]["operations"][0]["operationId"];
            let resolved = browser_graph(
                &browser,
                &csrf,
                RESOLVE,
                json!({"id":id,"operation":operation}),
            )
            .await;
            assert_graph_ok(&resolved);
            assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "SAVED");
            let restored = browser_graph(&browser, &csrf, UNDO, json!({"id":id,"baseline":saved["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"],"operation":operation})).await;
            assert_graph_result(&restored, "undoMetadata", "MetadataRestored");
        }
    }
    async fn mutation_child_controls(pool: &PgPool, fixture: &ReaderFixture, id: &str) {
        let shelf = id::Shelf::encode(fixture.shelf_a).as_str().to_owned();
        let own = reader_query(fixture, &fixture.child, READING_READ, json!({"id":id})).await;
        let child_saved=reader_query(fixture,&fixture.child,READING_SAVE,json!({"id":id,"baseline":own["data"]["manifestation"]["readingState"]["etag"],"patch":{"notes":"Child's change"}})).await;
        assert_graph_ok(&child_saved);
        let adult = reader_query(fixture, &fixture.admin, READING_READ, json!({"id":id})).await;
        assert_eq!(
            adult["data"]["manifestation"]["readingState"]["notes"],
            "Admin note"
        );
        let hidden = id::Manifestation::encode(fixture.hidden);
        assert_graph_code(&reader_query(fixture,&fixture.child,READING_SAVE,json!({"id":hidden,"baseline":own["data"]["manifestation"]["readingState"]["etag"],"patch":{"notes":"Hidden"}})).await,"not-found");
        let membership = "mutation($shelf:ID!,$id:ID!) { addShelfItem(input:{shelfId:$shelf,manifestationId:$id}) { __typename } }";
        assert_graph_result(
            &reader_query(
                fixture,
                &fixture.child,
                membership,
                json!({"shelf":id::Shelf::encode(fixture.child_shelf),"id":id}),
            )
            .await,
            "addShelfItem",
            "ShelfItemAdded",
        );
        assert_graph_result(
            &reader_query(
                fixture,
                &fixture.child,
                membership,
                json!({"shelf":id::Shelf::encode(fixture.child_shelf),"id":hidden}),
            )
            .await,
            "addShelfItem",
            "NotFound",
        );
        assert_graph_result(
            &reader_query(
                fixture,
                &fixture.admin,
                membership,
                json!({"shelf":id::Shelf::encode(fixture.other_shelf),"id":id}),
            )
            .await,
            "addShelfItem",
            "NotFound",
        );
        sqlx::query!(
            "UPDATE shelves SET is_system=TRUE WHERE id=$1",
            fixture.shelf_a
        )
        .execute(pool)
        .await
        .unwrap();
        for (field, query) in [
            (
                "renameShelf",
                format!(
                    "mutation {{ renameShelf(input:{{id:\"{shelf}\",name:\"Refused\"}}) {{ __typename }} }}"
                ),
            ),
            (
                "deleteShelf",
                format!("mutation {{ deleteShelf(input:{{id:\"{shelf}\"}}) {{ __typename }} }}"),
            ),
        ] {
            assert_graph_result(
                &reader_query(fixture, &fixture.admin, &query, json!({})).await,
                field,
                "SystemShelfImmutable",
            );
        }
    }
    async fn editing_browser(pool: &PgPool, name: &str) -> (uuid::Uuid, TestServer, String) {
        let app = db::app_pool_for(pool).await;
        let ing = db::ingestion_pool_for(pool).await;
        let email = format!("{name}@example.test");
        let user =
            db::create_adult_with_password(&app, name, &email, test_support::mail::PASSWORD).await;
        let mut server = db::server_with_real_pools(&app, &ing);
        server.save_cookies();
        let csrf = test_support::mail::login(&server, &email, test_support::mail::PASSWORD).await;
        (user, server, csrf)
    }
    async fn browser_graph(
        server: &TestServer,
        csrf: &str,
        query: &str,
        variables: serde_json::Value,
    ) -> serde_json::Value {
        let response = server
            .post("/api/graphql")
            .add_header("x-csrf-token", csrf.to_owned())
            .json(&json!({"query":query,"variables":variables}))
            .await;
        response.assert_status_ok();
        response.json()
    }
    fn assert_graph_ok(value: &serde_json::Value) {
        assert!(value.get("errors").is_none(), "{value}");
    }
    fn assert_graph_code(value: &serde_json::Value, code: &str) {
        assert_eq!(value["errors"][0]["extensions"]["code"], code, "{value}");
    }
    fn assert_graph_result(value: &serde_json::Value, field: &str, typename: &str) {
        assert_graph_ok(value);
        assert_eq!(value["data"][field]["__typename"], typename, "{value}");
    }
    const TITLE_READ: &str =
        "query($id: ID!) { manifestation(id: $id) { metadata { title etag } } }";
    const TITLE_SAVE: &str = "mutation($id: ID!, $baseline: ETag!, $request: UUID, $patch: MetadataPatch!) { updateMetadata(input: {id: $id, baseline: $baseline, requestId: $request, patch: $patch}) { __typename ... on MetadataSaved { requestId operations { operationId fieldName expiresAt status } manifestation { metadata { title etag } } } } }";
    const READING_READ: &str =
        "query($id: ID!) { manifestation(id: $id) { readingState { notes rating status etag } } }";
    const READING_SAVE: &str = "mutation($id: ID!, $baseline: String, $patch: ReadingPatch!) { updateReadingState(id: $id, baseline: $baseline, patch: $patch) { notes rating status etag } }";
    const RESOLVE: &str = "query($id: ID!, $request: UUID, $operation: UUID) { metadataOperation(id: $id, requestId: $request, operationId: $operation) { outcome operations { operationId fieldName expiresAt status } metadata { title etag } } }";
    const UNDO: &str = "mutation($id: ID!, $baseline: ETag!, $operation: UUID!) { undoMetadata(input: {id: $id, baseline: $baseline, operationId: $operation}) { __typename ... on MetadataRestored { operation { operationId fieldName expiresAt status } manifestation { metadata { title etag } } } ... on FieldsLocked { fields } } }";
    fn assert_request_error(value: &serde_json::Value) {
        assert!(
            value["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()),
            "{value}"
        );
        assert!(
            value.get("data").is_none_or(serde_json::Value::is_null),
            "{value}"
        );
    }

    // REV-REQ-0018/0020/0082 GraphQL preconditions; data-integrity boundary for selected revisions and explicit resend.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_edit_baselines(pool: PgPool) {
        let (_, server, csrf) = editing_browser(&pool, "graph-edits").await;
        let ing = db::ingestion_pool_for(&pool).await;
        let (_, edition) = db::insert_work_and_manifestation(&ing, "graph-edits").await;
        let id = id::Manifestation::encode(edition);
        let vars = json!({"id":id});
        let initial = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_graph_ok(&initial);
        let original = initial["data"]["manifestation"]["metadata"].clone();
        assert_eq!(original.as_object().unwrap().len(), 2);
        let request = uuid::Uuid::new_v4();
        let attempt = json!({"id":id,"baseline":original["etag"],"request":request,"patch":{"title":"Edited title"}});
        let mut missing = attempt.clone();
        missing["baseline"] = serde_json::Value::Null;
        assert_request_error(&browser_graph(&server, &csrf, TITLE_SAVE, missing).await);
        let mut omitted = attempt.clone();
        omitted.as_object_mut().unwrap().remove("baseline");
        assert_request_error(&browser_graph(&server, &csrf, TITLE_SAVE, omitted).await);
        let mut weak = attempt.clone();
        weak["baseline"] = json!("weak");
        let weak = browser_graph(&server, &csrf, TITLE_SAVE, weak).await;
        assert_request_error(&weak);
        assert_graph_code(&weak, "validation");
        let mut stale = attempt.clone();
        stale["baseline"] = json!("\"stale\"");
        assert_graph_result(
            &browser_graph(&server, &csrf, TITLE_SAVE, stale).await,
            "updateMetadata",
            "StaleBaseline",
        );
        let mut stale_invalid = attempt.clone();
        stale_invalid["baseline"] = json!("\"stale\"");
        stale_invalid["patch"] = json!({});
        assert_graph_result(
            &browser_graph(&server, &csrf, TITLE_SAVE, stale_invalid).await,
            "updateMetadata",
            "StaleBaseline",
        );
        let unchanged = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_eq!(unchanged["data"]["manifestation"]["metadata"], original);
        let saved = browser_graph(&server, &csrf, TITLE_SAVE, attempt.clone()).await;
        assert_graph_result(&saved, "updateMetadata", "MetadataSaved");
        let ack = &saved["data"]["updateMetadata"]["manifestation"]["metadata"];
        assert_eq!(ack["title"], "Edited title");
        assert_ne!(ack["etag"], original["etag"]);
        let matched = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_eq!(*ack, matched["data"]["manifestation"]["metadata"]);
        assert_graph_result(
            &browser_graph(&server, &csrf, TITLE_SAVE, attempt).await,
            "updateMetadata",
            "StaleBaseline",
        );
        let resolved =
            browser_graph(&server, &csrf, RESOLVE, json!({"id":id,"request":request})).await;
        assert_graph_ok(&resolved);
        assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "SAVED");
        assert_eq!(
            resolved["data"]["metadataOperation"]["operations"],
            saved["data"]["updateMetadata"]["operations"]
        );
        let reused = browser_graph(
            &server,
            &csrf,
            TITLE_SAVE,
            json!({"id":id,"baseline":ack["etag"],"request":request,"patch":{"title":"Reuse"}}),
        )
        .await;
        assert_graph_result(&reused, "updateMetadata", "MetadataInvalid");
        let unknown = browser_graph(
            &server,
            &csrf,
            RESOLVE,
            json!({"id":id,"request":uuid::Uuid::new_v4()}),
        )
        .await;
        assert_eq!(unknown["data"]["metadataOperation"]["outcome"], "UNKNOWN");
        let sparse = "mutation($id:ID!, $baseline:ETag!, $patch:MetadataPatch!) { updateMetadata(input:{id:$id,baseline:$baseline,patch:$patch}) { ... on MetadataSaved { manifestation { metadata { title subtitle etag } } } } }";
        let subtitle = browser_graph(
            &server,
            &csrf,
            sparse,
            json!({"id":id,"baseline":ack["etag"],"patch":{"subtitle":"Keep"}}),
        )
        .await;
        assert_graph_ok(&subtitle);
        let retained = browser_graph(&server,&csrf,sparse,json!({"id":id,"baseline":subtitle["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"],"patch":{"title":"Still edited"}})).await;
        assert_graph_ok(&retained);
        assert_eq!(
            retained["data"]["updateMetadata"]["manifestation"]["metadata"]["subtitle"],
            "Keep"
        );
        let cleared = browser_graph(&server,&csrf,sparse,json!({"id":id,"baseline":retained["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"],"patch":{"subtitle":null}})).await;
        assert_graph_ok(&cleared);
        assert!(
            cleared["data"]["updateMetadata"]["manifestation"]["metadata"]["subtitle"].is_null()
        );
        let shared = cleared["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"].clone();
        let (left, right) = tokio::join!(
            browser_graph(
                &server,
                &csrf,
                TITLE_SAVE,
                json!({"id":id,"baseline":shared,"patch":{"title":"Left"}})
            ),
            browser_graph(
                &server,
                &csrf,
                TITLE_SAVE,
                json!({"id":id,"baseline":shared,"patch":{"title":"Right"}})
            )
        );
        let outcomes: std::collections::BTreeSet<_> = [&left, &right]
            .into_iter()
            .map(|value| {
                assert_graph_ok(value);
                value["data"]["updateMetadata"]["__typename"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            outcomes,
            ["MetadataSaved", "StaleBaseline"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );
        let now = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        let names: Vec<_> = (0..22).map(|n| format!("Author {n:02}")).collect();
        let author_save = "mutation($id:ID!, $baseline:ETag!, $patch:MetadataPatch!) { updateMetadata(input:{id:$id,baseline:$baseline,patch:$patch}) { ... on MetadataSaved { manifestation { metadata { etag authors(first:20) { edges { cursor node { name } } pageInfo { hasNextPage hasPreviousPage endCursor } } } } } } }";
        let authors = browser_graph(&server,&csrf,author_save,json!({"id":id,"baseline":now["data"]["manifestation"]["metadata"]["etag"],"patch":{"contributors":{"author":names}}})).await;
        assert_graph_ok(&authors);
        let first = &authors["data"]["updateMetadata"]["manifestation"]["metadata"];
        assert_eq!(first["authors"]["edges"].as_array().unwrap().len(), 20);
        assert_eq!(first["authors"]["pageInfo"]["hasNextPage"], true);
        let pages = "query($id:ID!, $after:String) { manifestation(id:$id) { metadata { etag authors(first:20,after:$after) { edges { node { name } } pageInfo { hasNextPage hasPreviousPage } } one:authors(first:1) { edges { node { name } } } two:authors(first:2,after:$after) { edges { node { name } } } } } }";
        let rest = browser_graph(
            &server,
            &csrf,
            pages,
            json!({"id":id,"after":first["authors"]["pageInfo"]["endCursor"]}),
        )
        .await;
        assert_graph_ok(&rest);
        let rest = &rest["data"]["manifestation"]["metadata"];
        assert_eq!(rest["etag"], first["etag"]);
        assert_eq!(rest["authors"]["edges"].as_array().unwrap().len(), 2);
        assert_eq!(rest["authors"]["pageInfo"]["hasNextPage"], false);
        assert_eq!(rest["authors"]["pageInfo"]["hasPreviousPage"], true);
        assert_eq!(rest["one"]["edges"][0]["node"]["name"], "Author 00");
        assert_eq!(rest["two"]["edges"][0]["node"]["name"], "Author 20");
        let revised = browser_graph(&server,&csrf,author_save,json!({"id":id,"baseline":first["etag"],"patch":{"contributors":{"author":["Changed author"]}}})).await;
        assert_graph_ok(&revised);
        assert_ne!(
            revised["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"],
            first["etag"]
        );
        let read = browser_graph(&server, &csrf, READING_READ, vars.clone()).await;
        for (baseline, code) in [
            (serde_json::Value::Null, "if-match-required"),
            (json!("W/\"tag\""), "malformed-header"),
            (json!("\"old\""), "if-match-mismatch"),
        ] {
            assert_graph_code(
                &browser_graph(
                    &server,
                    &csrf,
                    READING_SAVE,
                    json!({"id":id,"baseline":baseline,"patch":{"notes":"Refused"}}),
                )
                .await,
                code,
            );
        }
        assert_graph_code(
            &browser_graph(
                &server,
                &csrf,
                READING_SAVE,
                json!({"id":id,"patch":{"notes":"Refused"}}),
            )
            .await,
            "if-match-required",
        );
        let reading_attempt = json!({"id":id,"baseline":read["data"]["manifestation"]["readingState"]["etag"],"patch":{"notes":"Notes","rating":3,"status":"READING"}});
        let read_saved = browser_graph(&server, &csrf, READING_SAVE, reading_attempt.clone()).await;
        assert_graph_ok(&read_saved);
        assert_graph_code(
            &browser_graph(&server, &csrf, READING_SAVE, reading_attempt).await,
            "if-match-mismatch",
        );
        let read_back = browser_graph(&server, &csrf, READING_READ, vars.clone()).await;
        assert_eq!(
            read_saved["data"]["updateReadingState"],
            read_back["data"]["manifestation"]["readingState"]
        );
        let notes_clear = browser_graph(&server,&csrf,READING_SAVE,json!({"id":id,"baseline":read_saved["data"]["updateReadingState"]["etag"],"patch":{"notes":null}})).await;
        assert_graph_ok(&notes_clear);
        assert!(notes_clear["data"]["updateReadingState"]["notes"].is_null());
        assert_eq!(notes_clear["data"]["updateReadingState"]["rating"], 3);
    }

    // Authentication, private-data and corruption boundaries for session-bound, non-consuming exact Undo.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_undo_boundary(pool: PgPool) {
        let (user, server, csrf) = editing_browser(&pool, "graph-undo").await;
        let (_, foreign, foreign_csrf) = editing_browser(&pool, "graph-foreign").await;
        let app = db::app_pool_for(&pool).await;
        let ing = db::ingestion_pool_for(&pool).await;
        let (_, edition) = db::insert_work_and_manifestation(&ing, "graph-undo").await;
        let (_, other_edition) =
            db::insert_work_and_manifestation(&ing, "graph-other-edition").await;
        let id = id::Manifestation::encode(edition);
        let other_id = id::Manifestation::encode(other_edition);
        let read = browser_graph(&server, &csrf, TITLE_READ, json!({"id":id})).await;
        let original = &read["data"]["manifestation"]["metadata"];
        let saved = browser_graph(&server,&csrf,TITLE_SAVE,json!({"id":id,"baseline":original["etag"],"request":uuid::Uuid::new_v4(),"patch":{"title":"Undo this"}})).await;
        assert_graph_ok(&saved);
        let result = &saved["data"]["updateMetadata"];
        let operation = &result["operations"][0]["operationId"];
        assert_eq!(result["operations"][0]["status"], "ELIGIBLE");
        let input = json!({"id":id,"baseline":result["manifestation"]["metadata"]["etag"],"operation":operation});
        let mut missing = input.clone();
        missing["baseline"] = serde_json::Value::Null;
        assert_request_error(&browser_graph(&server, &csrf, UNDO, missing).await);
        let mut omitted = input.clone();
        omitted.as_object_mut().unwrap().remove("baseline");
        assert_request_error(&browser_graph(&server, &csrf, UNDO, omitted).await);
        let mut wildcard = input.clone();
        wildcard["baseline"] = json!("*");
        let wildcard = browser_graph(&server, &csrf, UNDO, wildcard).await;
        assert_request_error(&wildcard);
        assert_graph_code(&wildcard, "validation");
        let mut stale = input.clone();
        stale["baseline"] = original["etag"].clone();
        assert_graph_result(
            &browser_graph(&server, &csrf, UNDO, stale).await,
            "undoMetadata",
            "StaleBaseline",
        );
        let foreign_result = browser_graph(&foreign, &foreign_csrf, UNDO, input.clone()).await;
        assert_graph_result(&foreign_result, "undoMetadata", "UndoUnavailable");
        let mut second = db::server_with_real_pools(&app, &ing);
        second.save_cookies();
        let second_csrf = test_support::mail::login(
            &second,
            "graph-undo@example.test",
            test_support::mail::PASSWORD,
        )
        .await;
        assert_graph_result(
            &browser_graph(&second, &second_csrf, UNDO, input.clone()).await,
            "undoMetadata",
            "UndoUnavailable",
        );
        let second_resolve = browser_graph(
            &second,
            &second_csrf,
            RESOLVE,
            json!({"id":id,"operation":operation}),
        )
        .await;
        assert_eq!(
            second_resolve["data"]["metadataOperation"]["outcome"],
            "UNKNOWN"
        );
        let other = browser_graph(&server, &csrf, TITLE_READ, json!({"id":other_id})).await;
        assert_graph_result(&browser_graph(&server,&csrf,UNDO,json!({"id":other_id,"baseline":other["data"]["manifestation"]["metadata"]["etag"],"operation":operation})).await,"undoMetadata","UndoUnavailable");
        let (_, basic) = db::create_admin_and_basic_auth(&app).await;
        let (secret, hash) = token::generate_device_token();
        let record = device_token::create_with_limit(
            &app,
            user,
            "graph-undo-token",
            &hash,
            &[Scope::Write],
            None,
        )
        .await
        .unwrap();
        let bearer = format!("Bearer {}{}.{}", token::TOKEN_PREFIX, record.id, secret);
        let credential_server = db::server_with_real_pools(&app, &ing);
        for auth in [basic, bearer] {
            for (query, vars) in [
                (UNDO, input.clone()),
                (RESOLVE, json!({"id":id,"operation":operation})),
            ] {
                let response = credential_server
                    .post("/api/graphql")
                    .add_header("authorization", auth.clone())
                    .json(&json!({"query":query,"variables":vars}))
                    .await;
                response.assert_status_ok();
                assert_graph_code(&response.json(), "forbidden");
            }
        }
        let lock = "mutation($id:ID!) { lockMetadataField(input:{id:$id,fieldName:\"title\",entityType:WORK}) { __typename } }";
        let unlock = "mutation($id:ID!) { unlockMetadataField(input:{id:$id,fieldName:\"title\",entityType:WORK}) { __typename } }";
        assert_graph_result(
            &browser_graph(&server, &csrf, lock, json!({"id":id})).await,
            "lockMetadataField",
            "MetadataFieldLockAdded",
        );
        let locked = browser_graph(&server, &csrf, UNDO, input.clone()).await;
        assert_graph_result(&locked, "undoMetadata", "FieldsLocked");
        assert_eq!(locked["data"]["undoMetadata"]["fields"], json!(["title"]));
        let resolved = browser_graph(
            &server,
            &csrf,
            RESOLVE,
            json!({"id":id,"operation":operation}),
        )
        .await;
        assert_graph_ok(&resolved);
        assert_eq!(
            resolved["data"]["metadataOperation"]["operations"][0]["status"],
            "LOCKED"
        );
        assert_eq!(
            resolved["data"]["metadataOperation"]["operations"][0]["expiresAt"],
            result["operations"][0]["expiresAt"]
        );
        let operation_uuid: uuid::Uuid = operation.as_str().unwrap().parse().unwrap();
        assert!(
            sqlx::query_scalar!(
                "SELECT consumed_at FROM metadata_undo_operations WHERE id=$1",
                operation_uuid
            )
            .fetch_one(&pool)
            .await
            .unwrap()
            .is_none()
        );
        assert_graph_result(
            &browser_graph(&server, &csrf, unlock, json!({"id":id})).await,
            "unlockMetadataField",
            "MetadataFieldLockRemoved",
        );
        let restored = browser_graph(&server, &csrf, UNDO, input.clone()).await;
        assert_graph_result(&restored, "undoMetadata", "MetadataRestored");
        assert_eq!(
            restored["data"]["undoMetadata"]["manifestation"]["metadata"]["title"],
            original["title"]
        );
        assert_eq!(
            restored["data"]["undoMetadata"]["operation"]["status"],
            "CONSUMED"
        );
        let after = browser_graph(&server, &csrf, TITLE_READ, json!({"id":id})).await;
        assert_eq!(
            restored["data"]["undoMetadata"]["manifestation"]["metadata"],
            after["data"]["manifestation"]["metadata"]
        );
        let resolved = browser_graph(
            &server,
            &csrf,
            RESOLVE,
            json!({"id":id,"operation":operation}),
        )
        .await;
        assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "RESTORED");
        let expiring=browser_graph(&server,&csrf,TITLE_SAVE,json!({"id":id,"baseline":after["data"]["manifestation"]["metadata"]["etag"],"patch":{"title":"Expires"}})).await;
        assert_graph_ok(&expiring);
        let expiring_id: uuid::Uuid =
            expiring["data"]["updateMetadata"]["operations"][0]["operationId"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
        sqlx::query!("UPDATE metadata_undo_operations SET expires_at = clock_timestamp() - interval '1 second' WHERE id = $1",expiring_id).execute(&pool).await.unwrap();
        let expired=browser_graph(&server,&csrf,UNDO,json!({"id":id,"baseline":expiring["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"],"operation":expiring_id})).await;
        assert_graph_result(&expired, "undoMetadata", "UndoUnavailable");
    }

    async fn trusted_editing_document(pool: &PgPool, document_id: &str, text: &str) {
        let fixture_name = format!("trusted-{}", uuid::Uuid::new_v4());
        let (user, server, csrf) = editing_browser(pool, &fixture_name).await;
        let app = db::app_pool_for(pool).await;
        let ing = db::ingestion_pool_for(pool).await;
        let (_, edition) = db::insert_work_and_manifestation(&ing, &fixture_name).await;
        let shelf = db::create_shelf(&app, user, "Trusted shelf").await;
        let id = id::Manifestation::encode(edition);
        if text.contains("unlockMetadataField(") {
            let locked = browser_graph(&server, &csrf, "mutation($id:ID!) { lockMetadataField(input:{id:$id,fieldName:\"description\",entityType:WORK}) { __typename } }", json!({"id":id})).await;
            assert_graph_result(&locked, "lockMetadataField", "MetadataFieldLockAdded");
        }
        let version = sqlx::query_scalar!("INSERT INTO metadata_versions (manifestation_id, source, field_name, new_value, value_hash, match_type, status, confidence_score) VALUES ($1, 'opf', 'description', to_jsonb('Trusted proposal'::text), decode('00000001', 'hex'), 'title', 'pending'::metadata_review_status, 0.5) RETURNING id", edition).fetch_one(&ing).await.unwrap();
        let current = browser_graph(&server, &csrf, TITLE_READ, json!({"id":id})).await;
        let mut baseline = current["data"]["manifestation"]["metadata"]["etag"].clone();
        let mut operation = serde_json::Value::Null;
        let mut request = json!(uuid::Uuid::new_v4());
        let mut patch = json!({"title":"Trusted title"});
        if text.contains("updateReadingState(") {
            let reading = browser_graph(&server, &csrf, READING_READ, json!({"id":id})).await;
            baseline = reading["data"]["manifestation"]["readingState"]["etag"].clone();
            patch = json!({"notes":"Trusted reading","status":"READING"});
        } else if text.contains("updateLibraryPreferences(") {
            patch = json!({"density":"COMPACT","hiddenColumns":[]});
        } else if text.contains("mutationsSaveAuthorsMutation") {
            patch = json!({"contributors":{"author":["Trusted author"]}});
        }
        if text.contains("undoMetadata(") || text.contains("metadataOperation(") {
            let saved = browser_graph(&server,&csrf,TITLE_SAVE,json!({"id":id,"baseline":baseline,"request":request,"patch":{"title":"Resolve or undo this"}})).await;
            assert_graph_ok(&saved);
            baseline = saved["data"]["updateMetadata"]["manifestation"]["metadata"]["etag"].clone();
            operation = saved["data"]["updateMetadata"]["operations"][0]["operationId"].clone();
            request = serde_json::Value::Null;
        }
        let shelf_id = id::Shelf::encode(shelf);
        let version_id = id::MetadataVersion::encode(version);
        let input = if text.contains("updateMetadata(") {
            json!({"id":id,"baseline":baseline,"requestId":request,"patch":patch})
        } else if text.contains("undoMetadata(") {
            json!({"id":id,"baseline":baseline,"operationId":operation})
        } else if text.contains("acceptMetadataVersion(") || text.contains("rejectMetadataVersion(")
        {
            json!({"id":id,"versionId":version_id})
        } else if text.contains("revertMetadataField(") {
            json!({"id":id,"fieldName":"description","versionId":version_id})
        } else if text.contains("lockMetadataField(") {
            json!({"id":id,"fieldName":"description","entityType":"WORK"})
        } else if text.contains("createShelf(") {
            json!({"name":"Trusted shelf change"})
        } else if text.contains("renameShelf(") {
            json!({"id":shelf_id,"name":"Trusted shelf change"})
        } else if text.contains("deleteShelf(") {
            json!({"id":shelf_id})
        } else if text.contains("addShelfItem(") {
            json!({"shelfId":shelf_id,"manifestationId":id})
        } else {
            serde_json::Value::Null
        };
        let response=server.post("/api/graphql").add_header("x-csrf-token",csrf).json(&json!({"id":document_id,"variables":{
            "id":id,"shelfId":shelf_id,"name":"Trusted shelf change","input":input,"connections":[],
            "baseline":baseline,"patch":patch,"requestId":request,"operationId":operation,
            "versionId":version_id,"fieldName":"description","entityType":"work","first":20,"count":20
        }})).await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_graph_ok(&body);
        for result in body["data"]
            .as_object()
            .into_iter()
            .flat_map(|data| data.values())
        {
            assert!(
                !matches!(
                    result["__typename"].as_str(),
                    Some(
                        "NotFound"
                            | "SystemShelfImmutable"
                            | "ShelfNameInvalid"
                            | "StaleBaseline"
                            | "MetadataInvalid"
                            | "FieldsLocked"
                            | "EditTooLarge"
                            | "UndoUnavailable"
                            | "UndoConflict"
                    )
                ),
                "{document_id}: {body}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_trusted_documents(pool: PgPool) {
        let documents: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../graphql-documents.json")).unwrap();
        assert!(!documents.is_empty());
        let fixture = reader_fixture(&pool).await;
        for id in documents.keys() {
            let envelope = serde_json::from_value(json!({"id":id})).unwrap();
            assert_eq!(
                document(&envelope, ExecutionPolicy::Production, &documents).unwrap(),
                documents[id]
            );
            if documents[id].contains("mutations") {
                trusted_editing_document(&pool, id, &documents[id]).await;
                continue;
            }
            let response = fixture
                .server
                .post("/api/graphql")
                .add_header("authorization", fixture.admin.clone())
                .json(&json!({"id": id, "variables": reader_variables(&documents[id], &fixture)}))
                .await;
            response.assert_status_ok();
            assert!(response.json::<serde_json::Value>().get("errors").is_none());
        }
        for request in [
            json!({"id":"unknown"}),
            json!({"query":"{ __typename }"}),
            json!({"id":documents.keys().next().unwrap(),"query":"{ __typename }"}),
        ] {
            let envelope = serde_json::from_value(request).unwrap();
            assert!(document(&envelope, ExecutionPolicy::Production, &documents).is_err());
        }
        for (policy, introspection_allowed) in [
            (ExecutionPolicy::Development, true),
            (ExecutionPolicy::Production, false),
        ] {
            let schema = schema_builder(policy)
                .data(test_support::test_state())
                .finish();
            let response = schema
                .execute("{ __schema { queryType { name } } __type(name: \"Viewer\") { name } }")
                .await;
            let data = serde_json::to_value(response.data).unwrap();
            if introspection_allowed {
                assert_eq!(data["__schema"]["queryType"]["name"], "Query");
                assert_eq!(data["__type"]["name"], "Viewer");
            } else {
                assert!(data["__schema"].is_null());
                assert!(data["__type"].is_null());
            }
            assert_eq!(schema.execute("{ __typename }").await.errors, []);
            assert_ne!(
                schema
                    .execute("query($show: Boolean!) { __typename @include(if: $show) }")
                    .await
                    .errors,
                []
            );
            assert_eq!(
                schema
                    .execute(format!(
                        "{{ {} }}",
                        (0..=complexity::LIMIT / 63_000)
                            .map(|i| format!("alias{i}: series(id: \"malformed\") {{ works(first: 50) {{ edges {{ node {{ manifestations(first: 20) {{ edges {{ node {{ ratings(first: 20) {{ edges {{ node {{ source }} }} }} }} }} }} }} }} }} }}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ))
                    .await
                    .errors[0]
                    .message,
                "Query is too complex."
            );
            let depth_response = schema_builder(policy)
                .limit_depth(1)
                .finish()
                .execute("{ viewer { displayName } }")
                .await;
            assert_eq!(
                depth_response.errors[0].message,
                "Query is nested too deep."
            );
        }
    }

    fn reader_variables(document: &str, fixture: &ReaderFixture) -> serde_json::Value {
        let id = if document.contains("readerShelfQuery") || document.contains("fetch__Shelf(") {
            id::Shelf::encode(fixture.shelf_a)
        } else if document.contains("readerSeriesQuery") || document.contains("fetch__Series(") {
            id::Series::encode(fixture.series)
        } else if document.contains("fetch__Work(") {
            id::Work::encode(fixture.visible_work)
        } else {
            id::Manifestation::encode(fixture.visible)
        };
        json!({
            "id": id.as_str(), "q": "Graph",
            "ids": [if document.contains("authorLabels(") { id::Author::encode(fixture.author) } else { id::Series::encode(fixture.series) }],
            "authorIds": [id::Author::encode(fixture.author).as_str()],
            "seriesIds": [id::Series::encode(fixture.series).as_str()]
        })
    }

    struct ReaderFixture {
        server: TestServer,
        admin: String,
        child: String,
        other: String,
        admin_id: uuid::Uuid,
        child_id: uuid::Uuid,
        visible_work: uuid::Uuid,
        visible: uuid::Uuid,
        sibling: uuid::Uuid,
        hidden_work: uuid::Uuid,
        hidden: uuid::Uuid,
        child_shelf: uuid::Uuid,
        other_shelf: uuid::Uuid,
        shelf_a: uuid::Uuid,
        shelf_b: uuid::Uuid,
        series: uuid::Uuid,
        author: uuid::Uuid,
        hidden_author: uuid::Uuid,
    }

    async fn reader_tags(pool: &PgPool, visible: uuid::Uuid, sibling: uuid::Uuid) {
        for name in ["alpha", "beta"] {
            let tag = sqlx::query_scalar!("INSERT INTO tags (name) VALUES ($1) RETURNING id", name)
                .fetch_one(pool)
                .await
                .unwrap();
            for book in [visible, sibling] {
                sqlx::query!(
                    "INSERT INTO manifestation_tags (manifestation_id, tag_id) VALUES ($1, $2)",
                    book,
                    tag
                )
                .execute(pool)
                .await
                .unwrap();
            }
        }
    }

    async fn reader_notes(
        pool: &PgPool,
        ingestion: &PgPool,
        visible: uuid::Uuid,
        admin_id: uuid::Uuid,
        child_id: uuid::Uuid,
        other_id: uuid::Uuid,
    ) {
        for (user, note) in [
            (admin_id, "Admin note"),
            (child_id, "Child note"),
            (other_id, "Other reader note"),
        ] {
            sqlx::query!(
                "INSERT INTO reading_state (user_id, manifestation_id, notes) VALUES ($1, $2, $3)",
                user,
                visible,
                note
            )
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query!("INSERT INTO metadata_versions (manifestation_id, source, field_name, new_value, value_hash, match_type, status, confidence_score) VALUES ($1, 'opf', 'description', to_jsonb('Unreviewed graph proposal'::text), decode('00000001', 'hex'), 'title', 'pending'::metadata_review_status, 0.5)", visible).execute(ingestion).await.unwrap();
    }

    async fn reader_fixture(pool: &PgPool) -> ReaderFixture {
        let (server, admin, admin_id) = server(pool).await;
        let app = db::app_pool_for(pool).await;
        let ingestion = db::ingestion_pool_for(pool).await;
        let (child_id, child) = db::create_child_user_and_basic_auth(&app, "graph-child").await;
        let (other_id, other) = db::create_adult_and_basic_auth(&app, "graph-other").await;
        let (visible_work, visible) =
            db::insert_work_and_manifestation(&ingestion, "graph-visible").await;
        let (_, sibling) = db::insert_work_and_manifestation(&ingestion, "graph-sibling").await;
        let (hidden_work, hidden) =
            db::insert_work_and_manifestation(&ingestion, "graph-hidden").await;
        sqlx::query!(
            "UPDATE manifestations SET work_id = $1 WHERE id = $2",
            visible_work,
            sibling
        )
        .execute(pool)
        .await
        .unwrap();
        for (work, title) in [
            (visible_work, "Visible Graph Book"),
            (hidden_work, "Hidden Graph Book"),
        ] {
            sqlx::query!(
                "UPDATE works SET title = $1, sort_title = lower($1) WHERE id = $2",
                title,
                work
            )
            .execute(pool)
            .await
            .unwrap();
        }
        for (book, pages, day) in [(visible, 10, 1), (sibling, 20, 2), (hidden, 20, 3)] {
            let created = format!("2026-01-{day:02}T00:00:00Z")
                .parse::<jiff::Timestamp>()
                .unwrap();
            sqlx::query!(
                "UPDATE manifestations SET pages = $1, created_at = $2 WHERE id = $3",
                pages,
                jiff_sqlx::Timestamp::from(created),
                book
            )
            .execute(pool)
            .await
            .unwrap();
        }
        let author =
            db::insert_contributor(pool, visible_work, "Visible Author", "author", 0).await;
        db::insert_contributor(pool, visible_work, "Second Author", "author", 1).await;
        let hidden_author =
            db::insert_contributor(pool, hidden_work, "Hidden Author", "author", 0).await;
        reader_tags(pool, visible, sibling).await;
        let child_shelf = db::create_shelf(&app, child_id, "Child graph shelf").await;
        db::add_to_shelf(&app, child_shelf, visible).await;
        let other_shelf = db::create_shelf(&app, other_id, "Other graph shelf").await;
        db::add_to_shelf(&app, other_shelf, hidden).await;
        let shelf_a = db::create_shelf(&app, admin_id, "Graph shelf A").await;
        let shelf_b = db::create_shelf(&app, admin_id, "Graph shelf B").await;
        for book in [visible, sibling] {
            db::add_to_shelf(&app, shelf_a, book).await;
        }
        db::add_to_shelf(&app, shelf_b, hidden).await;
        let series = sqlx::query_scalar!("INSERT INTO series (name, sort_name) VALUES ('Graph Series', 'graph series') RETURNING id").fetch_one(pool).await.unwrap();
        for (work, position) in [(visible_work, 2.0), (hidden_work, 1.0)] {
            sqlx::query!("INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)", series, work, position).execute(pool).await.unwrap();
        }
        reader_notes(pool, &ingestion, visible, admin_id, child_id, other_id).await;
        ReaderFixture {
            server,
            admin,
            child,
            other,
            admin_id,
            child_id,
            visible_work,
            visible,
            sibling,
            hidden_work,
            hidden,
            child_shelf,
            other_shelf,
            shelf_a,
            shelf_b,
            series,
            author,
            hidden_author,
        }
    }

    async fn reader_query(
        fixture: &ReaderFixture,
        auth: &str,
        query: &str,
        variables: serde_json::Value,
    ) -> serde_json::Value {
        let response = fixture
            .server
            .post("/api/graphql")
            .add_header("authorization", auth.to_owned())
            .json(&json!({"query": query, "variables": variables}))
            .await;
        response.assert_status_ok();
        response.json()
    }

    // Authorisation and private-data boundary for the concrete refetch roots.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_fetchable_boundary(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let app = db::app_pool_for(&pool).await;
        let (_, child) = db::create_child_user_and_basic_auth(&app, "fetch-hidden").await;
        let schema = schema(ExecutionPolicy::Development);
        for (field, id) in [
            (
                "fetch__Manifestation",
                id::Manifestation::encode(fixture.hidden),
            ),
            ("fetch__Shelf", id::Shelf::encode(fixture.other_shelf)),
            ("fetch__Series", id::Series::encode(fixture.series)),
        ] {
            let query = format!("query($id: ID!) {{ {field}(id: $id) {{ id }} }}");
            let variables = json!({"id": id.as_str()});
            let denied = schema
                .execute(
                    async_graphql::Request::new(&query)
                        .variables(async_graphql::Variables::from_json(variables.clone()))
                        .data(CurrentUser::without_scopes_for_test(fixture.admin_id)),
                )
                .await;
            assert_eq!(denied.errors.len(), 1, "{denied:?}");
            assert_eq!(
                denied.errors[0].extensions.as_ref().unwrap().get("code"),
                Some(&async_graphql::Value::from("forbidden"))
            );
            let hidden = reader_query(&fixture, &child, &query, variables).await;
            assert!(hidden.get("errors").is_none(), "{hidden}");
            assert!(hidden["data"][field].is_null(), "{hidden}");
        }
    }

    // Private-data boundary; REV-REQ-0009 and REV-REQ-0059 retain caller visibility and shelf ownership.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_reader_boundary(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let visible = id::Manifestation::encode(fixture.visible);
        let response = reader_query(
            &fixture,
            &fixture.child,
            "{ books(first: 50) { edges { node { id } } } }",
            json!({}),
        )
        .await;
        assert!(response.get("errors").is_none(), "{response}");
        assert_eq!(
            response["data"]["books"]["edges"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            response["data"]["books"]["edges"][0]["node"]["id"],
            visible.as_str()
        );
        let query = "query($id: ID!) { manifestation(id: $id) { id metadata { title etag } readingState { notes etag } work { id authors(first: 5) { edges { node { name } role position } } } } }";
        let response = reader_query(
            &fixture,
            &fixture.child,
            query,
            json!({"id": visible.as_str()}),
        )
        .await;
        assert!(response.get("errors").is_none(), "{response}");
        assert_eq!(
            response["data"]["manifestation"]["readingState"]["notes"],
            "Child note"
        );
        assert!(!response.to_string().contains("Other reader note"));
        assert_eq!(
            response["data"]["manifestation"]["work"]["authors"]["edges"][0]["node"]["name"],
            "Visible Author"
        );
        let other = reader_query(
            &fixture,
            &fixture.other,
            query,
            json!({"id": visible.as_str()}),
        )
        .await;
        assert!(other.get("errors").is_none(), "{other}");
        assert_eq!(
            other["data"]["manifestation"]["readingState"]["notes"],
            "Other reader note"
        );
        for invalid in [
            "not-a-type-id".to_owned(),
            fixture.visible.to_string(),
            id::Shelf::encode(fixture.visible).to_string(),
            id::Manifestation::encode(fixture.hidden).to_string(),
            id::Manifestation::encode(uuid::Uuid::new_v4()).to_string(),
        ] {
            let response = reader_query(
                &fixture,
                &fixture.child,
                "query($id: ID!) { manifestation(id: $id) { id } node(id: $id) { id } }",
                json!({"id": invalid}),
            )
            .await;
            assert!(response.get("errors").is_none(), "{response}");
            assert!(response["data"]["manifestation"].is_null());
            assert!(response["data"]["node"].is_null());
        }
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $shelf: ID!, $other: ID!, $caller: ID!) { work(id: $work) { id } shelf(id: $shelf) { id } other: node(id: $other) { id } caller: node(id: $caller) { id } }", json!({"work": id::Work::encode(fixture.hidden_work).as_str(), "shelf": id::Shelf::encode(fixture.other_shelf).as_str(), "other": id::User::encode(fixture.admin_id).as_str(), "caller": id::User::encode(fixture.child_id).as_str()})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert!(response["data"]["work"].is_null());
        assert!(response["data"]["shelf"].is_null());
        assert!(response["data"]["other"].is_null());
        assert_eq!(
            response["data"]["caller"]["id"],
            id::User::encode(fixture.child_id).as_str()
        );
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $series: ID!, $shelf: ID!) { work(id: $work) { manifestations(first: 2) { edges { node { id } } } } series(id: $series) { works(first: 50) { edges { node { id manifestations(first: 2) { edges { node { id } } } } } } } shelf(id: $shelf) { items(first: 50) { edges { node { id } } } } }", json!({"work": id::Work::encode(fixture.visible_work).as_str(), "series": id::Series::encode(fixture.series).as_str(), "shelf": id::Shelf::encode(fixture.child_shelf).as_str()})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert_eq!(
            response["data"]["work"]["manifestations"]["edges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            response["data"]["series"]["works"]["edges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            response["data"]["shelf"]["items"]["edges"][0]["node"]["id"],
            visible.as_str()
        );
        for hidden in [
            id::Manifestation::encode(fixture.hidden),
            id::Manifestation::encode(fixture.sibling),
            id::Work::encode(fixture.hidden_work),
        ] {
            assert!(!response.to_string().contains(hidden.as_str()));
        }
        let response = reader_query(&fixture, &fixture.child, "query($ids: [ID!]!) { authorLabels(ids: $ids) { entityId value } search(q: \"Graph\") { manifestation { id } } authorSuggestions(q: \"Hidden\") { value } shelves(first: 50) { edges { node { id name } } } }", json!({"ids": [id::Author::encode(fixture.author).as_str(), id::Author::encode(fixture.hidden_author).as_str()]})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert_eq!(
            response["data"]["authorLabels"].as_array().unwrap().len(),
            1
        );
        assert_eq!(response["data"]["search"].as_array().unwrap().len(), 1);
        assert_eq!(response["data"]["authorSuggestions"], json!([]));
        assert!(!response.to_string().contains("Other graph shelf"));
        let versions_query = "query($id: ID!) { manifestation(id: $id) { versions { pendingCount pending(first: 20) { edges { node { newValue } } } } } }";
        let adult_versions = reader_query(
            &fixture,
            &fixture.admin,
            versions_query,
            json!({"id": visible.as_str()}),
        )
        .await;
        assert!(adult_versions.get("errors").is_none(), "{adult_versions}");
        assert_eq!(
            adult_versions["data"]["manifestation"]["versions"]["pendingCount"],
            1
        );
        assert_eq!(
            adult_versions["data"]["manifestation"]["versions"]["pending"]["edges"][0]["node"]["newValue"],
            "Unreviewed graph proposal"
        );
        for (query, variables) in [
            (versions_query, json!({"id": visible.as_str()})),
            (
                "query($id: ID!) { node(id: $id) { ... on Manifestation { versions { pendingCount pending(first: 20) { edges { node { newValue } } } } } } }",
                json!({"id": visible.as_str()}),
            ),
            (
                "query($id: ID!) { work(id: $id) { manifestations(first: 2) { edges { node { versions { pendingCount pending(first: 20) { edges { node { newValue } } } } } } } } }",
                json!({"id": id::Work::encode(fixture.visible_work).as_str()}),
            ),
        ] {
            let response = reader_query(&fixture, &fixture.child, query, variables).await;
            assert!(response.get("errors").is_none(), "{response}");
            assert!(!response.to_string().contains("Unreviewed graph proposal"));
            assert!(response.to_string().contains("\"pendingCount\":0"));
            assert!(response.to_string().contains("\"edges\":[]"));
        }
    }

    async fn pending_cursor_identity(fixture: &ReaderFixture) {
        let query = "query($id: ID!, $after: String) { manifestation(id:$id) { versions { pending(first:1,after:$after) { edges { cursor node { id } } } } } }";
        let page = reader_query(
            fixture,
            &fixture.admin,
            query,
            json!({"id":id::Manifestation::encode(fixture.visible).as_str()}),
        )
        .await;
        assert!(page.get("errors").is_none(), "{page}");
        let cursor = page["data"]["manifestation"]["versions"]["pending"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        let refused = reader_query(
            fixture,
            &fixture.admin,
            query,
            json!({"id":id::Manifestation::encode(fixture.sibling).as_str(),"after":cursor}),
        )
        .await;
        assert_eq!(refused["errors"][0]["extensions"]["code"], "validation");
    }

    // REV-REQ-0003, REV-REQ-0057 and REV-REQ-0058 cover ranges, cursor identity and sort refusal.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_cursor_contract(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let query = "query($first: Int!, $after: String, $filter: BooksFilter, $sort: [SortLevel!]) { books(first: $first, after: $after, filter: $filter, sort: $sort) { edges { cursor node { id pages createdAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }";
        let first = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "sort": [{"column": "CREATED_AT", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-04T00:00:00Z"}})).await;
        assert!(first.get("errors").is_none(), "{first}");
        let cursor = first["data"]["books"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        assert_eq!(first["data"]["books"]["pageInfo"]["startCursor"], cursor);
        assert_eq!(first["data"]["books"]["pageInfo"]["endCursor"], cursor);
        let next = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "after": cursor, "sort": [{"column": "CREATED_AT", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T08:00:00+08:00", "createdAtLt": "2026-01-04T08:00:00+08:00"}})).await;
        assert!(next.get("errors").is_none(), "{next}");
        assert_eq!(
            next["data"]["books"]["edges"][0]["node"]["id"],
            id::Manifestation::encode(fixture.sibling).as_str()
        );
        assert_eq!(next["data"]["books"]["pageInfo"]["hasPreviousPage"], true);
        for filter in [
            json!({"pagesGte": 20, "pagesLte": 20}),
            json!({"pagesGte": 30, "pagesLte": 10}),
            json!({"createdAtGte": "2026-01-02T00:00:00Z", "createdAtLt": "2026-01-02T00:00:00Z"}),
            json!({"createdAtGte": "2026-01-03T00:00:00Z", "createdAtLt": "2026-01-01T00:00:00Z"}),
            json!({"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-02T00:00:00Z"}),
        ] {
            let response = reader_query(
                &fixture,
                &fixture.admin,
                query,
                json!({"first": 50, "filter": filter}),
            )
            .await;
            assert!(response.get("errors").is_none(), "{response}");
            let expected = if filter.get("pagesGte") == Some(&json!(20)) {
                2
            } else {
                usize::from(
                    filter.get("createdAtLt") == Some(&json!("2026-01-02T00:00:00Z"))
                        && filter.get("createdAtGte") == Some(&json!("2026-01-01T00:00:00Z")),
                )
            };
            assert_eq!(
                response["data"]["books"]["edges"].as_array().unwrap().len(),
                expected
            );
        }
        for variables in [
            json!({"first": 50, "after": cursor, "sort": [{"column": "TITLE", "direction": "ASC"}]}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "CREATED_AT", "direction": "DESC"}]}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "CREATED_AT", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:01Z", "createdAtLt": "2026-01-04T00:00:00Z"}}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "CREATED_AT", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-05T00:00:00Z"}}),
            json!({"first": 50, "sort": [{"column": "Title", "direction": "ASC"}]}),
            json!({"first": 50, "sort": [{"column": "TITLE", "direction": "ASC"}, {"column": "TITLE", "direction": "DESC"}]}),
            json!({"first": 50, "sort": [{"column": "TITLE", "direction": "ASC"}, {"column": "AUTHOR", "direction": "ASC"}, {"column": "PAGES", "direction": "ASC"}, {"column": "CREATED_AT", "direction": "ASC"}]}),
        ] {
            let response = reader_query(&fixture, &fixture.admin, query, variables).await;
            assert_eq!(
                response["errors"][0]["extensions"]["code"], "validation",
                "{response}"
            );
            assert!(response["data"]["books"].is_null());
        }
        let ordered = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "sort": [{"column": "TITLE", "direction": "ASC"}, {"column": "CREATED_AT", "direction": "ASC"}]})).await;
        assert!(ordered.get("errors").is_none(), "{ordered}");
        let ordered_cursor = ordered["data"]["books"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        let refused = reader_query(&fixture, &fixture.admin, query, json!({"first": 50, "after": ordered_cursor, "sort": [{"column": "CREATED_AT", "direction": "ASC"}, {"column": "TITLE", "direction": "ASC"}]})).await;
        assert_eq!(
            refused["errors"][0]["extensions"]["code"], "validation",
            "{refused}"
        );
        let refused = reader_query(
            &fixture,
            &fixture.admin,
            "query($after: String!) { shelves(first: 50, after: $after) { edges { node { id } } } }",
            json!({"after": cursor}),
        )
        .await;
        assert_eq!(
            refused["errors"][0]["extensions"]["code"], "validation",
            "{refused}"
        );
        let tagged = reader_query(
            &fixture,
            &fixture.admin,
            query,
            json!({"first": 1, "filter": {"tagAny": ["alpha", "beta"]}}),
        )
        .await;
        assert!(tagged.get("errors").is_none(), "{tagged}");
        let tagged_cursor = tagged["data"]["books"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        let reordered = reader_query(
            &fixture,
            &fixture.admin,
            query,
            json!({"first": 50, "after": tagged_cursor, "filter": {"tagAny": ["beta", "alpha"]}}),
        )
        .await;
        assert!(reordered.get("errors").is_none(), "{reordered}");
        assert_eq!(
            reordered["data"]["books"]["edges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        for (field, parent_a, parent_b) in [
            (
                "work",
                id::Work::encode(fixture.visible_work),
                id::Work::encode(fixture.hidden_work),
            ),
            (
                "shelf",
                id::Shelf::encode(fixture.shelf_a),
                id::Shelf::encode(fixture.shelf_b),
            ),
        ] {
            let relation = if field == "work" {
                "manifestations"
            } else {
                "items"
            };
            let nested_query = format!(
                "query($id: ID!, $after: String) {{ {field}(id: $id) {{ {relation}(first: 1, after: $after) {{ edges {{ cursor node {{ id }} }} }} }} }}"
            );
            let page = reader_query(
                &fixture,
                &fixture.admin,
                &nested_query,
                json!({"id": parent_a.as_str()}),
            )
            .await;
            assert!(page.get("errors").is_none(), "{page}");
            let nested_cursor = page["data"][field][relation]["edges"][0]["cursor"]
                .as_str()
                .unwrap();
            for response in [
                reader_query(
                    &fixture,
                    &fixture.admin,
                    &nested_query,
                    json!({"id": parent_b.as_str(), "after": nested_cursor}),
                )
                .await,
                reader_query(
                    &fixture,
                    &fixture.admin,
                    query,
                    json!({"after": nested_cursor}),
                )
                .await,
            ] {
                assert_eq!(
                    response["errors"][0]["extensions"]["code"], "validation",
                    "{response}"
                );
            }
        }
        pending_cursor_identity(&fixture).await;
        let shelf_page = reader_query(
            &fixture,
            &fixture.admin,
            "{ shelves(first: 1) { edges { cursor } } }",
            json!({}),
        )
        .await;
        assert!(shelf_page.get("errors").is_none(), "{shelf_page}");
        let shelf_cursor = shelf_page["data"]["shelves"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        let refused = reader_query(
            &fixture,
            &fixture.admin,
            query,
            json!({"after": shelf_cursor}),
        )
        .await;
        assert_eq!(
            refused["errors"][0]["extensions"]["code"], "validation",
            "{refused}"
        );
    }
}
