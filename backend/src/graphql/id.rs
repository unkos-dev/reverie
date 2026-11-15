use base64ct::{Base64, Encoding};
use uuid::Uuid;

pub(super) fn encode(kind: &str, id: Uuid) -> async_graphql::ID {
    async_graphql::ID(Base64::encode_string(format!("{kind}:{id}").as_bytes()))
}

pub(super) fn decode(kind: &str, id: &async_graphql::ID) -> Option<Uuid> {
    // THREAT: malformed and wrong-type IDs have the same outcome as hidden or missing records.
    if id.as_str().len() > 80 {
        return None;
    }
    let Ok(bytes) = Base64::decode_vec(id.as_str()) else {
        return None;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return None;
    };
    let (actual_kind, value) = text.split_once(':')?;
    if actual_kind != kind {
        return None;
    }
    let Ok(value) = Uuid::parse_str(value) else {
        return None;
    };
    Some(value)
}
