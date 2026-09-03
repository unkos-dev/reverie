//! `OpenSearch` descriptors. Separate endpoints per scope so a reader paired
//! at `/opds/shelves/{id}` gets a search URL scoped to that shelf.
//!
//! # Why `expect_used` is allowed here
//!
//! All `expect()` calls write to a `Writer<Cursor<Vec<u8>>>` or build a
//! `Response` from static status/header values. `Cursor<Vec<u8>>` writes are
//! infallible; `Response::builder()` with a valid `StatusCode` and a valid
//! ASCII header value cannot fail. Making these return `Result` would cascade
//! error-handling into every call site for error paths that cannot occur.
#![expect(
    clippy::expect_used,
    reason = "all expects write to Cursor<Vec<u8>> (infallible) or build Response from static inputs (cannot fail)"
)]

use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Cursor;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::{ACQUISITION_TYPE, OPENSEARCH_NS};
use super::root::base_url;

/// Build the `OpenSearch` descriptor router (one per scope so a reader
/// paired at `/opds/shelves/:id` gets a search URL scoped to that
/// shelf).
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(library_opensearch))
        .routes(routes!(shelf_opensearch))
}

/// `GET /opds/library/opensearch.xml` — `OpenSearch` descriptor whose
/// search template targets the library-wide `/opds/library/search`.
///
/// # Errors
/// - [`AppError::Internal`] when the OPDS base URL is unconfigured.
#[utoipa::path(
    get,
    path = "/opds/library/opensearch.xml",
    tag = "opds",
    security(("opds_basic" = [])),
    responses(
        (status = 200, description = "OpenSearch descriptor with the library-scoped search URL template", content_type = "application/opensearchdescription+xml", body = String),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic); body is empty", body = String)
    )
)]
async fn library_opensearch(
    BasicOnly(_user): BasicOnly,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let template = base
        .join("/opds/library/search?q={searchTerms}")
        .map_or_else(
            |_| "/opds/library/search?q={searchTerms}".into(),
            |u| u.to_string(),
        );
    let body = build_opensearch_xml("Reverie", "Search Reverie library", &template);
    Ok(build_response(body))
}

/// `GET /opds/shelves/{shelf_id}/opensearch.xml` — `OpenSearch` descriptor
/// whose search template is scoped to the shelf.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/opensearch.xml",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id")),
    responses(
        (status = 200, description = "OpenSearch descriptor with the shelf-scoped search URL template", content_type = "application/opensearchdescription+xml", body = String),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic); body is empty", body = String),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_opensearch(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();

    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned = sqlx::query_scalar!(
        "SELECT id FROM shelves \
         WHERE id = $1 \
           AND user_id = current_setting('app.current_user_id', true)::uuid \
         LIMIT 1",
        shelf_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }

    let template = base
        .join(&format!(
            "/opds/shelves/{shelf_id}/search?q={{searchTerms}}"
        ))
        .map_or_else(
            |_| format!("/opds/shelves/{shelf_id}/search?q={{searchTerms}}"),
            |u| u.to_string(),
        );
    let body = build_opensearch_xml("Reverie Shelf", "Search shelf contents", &template);
    Ok(build_response(body))
}

fn build_response(body: Vec<u8>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml",
        )
        .body(axum::body::Body::from(body))
        .expect("build opensearch response")
}

fn build_opensearch_xml(short_name: &str, description: &str, template: &str) -> Vec<u8> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect("xml decl");

    let mut root = BytesStart::new("OpenSearchDescription");
    root.push_attribute(("xmlns", OPENSEARCH_NS));
    writer.write_event(Event::Start(root)).expect("root open");

    write_text(&mut writer, "ShortName", short_name);
    write_text(&mut writer, "Description", description);

    let mut url = BytesStart::new("Url");
    url.push_attribute(("type", ACQUISITION_TYPE));
    url.push_attribute(("template", template));
    writer.write_event(Event::Empty(url)).expect("url");

    writer
        .write_event(Event::End(BytesEnd::new("OpenSearchDescription")))
        .expect("root close");
    writer.into_inner().into_inner()
}

fn write_text(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str, text: &str) {
    writer
        .write_event(Event::Start(BytesStart::new(name)))
        .expect("text open");
    writer
        .write_event(Event::Text(BytesText::new(text)))
        .expect("text");
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .expect("text close");
}
