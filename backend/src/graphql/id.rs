use type_safe_id::{StaticType, TypeSafeId};
use uuid::Uuid;

pub(super) trait Kind: StaticType {
    const NAME: &'static str;

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

kinds! {
    Author => "author",
    Genre => "genre",
    Manifestation => "book",
    MetadataVersion => "metadata_version",
    Mood => "mood",
    Publisher => "publisher",
    Series => "series",
    Shelf => "shelf",
    Tag => "tag",
    User => "user",
    Work => "work",
}
