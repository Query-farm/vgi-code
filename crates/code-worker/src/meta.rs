//! Shared helpers for the per-object discovery/description metadata that the
//! `vgi-lint` strict profile expects on **every** function and table.
//!
//! Each function/table surfaces these in its `FunctionMetadata.tags`:
//! - `vgi.title` (VGI124)        — human-friendly display name
//! - `vgi.doc_llm` (VGI112)      — Markdown narrative aimed at LLMs/agents
//! - `vgi.doc_md` (VGI113)       — Markdown narrative for human docs
//! - `vgi.keywords` (VGI126)     — search terms/synonyms
//! - `vgi.category` (VGI413)     — the schema `vgi.categories` bucket it belongs to
//!
//! Per-object `vgi.source_url` is intentionally NOT emitted: provenance is
//! advertised once on the catalog object (its `source_url` field). Repeating it
//! on every function/schema is redundant and flagged by VGI139.

/// Serialize comma-separated `keywords` into the `vgi.keywords` JSON-array form
/// the linter requires (VGI138), e.g. `"a, b"` → `["a","b"]`. Trims each term
/// and drops empties; values are JSON-escaped so commas/quotes survive.
fn keywords_json(keywords: &str) -> String {
    let items: Vec<String> = keywords
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| {
            let escaped = k.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// JSON-escape a single string value (quotes, backslashes, control chars) so it
/// can be embedded in a JSON string literal.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Build a `vgi.example_queries` tag from `(description, sql)` pairs as the
/// described-example JSON list `[{"description":…,"sql":…}]` the linter requires
/// (VGI515). The native `FunctionExample` carrier surfaced via
/// `duckdb_functions().examples` drops the per-example description, so every
/// function also publishes its examples through this tag, which preserves them.
pub fn example_queries_tag(examples: &[(&str, &str)]) -> (String, String) {
    let items: Vec<String> = examples
        .iter()
        .map(|(desc, sql)| {
            format!(
                r#"{{"description":"{}","sql":"{}"}}"#,
                json_escape(desc),
                json_escape(sql)
            )
        })
        .collect();
    (
        "vgi.example_queries".to_string(),
        format!("[{}]", items.join(",")),
    )
}

/// Build the standard per-object discovery/description tags.
///
/// `keywords` is given comma-separated and serialized to the `vgi.keywords`
/// JSON-array form (VGI138). `_relative_path` (the implementing file relative to
/// `code-worker/src`) is accepted for call-site documentation but no longer
/// emitted as a per-object `vgi.source_url`: catalog-level `source_url` is the
/// single provenance link (VGI139 — per-object copies are redundant).
pub fn object_tags(
    title: &str,
    doc_llm: &str,
    doc_md: &str,
    keywords: &str,
    category: &str,
    _relative_path: &str,
) -> Vec<(String, String)> {
    vec![
        ("vgi.title".to_string(), title.to_string()),
        ("vgi.doc_llm".to_string(), doc_llm.to_string()),
        ("vgi.doc_md".to_string(), doc_md.to_string()),
        ("vgi.keywords".to_string(), keywords_json(keywords)),
        // VGI413: name one of the schema's `vgi.categories` so the object is
        // placed in the worker's navigation/listing sections.
        ("vgi.category".to_string(), category.to_string()),
    ]
}
