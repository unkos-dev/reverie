//! Normalised canonical-JSON + SHA-256 comparison for metadata observations.
//!
//! Enrichment uses this hash for agreement and disagreement. Exact journal
//! observation identity preserves source values independently of normalisation.

use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Compute the canonical hash of a field's value.
///
/// The hash is stable across:
/// * JSON object key ordering,
/// * canonical Unicode equivalence and surrounding string whitespace,
/// * list-field item order (creators, subjects, genres, moods, tags),
/// * contributor-role list item order.
#[must_use]
pub fn value_hash(field_name: &str, value: &Value) -> Vec<u8> {
    let normalised = normalise(field_name, value);
    let canonical = canonical_json(&normalised);
    Sha256::digest(canonical.as_bytes()).to_vec()
}

fn normalise(field: &str, v: &Value) -> Value {
    match v {
        Value::Array(items) => {
            let mut normalised: Vec<Value> = items.iter().map(normalise_item).collect();
            if matches!(field, "creators" | "subjects" | "genres" | "moods" | "tags")
                || field.starts_with("contributors.")
            {
                normalised.sort_by_key(canonical_json);
            }
            Value::Array(normalised)
        }
        _ => normalise_item(v),
    }
}

fn normalise_item(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(s.nfc().collect::<String>().trim().to_owned()),
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
    use super::value_hash;
    use serde_json::json;

    #[test]
    fn canonically_equivalent_observations_share_value_hash() {
        for (field, composed, decomposed) in [
            ("publisher", json!("Beyoncé"), json!("Beyonce\u{0301}")),
            ("title", json!("Beyoncé"), json!("Beyonce\u{0301}")),
            (
                "contributors.author",
                json!(["Beyoncé"]),
                json!(["Beyonce\u{0301}"]),
            ),
        ] {
            assert_eq!(
                value_hash(field, &composed),
                value_hash(field, &decomposed),
                "{field}",
            );
        }
    }
}
