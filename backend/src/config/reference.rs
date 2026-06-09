//! Renders the operator-facing configuration reference (Starlight MDX) from the
//! declarative config schema plus [`ENV_MAP`].
//!
//! The committed artifact (`docs/src/content/docs/reference/configuration.mdx`)
//! is drift-gated by `tests/gen_config_ref.rs`: editing a config field's
//! `///` doc, default, or range without regenerating the page fails CI. The
//! schema (`schemars::schema_for!(Config)` — the same value the CI-gated
//! `config.schema.json` is checked against) is the single source for each
//! field's type/default/description; [`ENV_MAP`] supplies the operator-facing
//! variable names, and [`REQUIRED_ENV_VARS`] supplies required-ness (the
//! schema's own `required` array is empty under container `#[serde(default)]`).
//!
//! Secret fields render their schema default, which is `""`/`null` for every
//! secret (guarded by the `config_schema_has_no_secret_default_values` test),
//! so no secret value can leak into the reference.

use anyhow::{Context as _, anyhow};
use serde_json::Value;

use super::REQUIRED_ENV_VARS;
use super::provider::ENV_MAP;

/// Render the full Configuration reference page (frontmatter + table).
///
/// # Errors
///
/// Returns an error if the schema cannot be serialized, or if an [`ENV_MAP`]
/// entry names a dotted field path that does not resolve to a schema property
/// (a fail-loud signal that the map and the structs have diverged).
pub fn reference_markdown() -> anyhow::Result<String> {
    let schema = serde_json::to_value(schemars::schema_for!(super::Config))
        .context("serialize config schema to JSON value")?;
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);

    let mut rows: Vec<String> = Vec::new();
    for (var, path) in ENV_MAP {
        let node = node_for_path(&schema, &defs, path)
            .ok_or_else(|| anyhow!("ENV_MAP path `{path}` (var `{var}`) has no schema property"))?;
        let ty = type_label(node, &defs);
        let required = required_label(var);
        let default = default_label(node);
        let description = describe(node);
        rows.push(format!(
            "| `{var}` | {ty} | {required} | {default} | {description} |"
        ));
    }

    Ok(format!(
        "{FRONTMATTER}{INTRO}\n| Variable | Type | Required | Default | Description |\n| --- | --- | --- | --- | --- |\n{}\n",
        rows.join("\n")
    ))
}

const FRONTMATTER: &str = "---\ntitle: Configuration\ndescription: Environment variables that configure a Reverie instance.\n---\n\n";

const INTRO: &str = "import { Aside } from \"@astrojs/starlight/components\";\n\n<Aside type=\"caution\" title=\"Generated file\">\nThis page is generated from the backend configuration schema. Do not edit it by\nhand — regenerate with `REGEN=1 cargo test --test gen_config_ref` and commit the\nresult. The drift test fails CI if it is stale.\n</Aside>\n\nReverie is configured entirely through environment variables, read once at\nstartup. Secret-bearing variables are listed by name only; their values never\nappear here. A required variable left unset refuses startup with a clear error.\n";

/// Resolve a `$ref` node to its `$defs` target; pass any other node through.
fn resolve<'a>(node: &'a Value, defs: &'a Value) -> &'a Value {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let name = reference.trim_start_matches("#/$defs/");
        if let Some(target) = defs.get(name) {
            return target;
        }
    }
    node
}

/// Walk a dotted field path (`enrichment.concurrency`) to its leaf schema node,
/// descending through `$ref`-linked sub-struct definitions at each step.
fn node_for_path<'a>(schema: &'a Value, defs: &'a Value, path: &str) -> Option<&'a Value> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut node = schema.get("properties")?.get(first)?;
    for segment in segments {
        node = resolve(node, defs).get("properties")?.get(segment)?;
    }
    Some(node)
}

/// A human-readable type label for the field, wrapped in code formatting.
fn type_label(node: &Value, defs: &Value) -> String {
    let resolved = resolve(node, defs);

    // Enum: a `oneOf` of string `const`s (e.g. CleanupMode).
    if let Some(variants) = resolved.get("oneOf").and_then(Value::as_array) {
        let consts: Vec<String> = variants
            .iter()
            .filter_map(|v| v.get("const").and_then(Value::as_str))
            .map(|c| format!("`{c}`"))
            .collect();
        if !consts.is_empty() {
            return consts.join(" \\| ");
        }
    }

    match resolved.get("type") {
        // Array form `["string", "null"]` — drop null, label the real type.
        Some(Value::Array(types)) => {
            let base = types
                .iter()
                .filter_map(Value::as_str)
                .find(|t| *t != "null")
                .unwrap_or("string");
            scalar_label(base, resolved)
        }
        Some(Value::String(t)) if t == "array" => {
            let inner = resolved
                .get("items")
                .map_or_else(|| "`string`".to_string(), |items| type_label(items, defs));
            format!("list of {inner}")
        }
        Some(Value::String(t)) => scalar_label(t, resolved),
        _ => "`object`".to_string(),
    }
}

/// Label a scalar JSON-schema type, refining by `format` where it adds operator
/// meaning (`uri` → URL; integer formats collapse to `integer`).
fn scalar_label(base: &str, node: &Value) -> String {
    let format = node.get("format").and_then(Value::as_str);
    match (base, format) {
        (_, Some("uri")) => "`URL`".to_string(),
        ("integer", _) => "`integer`".to_string(),
        (other, _) => format!("`{other}`"),
    }
}

/// `Yes` for unconditionally-required vars, `Conditional` for the migration DSN,
/// `No` otherwise.
fn required_label(var: &str) -> &'static str {
    if REQUIRED_ENV_VARS.contains(&var) {
        "Yes"
    } else if var == "DATABASE_URL_MIGRATION" {
        "Conditional"
    } else {
        "No"
    }
}

/// Render the field's schema default for the table, code-formatted. An empty
/// string or `null` (every secret, and genuinely-empty defaults) renders blank.
fn default_label(node: &Value) -> String {
    match node.get("default") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) if s.is_empty() => String::new(),
        Some(Value::String(s)) => format!("`{}`", escape_cell(s)),
        Some(Value::Bool(b)) => format!("`{b}`"),
        Some(Value::Number(n)) => format!("`{n}`"),
        Some(Value::Array(items)) => {
            let joined = items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            if joined.is_empty() {
                String::new()
            } else {
                format!("`{joined}`")
            }
        }
        Some(other) => format!("`{}`", escape_cell(&other.to_string())),
    }
}

/// The field's `description`, collapsed to a single table-cell-safe line with
/// rustdoc intra-doc link markup reduced to plain code spans (the `///` docs
/// are dual-purpose — rustdoc and this operator reference — so `` [`X`] ``
/// renders as `` `X` `` rather than leaking link brackets into the page).
fn describe(node: &Value) -> String {
    let Some(raw) = node.get("description").and_then(Value::as_str) else {
        return String::new();
    };
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let delinked = collapsed.replace("[`", "`").replace("`]", "`");
    escape_cell(&delinked)
}

/// Escape characters that would break a Markdown table cell.
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}
