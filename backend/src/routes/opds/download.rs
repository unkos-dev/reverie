//! `GET /opds/books/:id/file` — streamed EPUB download.
//!
//! Lookups run inside `acquire_with_rls` so unauthorised users (or child
//! accounts where the manifestation isn't on one of their shelves) get
//! `NotFound` via RLS. Authorised paths open through the pinned library
//! capability; the opened handle supplies both metadata and streamed bytes.
//! Relative and absolute links resolving inside the library remain supported.

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::extract::ApiPath;
use crate::models::storage_library::LibraryId;
use crate::services::files::{LibraryFileError, LibraryLocation, OpenedLibraryFile};
use crate::state::AppState;

use super::feed::EPUB_MIME;

/// Build the OPDS download router.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(download_epub))
}

/// Streamed EPUB download.
///
/// The response carries an RFC 6266 `Content-Disposition: attachment` header
/// whose filename is derived from the work title (ASCII fallback plus RFC 5987
/// UTF-8 form).
///
/// # Errors
/// - [`AppError::NotFound`] when the manifestation is missing, RLS-hidden,
///   or its file is gone from disk.
/// - [`AppError::Forbidden`] when the on-disk path escapes the configured
///   library root during classification.
/// - [`AppError::Internal`] on database or other filesystem errors, including
///   ambiguous `PermissionDenied` from the contained open. Internal details are hidden.
#[utoipa::path(
    get,
    path = "/opds/books/{id}/file",
    summary = "Download an EPUB",
    description = "Streams one opened EPUB file for a manifestation, with Content-Length from that handle and a `Content-Disposition: attachment` header carrying a title-derived filename. Requires HTTP Basic authentication. Relative and absolute links resolving inside the library are supported. Established escapes return 403, missing files return 404, and other filesystem failures return a generic 500.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "EPUB byte stream; Content-Disposition: attachment with a title-derived filename", content_type = "application/epub+zip"),
        (status = 400, description = "A path parameter is malformed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "File path escapes the library root", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing, RLS-hidden, or file absent on disk", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Internal error; filesystem and database details are hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn download_epub(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
) -> Result<Response, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = sqlx::query!(
        "SELECT m.library_id, m.file_path, w.title FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.ok_or(AppError::NotFound)?;
    let location = LibraryLocation {
        library_id: LibraryId::from_uuid(row.library_id),
        path: row.file_path.parse().map_err(download_error)?,
    };
    let title = row.title;
    drop(tx);

    let opened = state
        .library_files
        .open_download(&location)
        .await
        .map_err(download_error)?;
    stream_download(opened, &title, manifestation_id)
}

pub(super) fn download_error(error: LibraryFileError) -> AppError {
    match error {
        LibraryFileError::OutsideLibrary => AppError::Forbidden,
        LibraryFileError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => AppError::NotFound,
        other => AppError::Internal(anyhow::Error::new(other).context("opening library download")),
    }
}

pub(super) fn stream_download(
    opened: OpenedLibraryFile,
    title: &str,
    manifestation_id: Uuid,
) -> Result<Response, AppError> {
    let stream = ReaderStream::new(File::from_std(opened.file));
    let body = Body::from_stream(stream);

    let disposition = content_disposition(title, manifestation_id);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, EPUB_MIME)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, opened.metadata.len())
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}

/// Build `Content-Disposition: attachment; filename="…"; filename*=UTF-8''…`
/// per RFC 6266 §4.1. ASCII fallback is derived from title; RFC 5987
/// extended form carries the full UTF-8 title. Falls back to
/// `reverie-{uuid6}.epub` if title is empty.
fn content_disposition(title: &str, manifestation_id: Uuid) -> String {
    let ascii = ascii_fallback(title, manifestation_id);
    let encoded = rfc5987_encode(title);
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

fn ascii_fallback(title: &str, manifestation_id: Uuid) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == ' ' || c == '-' || c == '_' {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        format!("reverie-{}.epub", short_uuid(manifestation_id))
    } else {
        format!("{trimmed}.epub")
    }
}

fn short_uuid(id: Uuid) -> String {
    // First 8 hex chars of the simple form (no hyphens).
    id.simple().to_string().chars().take(8).collect()
}

/// Percent-encode per RFC 5987 §3.2.1 attr-char set. Everything outside the
/// attr-char set (`ALPHA / DIGIT / "!" / "#" / "$" / "&" / "+" / "-" / "." /
/// "^" / "_" / backtick / "|" / "~"`) is percent-encoded.
const RFC5987_NON_ATTR_CHAR: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'%')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'{')
    .add(b'}');

fn rfc5987_encode(s: &str) -> String {
    utf8_percent_encode(s, RFC5987_NON_ATTR_CHAR).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_fallback_strips_non_ascii() {
        let id = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(ascii_fallback("Hello World", id), "Hello-World.epub");
        // Non-ASCII letters are dropped entirely; surrounding spaces collapse
        // so we don't get a runaway dash chain.
        assert_eq!(ascii_fallback("émile et à côté", id), "mile-et-ct.epub");
    }

    #[test]
    fn ascii_fallback_empty_falls_back_to_uuid() {
        let id = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(ascii_fallback("", id), "reverie-00000000.epub");
        assert_eq!(ascii_fallback("🚀", id), "reverie-00000000.epub");
    }

    #[test]
    fn rfc5987_encode_percent_encodes_spaces_and_utf8() {
        assert_eq!(rfc5987_encode("Hello World"), "Hello%20World");
        assert_eq!(rfc5987_encode("émilie"), "%C3%A9milie");
    }

    #[test]
    fn content_disposition_format() {
        let id = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        let cd = content_disposition("Winnie the Pooh", id);
        assert!(cd.starts_with("attachment;"));
        assert!(cd.contains("filename=\"Winnie-the-Pooh.epub\""));
        assert!(cd.contains("filename*=UTF-8''Winnie%20the%20Pooh"));
    }
}
