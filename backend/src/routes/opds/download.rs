//! `GET /opds/books/:id/file` — streamed EPUB download.
//!
//! Lookups run inside `acquire_with_rls` so unauthorised users (or child
//! accounts where the manifestation isn't on one of their shelves) get
//! `NotFound` via RLS. A subsequent canonicalisation guard prevents any
//! on-disk `file_path` from escaping `library_path`.

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, State};
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
use crate::state::AppState;

use super::feed::EPUB_MIME;

/// Build the OPDS download router.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(download_epub))
}

/// `GET /opds/books/{id}/file` — streamed EPUB download. The response
/// carries an RFC 6266 `Content-Disposition: attachment` header whose
/// filename is derived from the work title (ASCII fallback plus RFC 5987
/// UTF-8 form).
///
/// # Errors
/// - [`AppError::NotFound`] when the manifestation is missing, RLS-hidden,
///   or its file is gone from disk.
/// - [`AppError::Forbidden`] when the on-disk path escapes the configured
///   library root (canonicalisation guard).
/// - [`AppError::Internal`] on database or file IO errors.
#[utoipa::path(
    get,
    path = "/opds/books/{id}/file",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "EPUB byte stream; Content-Disposition: attachment with a title-derived filename", content_type = "application/epub+zip"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic); body is empty", body = String),
        (status = 403, description = "File path escapes the library root", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing, RLS-hidden, or file absent on disk", body = crate::openapi::ProblemDetails)
    )
)]
async fn download_epub(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = sqlx::query!(
        "SELECT m.file_path, w.title FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.ok_or(AppError::NotFound)?;
    let file_path = row.file_path;
    let title = row.title;
    drop(tx);

    let library_path = state.config.library_path.clone();
    let canonical = canonicalise_file_for_download(&file_path, &library_path).await?;

    let metadata = match tokio::fs::metadata(&canonical).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(e.into())),
    };

    let file = match File::open(&canonical).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(e.into())),
    };
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let disposition = content_disposition(&title, manifestation_id);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, EPUB_MIME)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, metadata.len())
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}

async fn canonicalise_file_for_download(
    file_path: &str,
    library_path: &str,
) -> Result<PathBuf, AppError> {
    // Copy owned so we can move into spawn_blocking. Join operator for Result
    // keeps the async signature clean.
    let file_path = file_path.to_owned();
    let library_path = library_path.to_owned();
    tokio::task::spawn_blocking(move || {
        let file_canonical = match std::fs::canonicalize(&file_path) {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::NotFound);
            }
            Err(e) => return Err(AppError::Internal(e.into())),
        };
        let library_canonical =
            std::fs::canonicalize(&library_path).map_err(|e| AppError::Internal(e.into()))?;
        if !file_canonical.starts_with(&library_canonical) {
            return Err(AppError::Forbidden);
        }
        Ok(file_canonical)
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
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
