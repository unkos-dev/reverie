use base64ct::{Base64, Encoding};
use juniper::ID;
use uuid::Uuid;

pub(super) fn encode(kind: &str, id: Uuid) -> ID {
    ID::new(Base64::encode_string(format!("{kind}:{id}").as_bytes()))
}

pub(super) fn decode(kind: &str, id: &ID) -> Option<Uuid> {
    // THREAT: malformed and wrong-type IDs have the same outcome as hidden or missing records.
    let id: &str = id;
    if id.len() > 80 {
        return None;
    }
    let Ok(bytes) = Base64::decode_vec(id) else {
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
