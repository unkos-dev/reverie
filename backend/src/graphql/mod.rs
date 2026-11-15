//! Authenticated GraphQL execution with build-generated document admission.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_graphql::{Context, EmptyMutation, EmptySubscription, ErrorExtensions, Object, Schema};
use async_graphql_axum::GraphQLResponse;
use axum::extract::{Extension, State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::state::AppState;

mod complexity;
mod id;
mod library;
mod loaders;
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
            id: id::encode("User", user.user_id),
            is_child: user.is_child(),
        })
    }
    #[graphql(guard = "ReadScope")]
    async fn node(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<library::Node>> {
        if let Some(user_id) = id::decode("User", &id) {
            if user_id == ctx.data::<CurrentUser>()?.user_id {
                return self
                    .viewer(ctx)
                    .await
                    .map(|viewer| Some(library::Node::Viewer(viewer)));
            }
            return Ok(None);
        }
        if id::decode("Work", &id).is_some() {
            return library::work(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Work));
        }
        if id::decode("Manifestation", &id).is_some() {
            return library::manifestation(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Manifestation));
        }
        if id::decode("Shelf", &id).is_some() {
            return library::shelf(ctx, &id)
                .await
                .map(|value| value.map(library::Node::Shelf));
        }
        if id::decode("Series", &id).is_some() {
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
    #[graphql(
        guard = "ReadScope",
        complexity = "complexity::page(first, child_complexity)?"
    )]
    async fn books(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        filter: Option<library::BooksFilter>,
        sort: Option<Vec<library::SortLevel>>,
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
        first: Option<i32>,
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
pub type AppSchema = Schema<Query, EmptyMutation, EmptySubscription>;

/// Construct a schema with finite execution limits and an explicit admission policy.
#[must_use]
pub fn schema(policy: ExecutionPolicy) -> AppSchema {
    schema_builder(policy).finish()
}

fn schema_builder(
    policy: ExecutionPolicy,
) -> async_graphql::SchemaBuilder<Query, EmptyMutation, EmptySubscription> {
    let builder = Schema::build(Query, EmptyMutation, EmptySubscription)
        .limit_depth(complexity::DEPTH)
        .limit_complexity(complexity::LIMIT)
        .extension(complexity::Defaults)
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
    use base64ct::Encoding;
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
        let id = if document.contains("readerShelfQuery") {
            id::encode("Shelf", fixture.shelf_a)
        } else if document.contains("readerSeriesQuery") {
            id::encode("Series", fixture.series)
        } else {
            id::encode("Manifestation", fixture.visible)
        };
        json!({
            "id": id.as_str(), "q": "Graph",
            "authorIds": [id::encode("Author", fixture.author).as_str()],
            "seriesIds": [id::encode("Series", fixture.series).as_str()]
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

    // Private-data boundary; REV-REQ-0009 and REV-REQ-0059 retain caller visibility and shelf ownership.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_reader_boundary(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let visible = id::encode("Manifestation", fixture.visible);
        let response = reader_query(
            &fixture,
            &fixture.child,
            "{ books { edges { node { id } } } }",
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
        let query = "query($id: ID!) { manifestation(id: $id) { id metadata { title etag } readingState { notes etag } work { id authors { edges { node { name } role position } } } } }";
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
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $shelf: ID!, $other: ID!, $caller: ID!) { work(id: $work) { id } shelf(id: $shelf) { id } other: node(id: $other) { id } caller: node(id: $caller) { id } }", json!({"work": id::encode("Work", fixture.hidden_work).as_str(), "shelf": id::encode("Shelf", fixture.other_shelf).as_str(), "other": id::encode("User", fixture.admin_id).as_str(), "caller": id::encode("User", fixture.child_id).as_str()})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert!(response["data"]["work"].is_null());
        assert!(response["data"]["shelf"].is_null());
        assert!(response["data"]["other"].is_null());
        assert_eq!(
            response["data"]["caller"]["id"],
            id::encode("User", fixture.child_id).as_str()
        );
        let response = reader_query(&fixture, &fixture.child, "query($work: ID!, $series: ID!, $shelf: ID!) { work(id: $work) { manifestations { edges { node { id } } } } series(id: $series) { works { edges { node { id manifestations { edges { node { id } } } } } } } shelf(id: $shelf) { items { edges { node { id } } } } }", json!({"work": id::encode("Work", fixture.visible_work).as_str(), "series": id::encode("Series", fixture.series).as_str(), "shelf": id::encode("Shelf", fixture.child_shelf).as_str()})).await;
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
            id::encode("Manifestation", fixture.hidden),
            id::encode("Manifestation", fixture.sibling),
            id::encode("Work", fixture.hidden_work),
        ] {
            assert!(!response.to_string().contains(hidden.as_str()));
        }
        let response = reader_query(&fixture, &fixture.child, "query($ids: [ID!]!) { authorLabels(ids: $ids) { entityId value } search(q: \"Graph\") { manifestation { id } } authorSuggestions(q: \"Hidden\") { value } shelves { edges { node { id name } } } }", json!({"ids": [id::encode("Author", fixture.author).as_str(), id::encode("Author", fixture.hidden_author).as_str()]})).await;
        assert!(response.get("errors").is_none(), "{response}");
        assert_eq!(
            response["data"]["authorLabels"].as_array().unwrap().len(),
            1
        );
        assert_eq!(response["data"]["search"].as_array().unwrap().len(), 1);
        assert_eq!(response["data"]["authorSuggestions"], json!([]));
        assert!(!response.to_string().contains("Other graph shelf"));
        let versions_query = "query($id: ID!) { manifestation(id: $id) { versions { pendingCount pending { edges { node { newValue } } } } } }";
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
                "query($id: ID!) { node(id: $id) { ... on Manifestation { versions { pendingCount pending { edges { node { newValue } } } } } } }",
                json!({"id": visible.as_str()}),
            ),
            (
                "query($id: ID!) { work(id: $id) { manifestations { edges { node { versions { pendingCount pending { edges { node { newValue } } } } } } } } }",
                json!({"id": id::encode("Work", fixture.visible_work).as_str()}),
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
            json!({"id":id::encode("Manifestation",fixture.visible).as_str()}),
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
            json!({"id":id::encode("Manifestation",fixture.sibling).as_str(),"after":cursor}),
        )
        .await;
        assert_eq!(refused["errors"][0]["extensions"]["code"], "validation");
    }

    // REV-REQ-0003, REV-REQ-0057 and REV-REQ-0058 cover ranges, cursor identity and sort refusal.
    #[sqlx::test(migrations = "./migrations")]
    async fn graphql_cursor_contract(pool: PgPool) {
        let fixture = reader_fixture(&pool).await;
        let query = "query($first: Int, $after: String, $filter: BooksFilter, $sort: [SortLevel!]) { books(first: $first, after: $after, filter: $filter, sort: $sort) { edges { cursor node { id pages createdAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }";
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
            id::encode("Manifestation", fixture.sibling).as_str()
        );
        assert_eq!(next["data"]["books"]["pageInfo"]["hasPreviousPage"], true);
        for filter in [
            json!({"pagesGte": 20, "pagesLte": 20}),
            json!({"pagesGte": 30, "pagesLte": 10}),
            json!({"createdAtGte": "2026-01-02T00:00:00Z", "createdAtLt": "2026-01-02T00:00:00Z"}),
            json!({"createdAtGte": "2026-01-03T00:00:00Z", "createdAtLt": "2026-01-01T00:00:00Z"}),
            json!({"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-02T00:00:00Z"}),
        ] {
            let response =
                reader_query(&fixture, &fixture.admin, query, json!({"filter": filter})).await;
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
            json!({"after": cursor, "sort": [{"column": "title", "direction": "ASC"}]}),
            json!({"after": cursor, "sort": [{"column": "created_at", "direction": "DESC"}]}),
            json!({"after": cursor, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:01Z", "createdAtLt": "2026-01-04T00:00:00Z"}}),
            json!({"after": cursor, "sort": [{"column": "created_at", "direction": "ASC"}], "filter": {"createdAtGte": "2026-01-01T00:00:00Z", "createdAtLt": "2026-01-05T00:00:00Z"}}),
            json!({"sort": [{"column": "Title", "direction": "ASC"}]}),
            json!({"sort": [{"column": "title", "direction": "ASC"}, {"column": "title", "direction": "DESC"}]}),
            json!({"sort": [{"column": "title", "direction": "ASC"}, {"column": "author", "direction": "ASC"}, {"column": "pages", "direction": "ASC"}, {"column": "created_at", "direction": "ASC"}]}),
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
        let refused = reader_query(&fixture, &fixture.admin, query, json!({"after": ordered_cursor, "sort": [{"column": "created_at", "direction": "ASC"}, {"column": "title", "direction": "ASC"}]})).await;
        assert_eq!(
            refused["errors"][0]["extensions"]["code"], "validation",
            "{refused}"
        );
        let refused = reader_query(
            &fixture,
            &fixture.admin,
            "query($after: String!) { shelves(after: $after) { edges { node { id } } } }",
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
            json!({"after": tagged_cursor, "filter": {"tagAny": ["beta", "alpha"]}}),
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
