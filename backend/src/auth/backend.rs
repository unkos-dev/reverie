use axum_login::{AuthnBackend, UserId};
use sqlx::PgPool;

use crate::models::user::{self, User};

/// Credentials produced after validating an OIDC callback.
#[derive(Clone)]
pub struct OidcCredentials {
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
}

/// Authentication backend that upserts users from OIDC claims.
#[derive(Clone)]
pub struct AuthBackend {
    pub pool: PgPool,
}

impl AuthnBackend for AuthBackend {
    type User = User;
    type Credentials = OidcCredentials;
    type Error = sqlx::Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = user::upsert_from_oidc_and_maybe_promote(
            &self.pool,
            &creds.subject,
            &creds.display_name,
            creds.email.as_deref(),
        )
        .await?;
        Ok(Some(user))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        user::find_by_id(&self.pool, *user_id).await
    }
}
