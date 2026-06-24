//! Shared helpers for the per-object discovery/description metadata that the
//! `vgi-lint` strict profile expects on **every** function and table.
//!
//! Each function/table surfaces these in its `FunctionMetadata.tags`:
//! - `vgi.title` (VGI124)        — human-friendly display name
//! - `vgi.description_llm` (VGI112) — concise prose aimed at LLMs
//! - `vgi.description_md` (VGI113)  — short Markdown description
//! - `vgi.keywords` (VGI126)        — comma-separated search terms/synonyms
//! - `vgi.source_url` (VGI128)      — link to the implementing source file
//!
//! `source_url(file)` builds the canonical GitHub blob URL for a source file so
//! every object points at exactly where it is implemented.

/// Base GitHub blob URL for source files in this repo (pinned to `main`).
const SOURCE_BASE: &str = "https://github.com/Query-farm/vgi-code/blob/main/crates/code-worker/src";

/// Build the implementation `vgi.source_url` for a file under `code-worker/src`,
/// e.g. `source_url("scalar/counts.rs")`.
pub fn source_url(relative_path: &str) -> String {
    format!("{SOURCE_BASE}/{relative_path}")
}

/// Build the five standard per-object discovery/description tags.
///
/// `relative_path` is the implementing file relative to `code-worker/src`.
pub fn object_tags(
    title: &str,
    description_llm: &str,
    description_md: &str,
    keywords: &str,
    relative_path: &str,
) -> Vec<(String, String)> {
    vec![
        ("vgi.title".to_string(), title.to_string()),
        (
            "vgi.description_llm".to_string(),
            description_llm.to_string(),
        ),
        ("vgi.description_md".to_string(), description_md.to_string()),
        ("vgi.keywords".to_string(), keywords.to_string()),
        ("vgi.source_url".to_string(), source_url(relative_path)),
    ]
}
