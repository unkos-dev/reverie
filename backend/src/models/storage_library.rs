//! Persistent library identity independent of its configured filesystem root.

use sqlx::PgPool;
use uuid::Uuid;

/// The owning library of a recorded file location.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, sqlx::Type)]
#[sqlx(transparent)]
pub struct LibraryId(Uuid);

impl LibraryId {
    /// Decode a database identity without assigning filesystem authority.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// The persisted UUID used in foreign keys.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Select the identity bound to the installation's configured library root.
///
/// # Errors
/// Returns a database error, including a missing default identity.
pub async fn default_library_id(pool: &PgPool) -> Result<LibraryId, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM libraries WHERE configuration_key = 'default'")
        .fetch_one(pool)
        .await
        .map(LibraryId::from_uuid)
}
