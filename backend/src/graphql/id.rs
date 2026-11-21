use type_safe_id::{StaticType, TypeSafeId};
use uuid::Uuid;

pub(super) trait Kind: StaticType {
    const NAME: &str;

    fn encode(id: Uuid) -> async_graphql::ID {
        async_graphql::ID(TypeSafeId::<Self>::from_uuid(id).to_string())
    }

    fn decode(id: &async_graphql::ID) -> Option<Uuid> {
        // THREAT: malformed and wrong-type IDs have the same outcome as hidden or missing records.
        // The parser copies a mismatched prefix of any length, so the length cap applies first.
        if id.as_str().len() > 80 {
            return None;
        }
        id.as_str()
            .parse::<TypeSafeId<Self>>()
            .ok()
            .map(|value| value.uuid())
    }
}

macro_rules! kinds {
    ($($name:ident => $prefix:literal),* $(,)?) => {$(
        #[derive(Default)]
        pub(super) struct $name;
        impl StaticType for $name {
            const TYPE: &'static str = $prefix;
        }
        impl Kind for $name {
            const NAME: &'static str = stringify!($name);
        }
        const _: () = assert!($name::TYPE_PREFIX_IS_VALID);
    )*};
}

/// Reverie's RFC 9562 §6.6 application-specific namespace for metadata revision IDs.
const METADATA_REVISION_NAMESPACE: Uuid = uuid::uuid!("700a2922-1735-49de-a6ea-910315c7da8f");

/// Name-based `UUIDv8` (RFC 9562 §5.5, Appendix B.2) over the revision's canonical serialisation, so each revision has
/// its own ID and the same revision always has the same one.
pub(super) fn metadata_revision(
    revision: &crate::routes::etag::MetadataRevision,
) -> Result<async_graphql::ID, crate::error::AppError> {
    use sha2::{Digest, Sha256};
    let name = serde_json::to_vec(revision)
        .map_err(|error| crate::error::AppError::Internal(error.into()))?;
    let digest = Sha256::new()
        .chain_update(METADATA_REVISION_NAMESPACE.as_bytes())
        .chain_update(name)
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    Ok(MetadataRevision::encode(
        uuid::Builder::from_custom_bytes(bytes).into_uuid(),
    ))
}

kinds! {
    Author => "author",
    Genre => "genre",
    Manifestation => "book",
    MetadataRevision => "metadata_rev",
    MetadataVersion => "metadata_version",
    Mood => "mood",
    Series => "series",
    Shelf => "shelf",
    Tag => "tag",
    User => "user",
    Work => "work",
}
