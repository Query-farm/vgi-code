//! Shared helpers for the per-object discovery/description metadata that the
//! `vgi-lint` strict profile expects on **every** function and table.
//!
//! Each function/table surfaces these in its `FunctionMetadata.tags`:
//! - `vgi.title` (VGI124)        — human-friendly display name
//! - `vgi.doc_llm` (VGI112)      — Markdown narrative aimed at LLMs/agents
//! - `vgi.doc_md` (VGI113)       — Markdown narrative for human docs
//! - `vgi.keywords` (VGI126)     — search terms/synonyms
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
    _relative_path: &str,
) -> Vec<(String, String)> {
    vec![
        ("vgi.title".to_string(), title.to_string()),
        ("vgi.doc_llm".to_string(), doc_llm.to_string()),
        ("vgi.doc_md".to_string(), doc_md.to_string()),
        ("vgi.keywords".to_string(), keywords_json(keywords)),
    ]
}
