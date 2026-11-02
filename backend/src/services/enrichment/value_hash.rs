//! Canonical-JSON + SHA-256 hashing for the metadata journal.
//!
//! Every `metadata_versions` row stores a stable hash of its `new_value` so that
//! repeated observations of the same logical value (from the same source) can
//! be deduplicated via the unique constraint on
//! `(manifestation_id, source, field_name, value_hash)`.
//!
//! Per-field normalisation hooks (sort list items, trim strings, preserve publication precision)
//! guarantee that equivalent values produce the same hash regardless of how the
//! source serialised them.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Compute the canonical hash of a field's value.
///
/// The hash is stable across:
/// * JSON object key ordering,
/// * insignificant whitespace,
/// * list-field item order (creators, subjects, genres, moods, tags),
/// * publisher leading/trailing whitespace,
/// * `pub_date` surrounding whitespace for conforming values.
#[must_use]
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
        // External identifiers are single canonical strings; trim so a manual
        // entry with stray whitespace dedups against the same source value.
        f if f.starts_with("identifiers.") => match v {
            Value::String(s) => Value::String(s.trim().to_string()),
            other => other.clone(),
        },
        "pub_date" => match v {
            Value::String(value) => {
                match crate::services::metadata::publication_date::normalise(value) {
                    Ok(Some(value)) => Value::String(value),
                    Ok(None) | Err(_) => v.clone(),
                }
            }
            other => other.clone(),
        },
        f if f == "creators"
            || f == "subjects"
            || f == "genres"
            || f == "moods"
            || f == "tags"
            || f.starts_with("contributors.") =>
        {
            match v {
                Value::Array(items) => {
                    let mut sorted: Vec<Value> = items.iter().map(normalise_item).collect();
                    sorted.sort_by_key(canonical_json);
                    Value::Array(sorted)
                }
                other => other.clone(),
            }
        }
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
