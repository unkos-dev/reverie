//! Transaction-bound ownership of managed library names.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::services::files::LibraryLocation;

/// Exclude other managed publishers and cleanup for this exact name.
///
/// # Errors
/// Returns database failures; the caller retains its transaction through mutation.
pub async fn exclude(
    connection: &mut Transaction<'_, Postgres>,
    location: &LibraryLocation,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2, 0))",
        location.library_id.as_uuid().to_string(),
        location.path.as_str(),
    )
    .execute(&mut **connection)
    .await?;
    Ok(())
}

/// Read the committed or transaction-local owner of a name.
///
/// # Errors
/// Returns database failures, including unavailable ownership evidence.
pub async fn owner(
    connection: &mut Transaction<'_, Postgres>,
    location: &LibraryLocation,
) -> Result<Option<Uuid>, sqlx::Error> {
    let evidence = sqlx::query!(
        "SELECT NOT row_security_active('public.library_path_claims')
                OR pg_has_role('reverie_ingestion', 'USAGE')
                OR (pg_has_role('reverie_app', 'USAGE')
                    AND COALESCE(current_setting('app.system_context', TRUE) = 'writeback', FALSE))
                AS \"available!\",
                (SELECT manifestation_id FROM library_path_claims
                 WHERE library_id = $1 AND path = $2) AS manifestation_id",
        location.library_id.as_uuid(),
        location.path.as_str(),
    )
    .fetch_one(&mut **connection)
    .await?;
    if !evidence.available {
        return Err(sqlx::Error::Protocol(
            "path ownership evidence is unavailable to this role".into(),
        ));
    }
    Ok(evidence.manifestation_id)
}

/// Reserve a name for this owner, refusing any competing owner.
///
/// # Errors
/// Returns database failures; deferred references are checked at transaction commit.
pub async fn reserve(
    connection: &mut Transaction<'_, Postgres>,
    location: &LibraryLocation,
    manifestation_id: Uuid,
) -> Result<bool, sqlx::Error> {
    exclude(connection, location).await?;
    Ok(sqlx::query_scalar!(
        "INSERT INTO library_path_claims (library_id, path, manifestation_id) VALUES ($1, $2, $3)
         ON CONFLICT (library_id, path) DO UPDATE SET manifestation_id = library_path_claims.manifestation_id
         WHERE library_path_claims.manifestation_id = EXCLUDED.manifestation_id
         RETURNING manifestation_id",
        location.library_id.as_uuid(),
        location.path.as_str(),
        manifestation_id,
    )
    .fetch_optional(&mut **connection)
    .await?
    .is_some())
}

/// Release only names no longer recorded or retained by an intent.
///
/// # Errors
/// Returns database failures; location finalisation belongs to the same transaction.
pub async fn release_obsolete(
    connection: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM library_path_claims AS c USING manifestations AS m
         WHERE c.manifestation_id = $1 AND m.id = c.manifestation_id
         AND c.path <> m.file_path
         AND c.path IS DISTINCT FROM m.relocation_source_path
         AND c.path IS DISTINCT FROM m.relocation_destination_path",
        manifestation_id,
    )
    .execute(&mut **connection)
    .await?;
    Ok(())
}
