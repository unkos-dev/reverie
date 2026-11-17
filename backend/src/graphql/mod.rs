//! Authenticated GraphQL execution with build-generated document admission.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Extension, State};
use juniper::{
    EmptySubscription, FieldError, GraphQLError, ID, IntoFieldError, Object, RootNode, ScalarValue,
    ValidatedOperation, Value, Variables, graphql_object,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::state::AppState;

macro_rules! connection {
    ($connection:ident, $edge:ident, $node:ty $(, $extra:ident: $extra_ty:ty)* $(,)?) => {
        pub(super) struct $edge {
            cursor: String,
            node: $node,
            $($extra: $extra_ty,)*
        }

        impl $edge {
            pub(super) const fn new(cursor: String, node: $node $(, $extra: $extra_ty)*) -> Self {
                Self { cursor, node $(, $extra)* }
            }
        }

        #[juniper::graphql_object]
        #[graphql(scalar = juniper::DefaultScalarValue, context = super::RequestContext, description = "An edge in a connection.")]
        impl $edge {
            #[graphql(description = "The item at the end of the edge")]
            const fn node(&self) -> &$node {
                &self.node
            }
            $(
                fn $extra(&self) -> $extra_ty {
                    self.$extra.clone()
                }
            )*
            #[graphql(description = "A cursor for use in pagination")]
            fn cursor(&self) -> &str {
                &self.cursor
            }
        }

        pub(super) struct $connection {
            edges: Vec<$edge>,
            page_info: super::PageInfo,
        }

        impl $connection {
            pub(super) fn new(has_previous: bool, has_next: bool, edges: Vec<$edge>) -> Self {
                let page_info = super::PageInfo {
                    has_previous_page: has_previous,
                    has_next_page: has_next,
                    start_cursor: edges.first().map(|edge| edge.cursor.clone()),
                    end_cursor: edges.last().map(|edge| edge.cursor.clone()),
                };
                Self { edges, page_info }
            }
        }

        #[juniper::graphql_object]
        #[graphql(scalar = juniper::DefaultScalarValue, context = super::RequestContext)]
        impl $connection {
            #[graphql(description = "Information to aid in pagination.")]
            const fn page_info(&self) -> &super::PageInfo {
                &self.page_info
            }
            #[graphql(description = "A list of edges.")]
            fn edges(&self) -> &[$edge] {
                &self.edges
            }
            #[graphql(description = "A list of nodes.")]
            fn nodes(&self) -> Vec<&$node> {
                self.edges.iter().map(|edge| &edge.node).collect()
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

/// Caller identity, application state, session and loaders for one execution request.
pub struct RequestContext {
    user: CurrentUser,
    state: AppState,
    session: tower_sessions::Session,
    loaders: loaders::Loaders,
}

impl juniper::Context for RequestContext {}

impl RequestContext {
    fn new(user: CurrentUser, state: AppState, session: tower_sessions::Session) -> Self {
        Self {
            loaders: loaders::Loaders::new(user.clone(), state.clone()),
            user,
            state,
            session,
        }
    }

    fn require(&self, scope: Scope) -> GraphResult<&CurrentUser> {
        // THREAT: each root field enforces its declared scope independently of HTTP POST.
        self.user.require_scope(scope)?;
        Ok(&self.user)
    }
}

// THREAT: no `Display` impl, so `?` cannot route internal text through Juniper's `From<Display>`.
pub(super) struct GraphError(FieldError);

impl<S: ScalarValue> IntoFieldError<S> for GraphError {
    fn into_field_error(self) -> FieldError<S> {
        self.0.map_scalar_value()
    }
}

impl From<AppError> for GraphError {
    fn from(error: AppError) -> Self {
        safe_error(error)
    }
}

pub(super) type GraphResult<T> = Result<T, GraphError>;

#[derive(juniper::GraphQLObject)]
#[graphql(
    scalar = juniper::DefaultScalarValue, context = RequestContext,
    description = "Information about pagination in a connection"
)]
pub(super) struct PageInfo {
    #[graphql(description = "When paginating backwards, are there more items?")]
    has_previous_page: bool,
    #[graphql(description = "When paginating forwards, are there more items?")]
    has_next_page: bool,
    #[graphql(description = "When paginating backwards, the cursor to continue.")]
    start_cursor: Option<String>,
    #[graphql(description = "When paginating forwards, the cursor to continue.")]
    end_cursor: Option<String>,
}

/// Root queries, with scopes declared on each field.
pub struct Query;

/// Caller-owned identity, independent of existing browser account projections.
#[derive(juniper::GraphQLObject)]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext, impl = library::NodeValue)]
pub struct Viewer {
    id: ID,
    is_child: bool,
    display_name: String,
}

async fn viewer(context: &RequestContext) -> GraphResult<Viewer> {
    let user = &context.user;
    let mut tx = crate::db::acquire_with_rls(&context.state.pool, user.user_id)
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let display_name =
        sqlx::query_scalar!("SELECT display_name FROM users WHERE id = $1", user.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Viewer {
        display_name,
        id: id::encode("User", user.user_id),
        is_child: user.is_child(),
    })
}

#[graphql_object]
#[graphql(scalar = juniper::DefaultScalarValue, context = RequestContext)]
impl Query {
    async fn library_preferences(
        context: &RequestContext,
    ) -> GraphResult<mutations::LibraryPreferences> {
        let user = context.require(Scope::Read)?;
        Ok(
            crate::services::preferences::get_preferences(&context.state.pool, user)
                .await?
                .into(),
        )
    }

    async fn metadata_operation(
        context: &RequestContext,
        executor: &juniper::Executor<'_, '_, RequestContext>,
        id: ID,
        request_id: Option<String>,
        operation_id: Option<String>,
    ) -> GraphResult<mutations::MetadataOperation> {
        context.require(Scope::Write)?;
        mutations::metadata_operation(context, executor, &id, request_id, operation_id).await
    }

    async fn viewer(context: &RequestContext) -> GraphResult<Viewer> {
        context.require(Scope::Read)?;
        viewer(context).await
    }

    async fn node(context: &RequestContext, id: ID) -> GraphResult<Option<library::NodeValue>> {
        let user = context.require(Scope::Read)?;
        if let Some(user_id) = id::decode("User", &id) {
            if user_id == user.user_id {
                return viewer(context).await.map(|viewer| Some(viewer.into()));
            }
            return Ok(None);
        }
        if id::decode("Work", &id).is_some() {
            return Ok(library::work(context, &id).await?.map(Into::into));
        }
        if id::decode("Manifestation", &id).is_some() {
            return Ok(library::manifestation(context, &id).await?.map(Into::into));
        }
        if id::decode("Shelf", &id).is_some() {
            return Ok(library::shelf(context, &id).await?.map(Into::into));
        }
        if id::decode("Series", &id).is_some() {
            return Ok(library::series(context, &id).await?.map(Into::into));
        }
        Ok(None)
    }

    async fn work(context: &RequestContext, id: ID) -> GraphResult<Option<library::Work>> {
        context.require(Scope::Read)?;
        library::work(context, &id).await
    }

    async fn manifestation(
        context: &RequestContext,
        id: ID,
    ) -> GraphResult<Option<library::Manifestation>> {
        context.require(Scope::Read)?;
        library::manifestation(context, &id).await
    }

    async fn shelf(context: &RequestContext, id: ID) -> GraphResult<Option<library::Shelf>> {
        context.require(Scope::Read)?;
        library::shelf(context, &id).await
    }

    async fn series(context: &RequestContext, id: ID) -> GraphResult<Option<library::Series>> {
        context.require(Scope::Read)?;
        library::series(context, &id).await
    }

    async fn books(
        context: &RequestContext,
        first: i32,
        after: Option<String>,
        filter: Option<library::BooksFilter>,
        #[graphql(description = "Omitted or null sort uses the installation default.")]
        sort: Option<Vec<library::SortLevel>>,
    ) -> GraphResult<library::ManifestationConnection> {
        context.require(Scope::Read)?;
        library::books(context, first, after, filter, sort).await
    }

    async fn shelves(
        context: &RequestContext,
        first: i32,
        after: Option<String>,
    ) -> GraphResult<library::ShelfConnection> {
        context.require(Scope::Read)?;
        library::shelves(context, first, after).await
    }

    async fn search(context: &RequestContext, q: String) -> GraphResult<Vec<library::SearchHit>> {
        context.require(Scope::Read)?;
        library::search(context, &q).await
    }

    async fn author_labels(
        context: &RequestContext,
        ids: Vec<ID>,
    ) -> GraphResult<Vec<library::AuthorSuggestion>> {
        context.require(Scope::Read)?;
        library::author_labels(context, &ids).await
    }

    async fn series_labels(
        context: &RequestContext,
        ids: Vec<ID>,
    ) -> GraphResult<Vec<library::SeriesSuggestion>> {
        context.require(Scope::Read)?;
        library::series_labels(context, &ids).await
    }

    async fn author_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::AuthorSuggestion>> {
        context.require(Scope::Read)?;
        library::author_suggestions(context, &q, limit).await
    }

    async fn series_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::SeriesSuggestion>> {
        context.require(Scope::Read)?;
        library::series_suggestions(context, &q, limit).await
    }

    async fn genre_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::GenreSuggestion>> {
        context.require(Scope::Read)?;
        library::genre_suggestions(context, &q, limit).await
    }

    async fn mood_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::MoodSuggestion>> {
        context.require(Scope::Read)?;
        library::mood_suggestions(context, &q, limit).await
    }

    async fn tag_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::TagSuggestion>> {
        context.require(Scope::Read)?;
        library::tag_suggestions(context, &q, limit).await
    }

    async fn publisher_suggestions(
        context: &RequestContext,
        q: String,
        limit: Option<i32>,
    ) -> GraphResult<Vec<library::PublisherSuggestion>> {
        context.require(Scope::Read)?;
        library::publisher_suggestions(context, &q, limit).await
    }
}

type Root = RootNode<Query, mutations::Mutation, EmptySubscription<RequestContext>>;

/// Schema for the authenticated browser API, with its admission limits.
pub struct AppSchema {
    root: Root,
    limits: complexity::Limits,
}

impl AppSchema {
    /// The schema in GraphQL schema definition language.
    #[must_use]
    pub fn sdl(&self) -> String {
        self.root.as_sdl()
    }
}

/// Construct a schema with finite execution limits and an explicit admission policy.
#[must_use]
pub fn schema(policy: ExecutionPolicy) -> AppSchema {
    schema_with_limits(
        policy,
        complexity::Limits {
            cost: complexity::LIMIT,
            depth: complexity::DEPTH,
        },
    )
}

fn schema_with_limits(policy: ExecutionPolicy, limits: complexity::Limits) -> AppSchema {
    let root = Root::new(Query, mutations::Mutation, EmptySubscription::new());
    AppSchema {
        root: if policy == ExecutionPolicy::Production {
            root.disable_introspection()
        } else {
            root
        },
        limits,
    }
}

fn extensions(code: &str, extra: Vec<(&str, Value)>) -> Value {
    let mut object = Object::with_capacity(extra.len() + 1);
    object.add_field("code", Value::scalar(code.to_owned()));
    for (name, value) in extra {
        object.add_field(name, value);
    }
    Value::object(object)
}

fn safe_error(error: AppError) -> GraphError {
    safe_error_with(error, Vec::new())
}

fn safe_error_with(error: AppError, mut extra: Vec<(&str, Value)>) -> GraphError {
    // THREAT: internal Display text can contain credentials and database diagnostics.
    let (message, code) = match error {
        AppError::FieldValidation { detail, errors } => {
            let fields = errors
                .into_iter()
                .map(|(name, message)| (name, Value::scalar(message)))
                .collect();
            extra.push(("errors", Value::object(fields)));
            (detail, "validation")
        }
        AppError::Validation(detail) => (detail, "validation"),
        AppError::Forbidden => ("Forbidden".to_owned(), "forbidden"),
        AppError::NotFound => ("Not found".to_owned(), "not-found"),
        AppError::IfMatchRequired => ("A baseline is required.".to_owned(), "if-match-required"),
        AppError::IfMatchMismatch => ("The baseline is stale.".to_owned(), "if-match-mismatch"),
        AppError::MalformedHeader(detail) => (detail, "malformed-header"),
        AppError::UndoUnavailable => (
            "Metadata Undo is unavailable.".to_owned(),
            "undo-unavailable",
        ),
        AppError::UndoConflict => ("Metadata Undo state conflicts.".to_owned(), "undo-conflict"),
        AppError::SystemShelfImmutable => (
            "System shelf cannot be modified.".to_owned(),
            "system-shelf-immutable",
        ),
        AppError::FieldLocked(fields) => {
            extra.push((
                "lockedFields",
                Value::list(fields.into_iter().map(Value::scalar).collect()),
            ));
            ("Metadata fields are locked.".to_owned(), "field-locked")
        }
        error => {
            tracing::error!(error = ?error, "GraphQL execution failed");
            ("An internal error occurred.".to_owned(), "internal")
        }
    };
    GraphError(FieldError::new(message, extensions(code, extra)))
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

fn response(data: serde_json::Value, mut errors: Vec<serde_json::Value>) -> serde_json::Value {
    for error in &mut errors {
        if let Some(error) = error.as_object_mut() {
            error
                .entry("extensions")
                .or_insert_with(|| serde_json::json!({"code": "validation"}));
        }
    }
    let mut body = serde_json::Map::new();
    body.insert("data".to_owned(), data);
    if !errors.is_empty() {
        body.insert("errors".to_owned(), serde_json::Value::Array(errors));
    }
    serde_json::Value::Object(body)
}

fn refusal(error: &GraphQLError) -> Result<serde_json::Value, AppError> {
    let errors = match serde_json::to_value(error).map_err(|e| AppError::Internal(e.into()))? {
        serde_json::Value::Array(errors) => errors,
        error => vec![error],
    };
    Ok(response(serde_json::Value::Null, errors))
}

// Juniper 0.18 refuses a variable's input object when a non-null field with a default is absent.
fn with_input_defaults(
    schema: &juniper::SchemaType<juniper::DefaultScalarValue>,
    operation: Option<&juniper::Operation<'_, juniper::DefaultScalarValue>>,
    variables: &Variables,
) -> Variables {
    let mut variables = variables.clone();
    for (name, definition) in operation
        .and_then(|operation| operation.variables_definition.as_ref())
        .iter()
        .flat_map(|definitions| definitions.item.items.iter())
    {
        if let Some(value) = variables.get_mut(name.item) {
            fill_input_defaults(schema, definition.var_type.item.innermost_name(), value);
        }
    }
    variables
}

fn fill_input_defaults(
    schema: &juniper::SchemaType<juniper::DefaultScalarValue>,
    type_name: &str,
    value: &mut juniper::InputValue,
) {
    match value {
        juniper::InputValue::List(items) => {
            for item in items {
                fill_input_defaults(schema, type_name, &mut item.item);
            }
        }
        juniper::InputValue::Object(fields) => {
            let Some(juniper::meta::MetaType::InputObject(meta)) =
                schema.concrete_type_by_name(type_name)
            else {
                return;
            };
            for input in &meta.input_fields {
                if let Some((_, present)) = fields
                    .iter_mut()
                    .find(|(name, _)| name.item == input.name.as_str())
                {
                    fill_input_defaults(schema, input.arg_type.innermost_name(), &mut present.item);
                } else if let Some(default) = &input.default_value {
                    fields.push((
                        juniper::Spanning::unlocated(input.name.to_string()),
                        juniper::Spanning::unlocated(default.clone()),
                    ));
                }
            }
        }
        _ => {}
    }
}

async fn run(
    schema: &AppSchema,
    text: &str,
    operation_name: Option<&str>,
    variables: &Variables,
    context: &RequestContext,
) -> Result<serde_json::Value, AppError> {
    let operation = match ValidatedOperation::new(text, operation_name, &schema.root) {
        Ok(operation) => operation,
        Err(error) => return refusal(&error),
    };
    let document = match juniper::parser::parse_document_source(text, &schema.root.schema) {
        Ok(document) => document,
        Err(error) => return refusal(&GraphQLError::ParseError(error)),
    };
    let variables = &with_input_defaults(
        &schema.root.schema,
        complexity::selected_operation(&document, operation_name),
        variables,
    );
    if let Err(messages) = complexity::check(
        &schema.root.schema,
        &document,
        operation_name,
        variables,
        schema.limits,
    ) {
        return Ok(response(
            serde_json::Value::Null,
            messages
                .into_iter()
                .map(|message| serde_json::json!({ "message": message }))
                .collect(),
        ));
    }
    match operation.execute(variables, context).await {
        Ok((data, errors)) => Ok(response(
            serde_json::to_value(&data).map_err(|e| AppError::Internal(e.into()))?,
            errors
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::Internal(e.into()))?,
        )),
        Err(error) => refusal(&error),
    }
}

#[derive(Clone)]
struct Service {
    schema: Arc<AppSchema>,
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
    let variables: Variables =
        serde_json::from_value(serde_json::Value::Object(envelope.variables))
            .map_err(|_| AppError::Validation("Invalid variables.".into()))?;
    let context = RequestContext::new(user, state, session);
    let body = run(
        &service.schema,
        &text,
        envelope.operation_name.as_deref(),
        &variables,
        &context,
    )
    .await?;
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/graphql-response+json",
            ),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        axum::Json(body),
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
            schema: Arc::new(schema(policy)),
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
    use base64ct::Encoding;
    use serde_json::json;
    use sqlx::PgPool;
    use tower_sessions::{MemoryStore, Session};

    async fn execute_as(
        schema: &AppSchema,
        user: CurrentUser,
        query: &str,
        variables: serde_json::Value,
    ) -> serde_json::Value {
        let session = Session::new(None, Arc::new(MemoryStore::default()), None);
        let context = RequestContext::new(user, test_support::test_state(), session);
        let variables = serde_json::from_value(variables).unwrap();
        run(schema, query, None, &variables, &context)
            .await
            .unwrap()
    }

    fn extensions_of(error: &GraphError) -> serde_json::Value {
        serde_json::to_value(error.0.extensions()).unwrap()
    }

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
            base64ct::Base64::encode_string(format!("User:{user_id}").as_bytes())
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
        assert_eq!(error.0.message(), "An internal error occurred.");
        assert!(!format!("{:?}", error.0).contains("private-database-credential"));
        let denied = safe_error(AppError::Forbidden);
        assert_eq!(extensions_of(&denied)["code"], "forbidden");
        let fields = safe_error(AppError::FieldValidation {
            detail: "Invalid input".into(),
            errors: BTreeMap::from([("name".into(), "Required".into())]),
        });
        assert_eq!(fields.0.message(), "Invalid input");
        assert_eq!(
            extensions_of(&fields)["errors"],
            json!({"name": "Required"})
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
        let pending = id::encode("MetadataVersion", pending).to_string();
        let id = id::encode("Manifestation", fixture.visible).to_string();
        let baseline=reader_query(&fixture,&fixture.admin,TITLE_READ,json!({"id":id})).await["data"]["manifestation"]["metadata"]["etag"].clone();
        let mutations = mutation_shapes(&fixture, &pending, &baseline);
        let inventory = execute_as(
            &schema(ExecutionPolicy::Development),
            CurrentUser::without_scopes_for_test(fixture.admin_id),
            "{ __type(name:\"Mutation\") { fields { name } } }",
            json!({}),
        )
        .await;
        assert_graph_ok(&inventory);
        let roots: std::collections::BTreeSet<_> = inventory["data"]["__type"]["fields"]
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
                let code = &admitted["errors"][0]["extensions"]["code"];
                assert_ne!(code, "forbidden", "{name}: {admitted}");
                assert_ne!(code, "validation", "{name}: {admitted}");
                if *name == "createShelf" {
                    assert_graph_ok(&admitted);
                    assert_eq!(admitted["data"][name]["name"], "Scoped shelf");
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
        let query = "query($id:ID!) { node(id:$id) { id } }";
        let denied = execute_as(
            &schema(ExecutionPolicy::Development),
            CurrentUser::without_scopes_for_test(fixture.admin_id),
            query,
            json!({"id":id::encode("Work",fixture.visible_work)}),
        )
        .await;
        assert_eq!(denied["errors"].as_array().unwrap().len(), 1);
        assert_graph_code(&denied, "forbidden");
        let hidden = reader_query(
            fixture,
            &fixture.child,
            query,
            json!({"id":id::encode("Work",fixture.hidden_work)}),
        )
        .await;
        assert_graph_ok(&hidden);
        assert!(hidden["data"]["node"].is_null());
    }

    async fn preference_scope_boundary(fixture: &ReaderFixture, read_credential: &str) {
        let preference_read = "{ libraryPreferences { hiddenColumns density view sortStack defaults { hiddenColumns density view sortStack } } }";
        let denied = execute_as(
            &schema(ExecutionPolicy::Development),
            CurrentUser::without_scopes_for_test(fixture.admin_id),
            preference_read,
            json!({}),
        )
        .await;
        assert_eq!(denied["errors"].as_array().unwrap().len(), 1);
        assert_graph_code(&denied, "forbidden");
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
        let missing = id::encode("Manifestation", uuid::Uuid::new_v4());
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
        let id = id::encode("Manifestation", fixture.visible).to_string();
        let shelf = id::encode("Shelf", fixture.shelf_a).to_string();
        let delete_shelf = id::encode("Shelf", fixture.shelf_b).to_string();
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
                    "updateMetadata(id:\"{id}\",baseline:{tag},patch:{{title:\"Scoped\"}}) {{ metadata {{ title etag }} }}"
                ),
            ),
            (
                "updateReadingState",
                format!("updateReadingState(id:\"{id}\",patch:{{notes:\"Scoped\"}}) {{ etag }}"),
            ),
            (
                "createShelf",
                "createShelf(name:\"Scoped shelf\") { id name }".to_owned(),
            ),
            (
                "renameShelf",
                format!("renameShelf(id:\"{shelf}\",name:\"Scoped rename\") {{ id name }}"),
            ),
            (
                "deleteShelf",
                format!("deleteShelf(id:\"{delete_shelf}\") {{ shelfId }}"),
            ),
            (
                "addShelfItem",
                format!(
                    "addShelfItem(shelfId:\"{shelf}\",manifestationId:\"{id}\") {{ shelfId manifestationId }}"
                ),
            ),
            (
                "removeShelfItem",
                format!(
                    "removeShelfItem(shelfId:\"{shelf}\",manifestationId:\"{id}\") {{ shelfId manifestationId }}"
                ),
            ),
            (
                "acceptMetadataVersion",
                format!(
                    "acceptMetadataVersion(id:\"{id}\",versionId:\"{pending}\") {{ manifestationId }}"
                ),
            ),
            (
                "rejectMetadataVersion",
                format!(
                    "rejectMetadataVersion(id:\"{id}\",versionId:\"{pending}\") {{ manifestationId }}"
                ),
            ),
            (
                "revertMetadataField",
                format!(
                    "revertMetadataField(id:\"{id}\",fieldName:\"subtitle\") {{ manifestationId }}"
                ),
            ),
            (
                "lockMetadataField",
                format!(
                    "lockMetadataField(id:\"{id}\",fieldName:\"subtitle\",entityType:\"work\") {{ manifestationId }}"
                ),
            ),
            (
                "unlockMetadataField",
                format!(
                    "unlockMetadataField(id:\"{id}\",fieldName:\"subtitle\",entityType:\"work\") {{ manifestationId }}"
                ),
            ),
            (
                "undoMetadata",
                format!(
                    "undoMetadata(id:\"{id}\",baseline:{tag},operationId:\"{}\") {{ outcome }}",
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
            assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "saved");
            let restored = browser_graph(&browser, &csrf, UNDO, json!({"id":id,"baseline":saved["data"]["updateMetadata"]["metadata"]["etag"],"operation":operation})).await;
            assert_graph_ok(&restored);
            assert_eq!(restored["data"]["undoMetadata"]["outcome"], "restored");
        }
    }
    async fn mutation_child_controls(pool: &PgPool, fixture: &ReaderFixture, id: &str) {
        let shelf = id::encode("Shelf", fixture.shelf_a).to_string();
        let own = reader_query(fixture, &fixture.child, READING_READ, json!({"id":id})).await;
        let child_saved=reader_query(fixture,&fixture.child,READING_SAVE,json!({"id":id,"baseline":own["data"]["manifestation"]["readingState"]["etag"],"patch":{"notes":"Child's change"}})).await;
        assert_graph_ok(&child_saved);
        let adult = reader_query(fixture, &fixture.admin, READING_READ, json!({"id":id})).await;
        assert_eq!(
            adult["data"]["manifestation"]["readingState"]["notes"],
            "Admin note"
        );
        let hidden = id::encode("Manifestation", fixture.hidden);
        assert_graph_code(&reader_query(fixture,&fixture.child,READING_SAVE,json!({"id":hidden,"baseline":own["data"]["manifestation"]["readingState"]["etag"],"patch":{"notes":"Hidden"}})).await,"not-found");
        let membership = "mutation($shelf:ID!,$id:ID!) { addShelfItem(shelfId:$shelf,manifestationId:$id) { shelfId manifestationId } }";
        assert_graph_ok(
            &reader_query(
                fixture,
                &fixture.child,
                membership,
                json!({"shelf":id::encode("Shelf",fixture.child_shelf),"id":id}),
            )
            .await,
        );
        assert_graph_code(
            &reader_query(
                fixture,
                &fixture.child,
                membership,
                json!({"shelf":id::encode("Shelf",fixture.child_shelf),"id":hidden}),
            )
            .await,
            "not-found",
        );
        assert_graph_code(
            &reader_query(
                fixture,
                &fixture.admin,
                membership,
                json!({"shelf":id::encode("Shelf",fixture.other_shelf),"id":id}),
            )
            .await,
            "not-found",
        );
        sqlx::query!(
            "UPDATE shelves SET is_system=TRUE WHERE id=$1",
            fixture.shelf_a
        )
        .execute(pool)
        .await
        .unwrap();
        for query in [
            format!("mutation {{ renameShelf(id:\"{shelf}\",name:\"Refused\") {{ id }} }}"),
            format!("mutation {{ deleteShelf(id:\"{shelf}\") {{ shelfId }} }}"),
        ] {
            assert_graph_code(
                &reader_query(fixture, &fixture.admin, &query, json!({})).await,
                "system-shelf-immutable",
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
    const TITLE_READ: &str =
        "query($id: ID!) { manifestation(id: $id) { metadata { title etag } } }";
    const TITLE_SAVE: &str = "mutation($id: ID!, $baseline: String, $request: String, $patch: MetadataPatch!) { updateMetadata(id: $id, baseline: $baseline, requestId: $request, patch: $patch) { requestId operations { operationId fieldName expiresAt status } metadata { title etag } } }";
    const READING_READ: &str =
        "query($id: ID!) { manifestation(id: $id) { readingState { notes rating status etag } } }";
    const READING_SAVE: &str = "mutation($id: ID!, $baseline: String, $patch: ReadingPatch!) { updateReadingState(id: $id, baseline: $baseline, patch: $patch) { notes rating status etag } }";
    const RESOLVE: &str = "query($id: ID!, $request: String, $operation: String) { metadataOperation(id: $id, requestId: $request, operationId: $operation) { outcome operations { operationId fieldName expiresAt status } metadata { title etag } } }";
    const UNDO: &str = "mutation($id: ID!, $baseline: String, $operation: String!) { undoMetadata(id: $id, baseline: $baseline, operationId: $operation) { outcome operations { operationId fieldName expiresAt status } metadata { title etag } } }";

    // REV-REQ-0018/0020 GraphQL preconditions; data-integrity boundary for selected revisions and explicit resend.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_edit_baselines(pool: PgPool) {
        let (_, server, csrf) = editing_browser(&pool, "graph-edits").await;
        let ing = db::ingestion_pool_for(&pool).await;
        let (_, edition) = db::insert_work_and_manifestation(&ing, "graph-edits").await;
        let id = id::encode("Manifestation", edition);
        let vars = json!({"id":id});
        let initial = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_graph_ok(&initial);
        let original = initial["data"]["manifestation"]["metadata"].clone();
        assert_eq!(original.as_object().unwrap().len(), 2);
        let request = uuid::Uuid::new_v4();
        let attempt = json!({"id":id,"baseline":original["etag"],"request":request,"patch":{"title":"Edited title"}});
        for (baseline, code) in [
            (serde_json::Value::Null, "if-match-required"),
            (json!("weak"), "malformed-header"),
            (json!("\"stale\""), "if-match-mismatch"),
        ] {
            let mut rejected = attempt.clone();
            rejected["baseline"] = baseline;
            let result = browser_graph(&server, &csrf, TITLE_SAVE, rejected).await;
            assert_graph_code(&result, code);
            if code == "if-match-mismatch" {
                assert_eq!(
                    result["errors"][0]["extensions"]["currentEtag"],
                    original["etag"]
                );
            }
        }
        let mut omitted = attempt.clone();
        omitted.as_object_mut().unwrap().remove("baseline");
        assert_graph_code(
            &browser_graph(&server, &csrf, TITLE_SAVE, omitted).await,
            "if-match-required",
        );
        let unchanged = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_eq!(unchanged["data"]["manifestation"]["metadata"], original);
        let saved = browser_graph(&server, &csrf, TITLE_SAVE, attempt.clone()).await;
        assert_graph_ok(&saved);
        let ack = &saved["data"]["updateMetadata"]["metadata"];
        assert_eq!(ack["title"], "Edited title");
        assert_ne!(ack["etag"], original["etag"]);
        let matched = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        assert_eq!(*ack, matched["data"]["manifestation"]["metadata"]);
        assert_graph_code(
            &browser_graph(&server, &csrf, TITLE_SAVE, attempt).await,
            "if-match-mismatch",
        );
        let resolved =
            browser_graph(&server, &csrf, RESOLVE, json!({"id":id,"request":request})).await;
        assert_graph_ok(&resolved);
        assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "saved");
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
        assert_graph_code(&reused, "validation");
        let unknown = browser_graph(
            &server,
            &csrf,
            RESOLVE,
            json!({"id":id,"request":uuid::Uuid::new_v4()}),
        )
        .await;
        assert_eq!(unknown["data"]["metadataOperation"]["outcome"], "unknown");
        let sparse = "mutation($id:ID!, $baseline:String, $patch:MetadataPatch!) { updateMetadata(id:$id,baseline:$baseline,patch:$patch) { metadata { title subtitle etag } } }";
        let subtitle = browser_graph(
            &server,
            &csrf,
            sparse,
            json!({"id":id,"baseline":ack["etag"],"patch":{"subtitle":"Keep"}}),
        )
        .await;
        assert_graph_ok(&subtitle);
        let retained = browser_graph(&server,&csrf,sparse,json!({"id":id,"baseline":subtitle["data"]["updateMetadata"]["metadata"]["etag"],"patch":{"title":"Still edited"}})).await;
        assert_graph_ok(&retained);
        assert_eq!(
            retained["data"]["updateMetadata"]["metadata"]["subtitle"],
            "Keep"
        );
        let cleared = browser_graph(&server,&csrf,sparse,json!({"id":id,"baseline":retained["data"]["updateMetadata"]["metadata"]["etag"],"patch":{"subtitle":null}})).await;
        assert_graph_ok(&cleared);
        assert!(cleared["data"]["updateMetadata"]["metadata"]["subtitle"].is_null());
        let shared = cleared["data"]["updateMetadata"]["metadata"]["etag"].clone();
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
        assert_eq!(
            [&left, &right]
                .into_iter()
                .filter(|v| v.get("errors").is_none())
                .count(),
            1
        );
        assert_graph_code(
            if left.get("errors").is_some() {
                &left
            } else {
                &right
            },
            "if-match-mismatch",
        );
        let now = browser_graph(&server, &csrf, TITLE_READ, vars.clone()).await;
        let names: Vec<_> = (0..22).map(|n| format!("Author {n:02}")).collect();
        let author_save = "mutation($id:ID!, $baseline:String, $patch:MetadataPatch!) { updateMetadata(id:$id,baseline:$baseline,patch:$patch) { metadata { etag authors(first:20) { edges { cursor node { name } } pageInfo { hasNextPage hasPreviousPage endCursor } } } } }";
        let authors = browser_graph(&server,&csrf,author_save,json!({"id":id,"baseline":now["data"]["manifestation"]["metadata"]["etag"],"patch":{"contributors":{"author":names}}})).await;
        assert_graph_ok(&authors);
        let first = &authors["data"]["updateMetadata"]["metadata"];
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
            revised["data"]["updateMetadata"]["metadata"]["etag"],
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
        let id = id::encode("Manifestation", edition);
        let other_id = id::encode("Manifestation", other_edition);
        let read = browser_graph(&server, &csrf, TITLE_READ, json!({"id":id})).await;
        let original = &read["data"]["manifestation"]["metadata"];
        let saved = browser_graph(&server,&csrf,TITLE_SAVE,json!({"id":id,"baseline":original["etag"],"request":uuid::Uuid::new_v4(),"patch":{"title":"Undo this"}})).await;
        assert_graph_ok(&saved);
        let result = &saved["data"]["updateMetadata"];
        let operation = &result["operations"][0]["operationId"];
        assert_eq!(result["operations"][0]["status"], "eligible");
        let input = json!({"id":id,"baseline":result["metadata"]["etag"],"operation":operation});
        for (baseline, code) in [
            (serde_json::Value::Null, "if-match-required"),
            (json!("*"), "malformed-header"),
            (original["etag"].clone(), "if-match-mismatch"),
        ] {
            let mut bad = input.clone();
            bad["baseline"] = baseline;
            assert_graph_code(&browser_graph(&server, &csrf, UNDO, bad).await, code);
        }
        let mut omitted = input.clone();
        omitted.as_object_mut().unwrap().remove("baseline");
        assert_graph_code(
            &browser_graph(&server, &csrf, UNDO, omitted).await,
            "if-match-required",
        );
        let foreign_result = browser_graph(&foreign, &foreign_csrf, UNDO, input.clone()).await;
        assert_graph_code(&foreign_result, "undo-unavailable");
        let mut second = db::server_with_real_pools(&app, &ing);
        second.save_cookies();
        let second_csrf = test_support::mail::login(
            &second,
            "graph-undo@example.test",
            test_support::mail::PASSWORD,
        )
        .await;
        assert_graph_code(
            &browser_graph(&second, &second_csrf, UNDO, input.clone()).await,
            "undo-unavailable",
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
            "unknown"
        );
        let other = browser_graph(&server, &csrf, TITLE_READ, json!({"id":other_id})).await;
        assert_graph_code(&browser_graph(&server,&csrf,UNDO,json!({"id":other_id,"baseline":other["data"]["manifestation"]["metadata"]["etag"],"operation":operation})).await,"undo-unavailable");
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
        let lock = "mutation($id:ID!) { lockMetadataField(id:$id,fieldName:\"title\",entityType:\"work\") { manifestationId } }";
        let unlock = "mutation($id:ID!) { unlockMetadataField(id:$id,fieldName:\"title\",entityType:\"work\") { manifestationId } }";
        assert_graph_ok(&browser_graph(&server, &csrf, lock, json!({"id":id})).await);
        let locked = browser_graph(&server, &csrf, UNDO, input.clone()).await;
        assert_graph_code(&locked, "field-locked");
        assert_eq!(
            locked["errors"][0]["extensions"]["lockedFields"],
            json!(["title"])
        );
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
            "locked"
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
        assert_graph_ok(&browser_graph(&server, &csrf, unlock, json!({"id":id})).await);
        let restored = browser_graph(&server, &csrf, UNDO, input.clone()).await;
        assert_graph_ok(&restored);
        assert_eq!(restored["data"]["undoMetadata"]["outcome"], "restored");
        assert_eq!(
            restored["data"]["undoMetadata"]["metadata"]["title"],
            original["title"]
        );
        assert_eq!(
            restored["data"]["undoMetadata"]["operations"][0]["status"],
            "consumed"
        );
        let after = browser_graph(&server, &csrf, TITLE_READ, json!({"id":id})).await;
        assert_eq!(
            restored["data"]["undoMetadata"]["metadata"],
            after["data"]["manifestation"]["metadata"]
        );
        let resolved = browser_graph(
            &server,
            &csrf,
            RESOLVE,
            json!({"id":id,"operation":operation}),
        )
        .await;
        assert_eq!(resolved["data"]["metadataOperation"]["outcome"], "restored");
        let expiring=browser_graph(&server,&csrf,TITLE_SAVE,json!({"id":id,"baseline":after["data"]["manifestation"]["metadata"]["etag"],"patch":{"title":"Expires"}})).await;
        assert_graph_ok(&expiring);
        let expiring_id: uuid::Uuid =
            expiring["data"]["updateMetadata"]["operations"][0]["operationId"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
        sqlx::query!("UPDATE metadata_undo_operations SET expires_at = clock_timestamp() - interval '1 second' WHERE id = $1",expiring_id).execute(&pool).await.unwrap();
        let expired=browser_graph(&server,&csrf,UNDO,json!({"id":id,"baseline":expiring["data"]["updateMetadata"]["metadata"]["etag"],"operation":expiring_id})).await;
        assert_graph_code(&expired, "undo-unavailable");
    }

    async fn trusted_editing_document(pool: &PgPool, document_id: &str, text: &str) {
        let fixture_name = format!("trusted-{}", uuid::Uuid::new_v4());
        let (user, server, csrf) = editing_browser(pool, &fixture_name).await;
        let app = db::app_pool_for(pool).await;
        let ing = db::ingestion_pool_for(pool).await;
        let (_, edition) = db::insert_work_and_manifestation(&ing, &fixture_name).await;
        let shelf = db::create_shelf(&app, user, "Trusted shelf").await;
        let id = id::encode("Manifestation", edition);
        if text.contains("removeShelfItem(") {
            db::add_to_shelf(&app, shelf, edition).await;
        }
        if text.contains("unlockMetadataField(") {
            let locked = browser_graph(&server, &csrf, "mutation($id:ID!) { lockMetadataField(id:$id,fieldName:\"description\",entityType:\"work\") { manifestationId } }", json!({"id":id})).await;
            assert_graph_ok(&locked);
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
            baseline = saved["data"]["updateMetadata"]["metadata"]["etag"].clone();
            operation = saved["data"]["updateMetadata"]["operations"][0]["operationId"].clone();
            request = serde_json::Value::Null;
        }
        let response=server.post("/api/graphql").add_header("x-csrf-token",csrf).json(&json!({"id":document_id,"variables":{
            "id":id,"shelfId":id::encode("Shelf",shelf),"name":"Trusted shelf change",
            "baseline":baseline,"patch":patch,"requestId":request,"operationId":operation,
            "versionId":id::encode("MetadataVersion",version),"fieldName":"description","entityType":"work","first":20,"count":20
        }})).await;
        response.assert_status_ok();
        assert_graph_ok(&response.json());
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
        let user = CurrentUser::without_scopes_for_test(fixture.admin_id);
        for (policy, introspection_allowed) in [
            (ExecutionPolicy::Development, true),
            (ExecutionPolicy::Production, false),
        ] {
            let schema = schema(policy);
            let response = execute_as(
                &schema,
                user.clone(),
                "{ __schema { queryType { name } } __type(name: \"Viewer\") { name } }",
                json!({}),
            )
            .await;
            let data = &response["data"];
            if introspection_allowed {
                assert_eq!(data["__schema"]["queryType"]["name"], "Query");
                assert_eq!(data["__type"]["name"], "Viewer");
            } else {
                assert!(data["__schema"].is_null());
                assert!(data["__type"].is_null());
            }
            assert_graph_ok(&execute_as(&schema, user.clone(), "{ __typename }", json!({})).await);
            assert!(
                execute_as(
                    &schema,
                    user.clone(),
                    "query($show: Boolean!) { __typename @include(if: $show) }",
                    json!({}),
                )
                .await
                .get("errors")
                .is_some()
            );
            let complex = execute_as(
                &schema,
                user.clone(),
                &format!(
                    "{{ {} }}",
                    (0..=complexity::LIMIT / 63_000)
                        .map(|i| format!("alias{i}: series(id: \"malformed\") {{ works(first: 50) {{ edges {{ node {{ manifestations(first: 20) {{ edges {{ node {{ ratings(first: 20) {{ edges {{ node {{ source }} }} }} }} }} }} }} }} }} }}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                json!({}),
            )
            .await;
            assert_eq!(complex["errors"][0]["message"], "Query is too complex.");
            let shallow = schema_with_limits(
                policy,
                complexity::Limits {
                    cost: complexity::LIMIT,
                    depth: 1,
                },
            );
            let depth_response = execute_as(
                &shallow,
                user.clone(),
                "{ viewer { displayName } }",
                json!({}),
            )
            .await;
            assert_eq!(
                depth_response["errors"][0]["message"],
                "Query is nested too deep."
            );
        }
    }

    fn reader_variables(document: &str, fixture: &ReaderFixture) -> serde_json::Value {
        let id = if document.contains("readerShelfQuery")
            || document.contains("query readerShelfItemsPaginationQuery")
        {
            id::encode("Shelf", fixture.shelf_a)
        } else if document.contains("readerSeriesQuery")
            || document.contains("query readerSeriesWorksPaginationQuery")
        {
            id::encode("Series", fixture.series)
        } else if document.contains("query readerWorkAuthorsPaginationQuery") {
            id::encode("Work", fixture.visible_work)
        } else {
            id::encode("Manifestation", fixture.visible)
        };
        json!({
            "id": id.to_string(), "q": "Graph",
            "ids": [if document.contains("authorLabels(") { id::encode("Author",fixture.author) } else { id::encode("Series",fixture.series) }],
            "authorIds": [id::encode("Author", fixture.author).to_string()],
            "seriesIds": [id::encode("Series", fixture.series).to_string()]
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
            let created =
                chrono::DateTime::parse_from_rfc3339(&format!("2026-01-{day:02}T00:00:00Z"))
                    .unwrap();
            sqlx::query!(
                "UPDATE manifestations SET pages = $1, created_at = $2 WHERE id = $3",
                pages,
                created,
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

    // Authorisation and private-data boundary for Node refetch of paged types.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_refetch_boundary(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let app = db::app_pool_for(&pool).await;
        let (_, child) = db::create_child_user_and_basic_auth(&app, "fetch-hidden").await;
        let schema = schema(ExecutionPolicy::Development);
        let query = "query($id: ID!) { node(id: $id) { id } }";
        for id in [
            id::encode("Manifestation", fixture.hidden),
            id::encode("Shelf", fixture.other_shelf),
            id::encode("Series", fixture.series),
        ] {
            let variables = json!({ "id": id });
            let denied = execute_as(
                &schema,
                CurrentUser::without_scopes_for_test(fixture.admin_id),
                query,
                variables.clone(),
            )
            .await;
            assert_eq!(denied["errors"].as_array().unwrap().len(), 1, "{denied}");
            assert_graph_code(&denied, "forbidden");
            let hidden = reader_query(&fixture, &child, query, variables).await;
            assert!(hidden.get("errors").is_none(), "{hidden}");
            assert!(hidden["data"]["node"].is_null(), "{hidden}");
        }
    }

    // Private-data boundary; REV-REQ-0009 and REV-REQ-0059 retain caller visibility and shelf ownership.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_reader_boundary(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let visible = id::encode("Manifestation", fixture.visible);
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
            visible.to_string()
        );
        let query = "query($id: ID!) { manifestation(id: $id) { id metadata { title etag } readingState { notes etag } work { id authors(first: 5) { edges { node { name } role position } } } } }";
        let response = reader_query(
            &fixture,
            &fixture.child,
            query,
            json!({"id": visible.to_string()}),
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
            json!({"id": visible.to_string()}),
        )
        .await;
        assert!(other.get("errors").is_none(), "{other}");
        assert_eq!(
            other["data"]["manifestation"]["readingState"]["notes"],
            "Other reader note"
        );
        for invalid in [
            "not-base64".to_owned(),
            fixture.visible.to_string(),
            id::encode("Shelf", fixture.visible).to_string(),
            id::encode("Manifestation", fixture.hidden).to_string(),
            id::encode("Manifestation", uuid::Uuid::new_v4()).to_string(),
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
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $shelf: ID!, $other: ID!, $caller: ID!) { work(id: $work) { id } shelf(id: $shelf) { id } other: node(id: $other) { id } caller: node(id: $caller) { id } }", json!({"work": id::encode("Work", fixture.hidden_work).to_string(), "shelf": id::encode("Shelf", fixture.other_shelf).to_string(), "other": id::encode("User", fixture.admin_id).to_string(), "caller": id::encode("User", fixture.child_id).to_string()})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert!(response["data"]["work"].is_null());
        assert!(response["data"]["shelf"].is_null());
        assert!(response["data"]["other"].is_null());
        assert_eq!(
            response["data"]["caller"]["id"],
            id::encode("User", fixture.child_id).to_string()
        );
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $series: ID!, $shelf: ID!) { work(id: $work) { manifestations(first: 2) { edges { node { id } } } } series(id: $series) { works(first: 50) { edges { node { id manifestations(first: 2) { edges { node { id } } } } } } } shelf(id: $shelf) { items(first: 50) { edges { node { id } } } } }", json!({"work": id::encode("Work", fixture.visible_work).to_string(), "series": id::encode("Series", fixture.series).to_string(), "shelf": id::encode("Shelf", fixture.child_shelf).to_string()})).await;
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
            visible.to_string()
        );
        for hidden in [
            id::encode("Manifestation", fixture.hidden),
            id::encode("Manifestation", fixture.sibling),
            id::encode("Work", fixture.hidden_work),
        ] {
            assert!(!response.to_string().contains(&*hidden));
        }
        let response = reader_query(&fixture, &fixture.child, "query($ids: [ID!]!) { authorLabels(ids: $ids) { entityId value } search(q: \"Graph\") { manifestation { id } } authorSuggestions(q: \"Hidden\") { value } shelves(first: 50) { edges { node { id name } } } }", json!({"ids": [id::encode("Author", fixture.author).to_string(), id::encode("Author", fixture.hidden_author).to_string()]})).await;
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
            json!({"id": visible.to_string()}),
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
            (versions_query, json!({"id": visible.to_string()})),
            (
                "query($id: ID!) { node(id: $id) { ... on Manifestation { versions { pendingCount pending(first: 20) { edges { node { newValue } } } } } } }",
                json!({"id": visible.to_string()}),
            ),
            (
                "query($id: ID!) { work(id: $id) { manifestations(first: 2) { edges { node { versions { pendingCount pending(first: 20) { edges { node { newValue } } } } } } } } }",
                json!({"id": id::encode("Work", fixture.visible_work).to_string()}),
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
            json!({"id":id::encode("Manifestation",fixture.visible).to_string()}),
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
            json!({"id":id::encode("Manifestation",fixture.sibling).to_string(),"after":cursor}),
        )
        .await;
        assert_eq!(refused["errors"][0]["extensions"]["code"], "validation");
    }

    // REV-REQ-0003, REV-REQ-0057 and REV-REQ-0058 cover ranges, cursor identity and sort refusal.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_cursor_contract(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let query = "query($first: Int!, $after: String, $filter: BooksFilter, $sort: [SortLevel!]) { books(first: $first, after: $after, filter: $filter, sort: $sort) { edges { cursor node { id pages createdAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }";
        let first = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-04T00:00:00Z"}})).await;
        assert!(first.get("errors").is_none(), "{first}");
        let cursor = first["data"]["books"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        assert_eq!(first["data"]["books"]["pageInfo"]["startCursor"], cursor);
        assert_eq!(first["data"]["books"]["pageInfo"]["endCursor"], cursor);
        let next = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "after": cursor, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T08:00:00+08:00", "createdAtLt": "2026-01-04T08:00:00+08:00"}})).await;
        assert!(next.get("errors").is_none(), "{next}");
        assert_eq!(
            next["data"]["books"]["edges"][0]["node"]["id"],
            id::encode("Manifestation", fixture.sibling).to_string()
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
            json!({"first": 50, "after": cursor, "sort": [{"column": "title", "direction": "ASC"}]}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "created_at", "direction": "DESC"}]}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:01Z", "createdAtLt": "2026-01-04T00:00:00Z"}}),
            json!({"first": 50, "after": cursor, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-05T00:00:00Z"}}),
            json!({"first": 50, "sort": [{"column": "Title", "direction": "ASC"}]}),
            json!({"first": 50, "sort": [{"column": "title", "direction": "ASC"}, {"column": "title", "direction": "DESC"}]}),
            json!({"first": 50, "sort": [{"column": "title", "direction": "ASC"}, {"column": "author", "direction": "ASC"}, {"column": "pages", "direction": "ASC"}, {"column": "created_at", "direction": "ASC"}]}),
        ] {
            let response = reader_query(&fixture, &fixture.admin, query, variables).await;
            assert_eq!(
                response["errors"][0]["extensions"]["code"], "validation",
                "{response}"
            );
            assert!(response["data"]["books"].is_null());
        }
        let ordered = reader_query(&fixture, &fixture.admin, query, json!({"first": 1, "sort": [{"column": "title", "direction": "ASC"}, {"column": "created_at", "direction": "ASC"}]})).await;
        assert!(ordered.get("errors").is_none(), "{ordered}");
        let ordered_cursor = ordered["data"]["books"]["edges"][0]["cursor"]
            .as_str()
            .unwrap();
        let refused = reader_query(&fixture, &fixture.admin, query, json!({"first": 50, "after": ordered_cursor, "sort": [{"column": "created_at", "direction": "ASC"}, {"column": "title", "direction": "ASC"}]})).await;
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
                id::encode("Work", fixture.visible_work),
                id::encode("Work", fixture.hidden_work),
            ),
            (
                "shelf",
                id::encode("Shelf", fixture.shelf_a),
                id::encode("Shelf", fixture.shelf_b),
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
                json!({"id": parent_a.to_string()}),
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
                    json!({"id": parent_b.to_string(), "after": nested_cursor}),
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
