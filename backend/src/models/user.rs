use axum_login::AuthUser;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

const USER_COLUMNS: &str = "id, oidc_subject, display_name, email, role::text, is_child, \
                            created_at, updated_at, session_version, theme_preference";

/// Raw row from the database. Use `User::from` to get the public type.
#[derive(Debug, Clone, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    oidc_subject: String,
    display_name: String,
    email: Option<String>,
    role: String, // Decoded from role::text cast in query
    is_child: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    session_version: i32,
    theme_preference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Uuid,
    pub oidc_subject: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub is_child: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub session_version: i32,
    pub theme_preference: String,
    #[serde(skip)]
    session_version_bytes: Vec<u8>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        let session_version_bytes = row.session_version.to_le_bytes().to_vec();
        Self {
            id: row.id,
            oidc_subject: row.oidc_subject,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            is_child: row.is_child,
            created_at: row.created_at,
            updated_at: row.updated_at,
            session_version: row.session_version,
            theme_preference: row.theme_preference,
            session_version_bytes,
        }
    }
}

impl AuthUser for User {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        // Intentional session invalidation: incrementing session_version forces
        // logout of all sessions for this user. This is preferred over hashing
        // updated_at because it only invalidates when we explicitly want it to
        // (e.g., admin action, security event), not on every profile update.
        &self.session_version_bytes
    }
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(User::from))
}

#[allow(dead_code)] // Used by admin user management in future steps
pub async fn find_by_oidc_subject(
    pool: &PgPool,
    subject: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE oidc_subject = $1"
    ))
    .bind(subject)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(User::from))
}

/// Insert or update a user from OIDC claims, then auto-promote to admin if first user.
/// Runs upsert + promotion in a single transaction to prevent race conditions where
/// concurrent first logins result in no admin.
pub async fn upsert_from_oidc_and_maybe_promote(
    pool: &PgPool,
    subject: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<User, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Serialize concurrent first-user promotion attempts. Without this lock,
    // two concurrent transactions under READ COMMITTED could both see count=1
    // (their own uncommitted insert) and both promote to admin.
    sqlx::query("SELECT pg_advisory_xact_lock(42)")
        .execute(&mut *tx)
        .await?;

    let row = sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO users (oidc_subject, display_name, email) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (oidc_subject) DO UPDATE \
           SET display_name = EXCLUDED.display_name, \
               email = EXCLUDED.email, \
               updated_at = now() \
         RETURNING {USER_COLUMNS}"
    ))
    .bind(subject)
    .bind(display_name)
    .bind(email)
    .fetch_one(&mut *tx)
    .await?;

    // Promote to admin if this is the only user in the table.
    sqlx::query(
        "UPDATE users SET role = 'admin'::user_role, updated_at = now() \
         WHERE id = $1 AND (SELECT count(*) FROM users) = 1",
    )
    .bind(row.id)
    .execute(&mut *tx)
    .await?;

    // Re-fetch to get potentially updated role
    let row =
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(row.id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;
    Ok(User::from(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_creates_and_updates_user(pool: PgPool) {
        let subject = format!("test-subject-{}", Uuid::new_v4());
        let user =
            upsert_from_oidc_and_maybe_promote(&pool, &subject, "Alice", Some("alice@example.com"))
                .await
                .expect("upsert");
        assert_eq!(user.display_name, "Alice");
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
        // First user in a fresh DB is auto-promoted to admin.
        assert_eq!(user.role, "admin");
        assert_eq!(user.session_version, 0);
        assert_eq!(user.session_version_bytes, 0_i32.to_le_bytes());

        let updated = upsert_from_oidc_and_maybe_promote(
            &pool,
            &subject,
            "Alice B",
            Some("alice-b@example.com"),
        )
        .await
        .expect("upsert update");
        assert_eq!(updated.id, user.id);
        assert_eq!(updated.display_name, "Alice B");

        let found = find_by_id(&pool, user.id).await.expect("find").unwrap();
        assert_eq!(found.oidc_subject, subject);

        let found = find_by_oidc_subject(&pool, &subject)
            .await
            .expect("find by subject")
            .unwrap();
        assert_eq!(found.id, user.id);
    }
}
