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
            id: async_graphql::ID(format!("User:{}", user.user_id)),
            is_child: user.is_child(),
        })
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
        .limit_depth(8)
        .limit_complexity(64)
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
            format!("User:{user_id}")
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
        let (server, auth, _) = server(&pool).await;
        for id in documents.keys() {
            let envelope = serde_json::from_value(json!({"id":id})).unwrap();
            assert_eq!(
                document(&envelope, ExecutionPolicy::Production, &documents).unwrap(),
                documents[id]
            );
            let response = server
                .post("/api/graphql")
                .add_header("authorization", auth.clone())
                .json(&json!({"id": id}))
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
            let schema = schema(policy);
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
                        "{{ viewer {{ {} }} }}",
                        (0..65)
                            .map(|i| format!("alias{i}: displayName"))
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
}
