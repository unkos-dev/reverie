//! Canonical-JSON + SHA-256 hashing for the metadata journal.
//!
//! Every `metadata_versions` row stores a stable hash of its `new_value` so that
//! repeated observations of the same logical value (from the same source) can
//! be deduplicated via the unique constraint on
//! `(manifestation_id, source, field_name, value_hash)`.
//!
//! Per-field normalisation hooks (sort list items, trim strings, coerce dates)
//! guarantee that equivalent values produce the same hash regardless of how the
//! source serialised them.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Compute the canonical hash of a field's value.
///
/// The hash is stable across:
/// * JSON object key ordering,
/// * insignificant whitespace,
/// * list-field item order (creators, subjects, genres, tags),
/// * publisher leading/trailing whitespace,
/// * `pub_date` alternate ISO representations.
pub fn value_hash(field_name: &str, value: &Value) -> Vec<u8> {
    let normalised = normalise(field_name, value);
    let canonical = canonical_json(&normalised);
    Sha256::digest(canonical.as_bytes()).to_vec()
}

fn normalise(field: &str, v: &Value) -> Value {
    match field {
        "publisher" => match v {
            Value::String(s) => Value::String(s.trim().to_string()),
            other => other.clone(),
        },
        "pub_date" => match v {
            Value::String(s) => {
                // Best-effort ISO date coercion: keep the YYYY-MM-DD prefix if
                // the string starts with one, otherwise leave as-is.
                let t = s.trim();
                if t.len() >= 10 && t.as_bytes()[4] == b'-' && t.as_bytes()[7] == b'-' {
                    Value::String(t[..10].to_string())
                } else {
                    Value::String(t.to_string())
                }
            }
            other => other.clone(),
        },
        "creators" | "subjects" | "genres" | "tags" => match v {
            Value::Array(items) => {
                let mut sorted: Vec<Value> = items.iter().map(normalise_item).collect();
                sorted.sort_by_key(canonical_json);
                Value::Array(sorted)
            }
            other => other.clone(),
        },
        _ => v.clone(),
    }
}

fn normalise_item(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(s.trim().to_string()),
        other => other.clone(),
    }
}

fn canonical_json(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, val)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical_json(val),
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_value_hashes_equal() {
        let a = value_hash("title", &json!("Hello"));
        let b = value_hash("title", &json!("Hello"));
        assert_eq!(a, b);
    }

    #[test]
    fn different_values_hash_differently() {
        let a = value_hash("title", &json!("Hello"));
        let b = value_hash("title", &json!("World"));
        assert_ne!(a, b);
    }

    #[test]
    fn list_field_order_insensitive() {
        let a = value_hash("subjects", &json!(["a", "b", "c"]));
        let b = value_hash("subjects", &json!(["c", "a", "b"]));
        assert_eq!(a, b);
    }

    #[test]
    fn non_list_field_order_sensitive() {
        let a = value_hash("other", &json!(["a", "b"]));
        let b = value_hash("other", &json!(["b", "a"]));
        assert_ne!(a, b);
    }

    #[test]
    fn publisher_trimmed() {
        let a = value_hash("publisher", &json!("Acme Press"));
        let b = value_hash("publisher", &json!("  Acme Press  "));
        assert_eq!(a, b);
    }

    #[test]
    fn pub_date_normalised() {
        let a = value_hash("pub_date", &json!("2024-01-15"));
        let b = value_hash("pub_date", &json!("2024-01-15T00:00:00Z"));
        assert_eq!(a, b);
    }

    #[test]
    fn object_key_order_irrelevant() {
        let a = value_hash("series", &json!({"name": "Foo", "position": 1}));
        let b = value_hash("series", &json!({"position": 1, "name": "Foo"}));
        assert_eq!(a, b);
    }
}
