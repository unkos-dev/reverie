//! OpenSearch descriptors. Separate endpoints per scope so a reader paired
//! at `/opds/shelves/{id}` gets a search URL scoped to that shelf.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Cursor;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::{ACQUISITION_TYPE, OPENSEARCH_NS};
use super::root::base_url;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/opds/library/opensearch.xml", get(library_opensearch))
        .route(
            "/opds/shelves/{shelf_id}/opensearch.xml",
            get(shelf_opensearch),
        )
}

async fn library_opensearch(
    BasicOnly(_user): BasicOnly,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let template = base
        .join("/opds/library/search?q={searchTerms}")
        .map(|u| u.to_string())
        .unwrap_or_else(|_| "/opds/library/search?q={searchTerms}".into());
    let body = build_opensearch_xml("Reverie", "Search Reverie library", &template);
    Ok(build_response(body))
}

async fn shelf_opensearch(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();

    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM shelves \
         WHERE id = $1 \
           AND user_id = current_setting('app.current_user_id', true)::uuid \
         LIMIT 1",
    )
    .bind(shelf_id)
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
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("/opds/shelves/{shelf_id}/search?q={{searchTerms}}"));
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
