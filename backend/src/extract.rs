//! Request extractors whose rejections are Problem Details.
//!
//! Axum's `Path` and `Json` answer a malformed request with a plain-text body
//! before the handler runs. These wrappers call the same extractors and map
//! each rejection into [`AppError`], so every failure the API raises on the
//! way into a handler is an `application/problem+json` document. They are
//! request-only: `axum::Json` remains the response type.

use axum::extract::{FromRequest, FromRequestParts};

use crate::error::AppError;

/// Path parameters; a value that fails to parse is a 400 `malformed-path`.
#[derive(Debug, FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct ApiPath<T>(pub T);

/// JSON request body; a rejection keeps axum's status under `invalid-request-body`.
#[derive(Debug, FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct ApiJson<T>(pub T);
