//! `BasicOnly` extractor: rejects session cookies, requires
//! `Authorization: Basic …`, emits an RFC 7617 challenge on failure.
//!
//! Used by the `/opds/*` routes so that OPDS reader apps (KOReader, Moon+,
//! Librera, KyBook 3) receive a 401 with `WWW-Authenticate: Basic realm="…",
//! charset="UTF-8"` and prompt for credentials — the cookie-or-Basic
//! [`CurrentUser`] extractor returns a JSON 401 without a challenge, which
//! mobile clients silently treat as an error.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::auth::middleware::{CurrentUser, verify_basic};
use crate::error::AppError;
use crate::state::AppState;

/// Wraps a [`CurrentUser`] that was authenticated via `Authorization: Basic`
/// only. Session cookies are ignored and a missing or invalid Basic header
/// returns [`AppError::BasicAuthRequired`], which emits the RFC 7617
/// challenge.
#[derive(Debug, Clone)]
pub struct BasicOnly(pub CurrentUser);

impl FromRequestParts<AppState> for BasicOnly {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match verify_basic(state, parts).await {
            Ok(Some(user)) => Ok(BasicOnly(user)),
            Ok(None) | Err(AppError::Unauthorized) => Err(AppError::BasicAuthRequired {
                realm: state.config.opds.realm.clone(),
            }),
            Err(other) => Err(other),
        }
    }
}
