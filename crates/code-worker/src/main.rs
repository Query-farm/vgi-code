//! The `code` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'code' (TYPE vgi, LOCATION '…')`). It parses source code with
//! tree-sitter and exposes its structure — symbols, imports, comments, strings,
//! line counts and arbitrary tree-sitter queries — to SQL under the catalog
//! `code`, schema `main`:
//!
//! ```sql
//! ATTACH 'code' (TYPE vgi, LOCATION './target/release/code-worker');
//! SET search_path = 'code.main';
//!
//! SELECT language_of('main.rs');                       -- → 'rust'
//! SELECT count_functions(src, 'rust') FROM files;      -- function count
//! SELECT UNNEST(extract_imports(src, 'python'));        -- one import per row
//! SELECT * FROM symbols('fn a(){}', 'rust');           -- kind/name/start/end
//! SELECT * FROM ts_nodes(src, 'go', '(function_declaration) @f');
//! ```
//!
//! Pure analysis (no Arrow) lives in `parsing.rs`; the `scalar/` and `table/`
//! modules are thin Arrow adapters over it.

mod arrow_io;
mod meta;
mod parsing;
mod scalar;
mod table;

use vgi::catalog::{CatSchema, CatalogModel};
use vgi::Worker;

/// Worker version string, surfaced by `code_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Catalog + schema metadata (description, provenance) surfaced to DuckDB and
/// the `vgi-lint` metadata-quality linter. The function objects themselves are
/// served from the registered scalars/tables; this only adds catalog/schema-level
/// comments and tags.
fn catalog_metadata(name: &str) -> CatalogModel {
    CatalogModel {
        name: name.to_string(),
        comment: Some(
            "Source-code structure for DuckDB: tree-sitter symbols, imports, comments, strings, \
             line counts and arbitrary tree-sitter queries."
                .to_string(),
        ),
        tags: vec![
            (
                "vgi.title".to_string(),
                "Source-Code Structure Analysis".to_string(),
            ),
            (
                "vgi.keywords".to_string(),
                "code, source code, tree-sitter, parsing, symbols, functions, imports, comments, \
                 string literals, lines of code, loc, language detection, syntax tree, static \
                 analysis, rust, python, javascript, typescript, go, java, c, cpp, json"
                    .to_string(),
            ),
            (
                "vgi.doc_llm".to_string(),
                "Parse source code with tree-sitter and expose its structure to SQL. Infer a \
                 file's language from its name, count physical lines / lines-of-code / function \
                 definitions, extract imports, comments and string literals as arrays, run \
                 arbitrary tree-sitter queries, and list structural symbols (functions, classes, \
                 methods, structs, enums, …). Supports rust, python, javascript, typescript, go, \
                 java, c, cpp and json. Use for code analysis, metrics and structural search in SQL."
                    .to_string(),
            ),
            (
                "vgi.doc_md".to_string(),
                "# code\n\nSource-code structure over Apache Arrow, powered by \
                 [tree-sitter](https://tree-sitter.github.io/).\n\nScalars: `language_of`, \
                 `count_lines`, `loc`, `count_functions`, `extract_imports`, `extract_comments`, \
                 `extract_strings`, `ts_query`, `code_version`. Tables: `symbols`, `ts_nodes`, \
                 `supported_languages`.\n\nSupported languages: rust, python, javascript, \
                 typescript, go, java, c, cpp, json."
                    .to_string(),
            ),
            ("vgi.author".to_string(), "Query.Farm".to_string()),
            (
                "vgi.copyright".to_string(),
                "Copyright 2026 Query Farm LLC - https://query.farm".to_string(),
            ),
            ("vgi.license".to_string(), "MIT".to_string()),
            (
                "vgi.support_contact".to_string(),
                "https://github.com/Query-farm/vgi-code/issues".to_string(),
            ),
            (
                "vgi.support_policy_url".to_string(),
                "https://github.com/Query-farm/vgi-code/blob/main/README.md".to_string(),
            ),
        ],
        source_url: Some("https://github.com/Query-farm/vgi-code".to_string()),
        schemas: vec![CatSchema {
            name: "main".to_string(),
            comment: Some(
                "Source-code structure functions: language detection, line/function counts, \
                 import/comment/string extraction, tree-sitter queries and symbol listing."
                    .to_string(),
            ),
            tags: vec![
                ("vgi.title".to_string(), "Code — main".to_string()),
                (
                    "vgi.keywords".to_string(),
                    "code, tree-sitter, language_of, count_lines, loc, count_functions, \
                     extract_imports, extract_comments, extract_strings, ts_query, symbols, \
                     ts_nodes, supported_languages, parsing, static analysis, syntax tree"
                        .to_string(),
                ),
                // VGI123 classifying tags (bare keys: domain/category/topic) for faceting.
                ("domain".to_string(), "software-engineering".to_string()),
                ("category".to_string(), "code-analysis".to_string()),
                ("topic".to_string(), "source-structure".to_string()),
                (
                    "vgi.source_url".to_string(),
                    "https://github.com/Query-farm/vgi-code/blob/main/crates/code-worker/src/main.rs"
                        .to_string(),
                ),
                (
                    "vgi.doc_llm".to_string(),
                    "Source-code structure functions: detect a file's language, count lines / \
                     lines-of-code / functions, extract imports, comments and strings as arrays, \
                     run tree-sitter queries, and list structural symbols across rust, python, \
                     javascript, typescript, go, java, c, cpp and json."
                        .to_string(),
                ),
                (
                    "vgi.doc_md".to_string(),
                    "Source-code structure functions (symbols, imports, comments, strings, line \
                     counts and tree-sitter queries) over Apache Arrow."
                        .to_string(),
                ),
                // VGI506 representative example queries for the schema.
                (
                    "vgi.example_queries".to_string(),
                    "SELECT code.main.language_of('src/main.rs');\n\
                     SELECT code.main.count_lines('fn a() {}\nfn b() {}\n');\n\
                     SELECT code.main.loc('fn a() {}\n// note\nfn b() {}\n', 'rust');\n\
                     SELECT code.main.count_functions('def a(): pass\ndef b(): pass\n', 'python');\n\
                     SELECT code.main.extract_imports('import os\nimport sys\n', 'python');\n\
                     SELECT * FROM code.main.symbols('fn a() {}\nfn b() {}\n', 'rust');\n\
                     SELECT * FROM code.main.supported_languages();"
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            tables: Vec::new(),
        }],
        ..Default::default()
    }
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    // The catalog name DuckDB sees in `ATTACH 'code' (TYPE vgi, …)`. Default to
    // `code`, but honor an explicit override so a test harness can rename it.
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "code");
    }
    let catalog_name =
        std::env::var("VGI_WORKER_CATALOG_NAME").unwrap_or_else(|_| "code".to_string());

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker.run();
}
