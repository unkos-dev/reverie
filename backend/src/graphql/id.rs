use type_safe_id::{StaticType, TypeSafeId};
use uuid::Uuid;

pub(super) trait Kind: StaticType {
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
        impl Kind for $name {}
        const _: () = assert!($name::TYPE_PREFIX_IS_VALID);
    )*};
}

pub(super) fn reading_revision(user: Uuid, manifestation: Uuid, etag: &str) -> async_graphql::ID {
    use sha2::{Digest, Sha256};

    const NAMESPACE: Uuid = uuid::uuid!("612ac3d5-e9a1-481f-ada7-1829aef70bc7");
    let digest = Sha256::new()
        .chain_update(NAMESPACE.as_bytes())
        .chain_update(user.as_bytes())
        .chain_update(manifestation.as_bytes())
        .chain_update(etag.as_bytes())
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    ReadingRevision::encode(uuid::Builder::from_custom_bytes(bytes).into_uuid())
}

kinds! {
    Author => "author",
    Genre => "genre",
    Manifestation => "book",
    MetadataRevision => "metadata_rev",
    MetadataVersion => "metadata_version",
    Mood => "mood",
    ReadingRevision => "reading_rev",
    Series => "series",
    Shelf => "shelf",
    Tag => "tag",
    User => "user",
    Work => "work",
}
