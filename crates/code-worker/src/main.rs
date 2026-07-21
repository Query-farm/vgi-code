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

use std::sync::Arc;

use vgi::catalog::{CatSchema, CatTable, CatalogModel};
use vgi::Worker;

/// Worker version string, published as the catalog `implementation_version`.
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
                r#"["code","source code","tree-sitter","parsing","symbols","functions","imports","comments","string literals","lines of code","loc","language detection","syntax tree","static analysis","rust","python","javascript","typescript","go","java","c","cpp","json"]"#
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
                "# Source-Code Structure Analysis for DuckDB\n\n\
                 ![tree-sitter logo](https://raw.githubusercontent.com/tree-sitter/tree-sitter/master/docs/src/assets/images/tree-sitter-small.png)\n\n\
                 Parse, search, and measure source code directly in SQL: extract symbols, \
                 imports, comments, and string literals, count physical lines and function \
                 definitions, and run arbitrary tree-sitter queries across nine programming \
                 languages — all over Apache Arrow with no language server, no checkout, and \
                 no external services.\n\n\
                 The `code` worker turns DuckDB into a lightweight code-intelligence engine for \
                 developers, data and platform engineers, and security or research teams who need \
                 to treat large codebases as queryable data. Point it at source held in any \
                 DuckDB table or file and get structured facts back as ordinary rows and columns. \
                 Because everything is just SQL, you can join code structure against commits, \
                 issues, ownership, or CVE data and compute metrics over an entire repository in \
                 a single query.\n\n\
                 Parsing is powered by [tree-sitter](https://tree-sitter.github.io/), the fast, \
                 incremental, error-tolerant parsing library used by editors and code-search tools \
                 such as Neovim, Zed, and GitHub. Each call builds a concrete syntax tree and runs \
                 curated per-language tree-sitter queries, so extraction stays accurate even on \
                 incomplete or malformed input (garbage in yields empty results, never a crash). \
                 Supported languages are rust, python, javascript, typescript, go, java, c, cpp, \
                 and json. See the official [tree-sitter documentation](https://tree-sitter.github.io/tree-sitter/) \
                 and the [tree-sitter source on GitHub](https://github.com/tree-sitter/tree-sitter) \
                 for grammar and query details.\n\n\
                 ## Key concepts\n\n\
                 Everything is exposed as ordinary SQL: scalar functions take a per-row source \
                 string (and, where parsing is needed, a `language` id) and return a value or a \
                 `VARCHAR[]` array; table functions take a constant source and return a result \
                 set you can join and aggregate. Language ids are lowercase (`rust`, `python`, \
                 `go`, …) and parsing is best-effort — malformed or truncated input yields empty \
                 results rather than an error, while an unknown language or a bad tree-sitter \
                 query is reported clearly.\n\n\
                 ## When to reach for it\n\n\
                 Use the `code` worker whenever you want to treat source as data instead of text: \
                 build codebase metrics dashboards (size, complexity, function density), audit \
                 dependencies and import graphs, mine comments and TODOs, scan string literals \
                 for secrets, URLs, or missing license headers, and run structural code search \
                 with your own tree-sitter queries. Because the answers come back as rows, you \
                 can join them against commits, ownership, issues, or CVE data and compute over \
                 an entire repository in one query."
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
            // VGI152: analyst task suite `vgi-lint simulate` runs to measure how
            // well an agent actually uses this worker. Each task is a natural-
            // language prompt with the SQL a correct answer should be equivalent to.
            (
                "vgi.agent_test_tasks".to_string(),
                r#"[
                  {"name":"detect_language","prompt":"Using this worker, what programming language does the file named 'server.py' contain? Return a single value: the detected language id.","reference_sql":"SELECT code.main.language_of('server.py')","ignore_column_names":true},
                  {"name":"count_functions","prompt":"Using this worker, how many function definitions are in this Rust source string: 'fn a() {} fn b() {} fn c() {}' ? Return the single integer count.","reference_sql":"SELECT code.main.count_functions('fn a() {} fn b() {} fn c() {}', 'rust')","ignore_column_names":true},
                  {"name":"count_lines","prompt":"Using this worker, how many physical lines are in this source string: 'fn a() {}\nfn b() {}\nfn c() {}\n' ? Return the single integer count.","reference_sql":"SELECT code.main.count_lines('fn a() {}\nfn b() {}\nfn c() {}\n')","ignore_column_names":true},
                  {"name":"extract_imports","prompt":"Using this worker, extract the import statements from this Python source string: 'import os\nimport sys' . Return one import per row as a single column.","reference_sql":"SELECT UNNEST(code.main.extract_imports('import os\nimport sys', 'python'))","ignore_column_names":true,"unordered":true},
                  {"name":"list_symbols","prompt":"Using this worker, list the names of the top-level definitions in this Rust source string: 'fn a() {} struct S {}' . Return just the names, one per row.","reference_sql":"SELECT name FROM code.main.symbols('fn a() {} struct S {}', 'rust')","ignore_column_names":true,"unordered":true},
                  {"name":"supported_languages","prompt":"Using this worker, which programming languages can it parse? Return the language ids, one per row.","reference_sql":"SELECT language FROM code.main.supported_languages","ignore_column_names":true,"unordered":true},
                  {"name":"count_loc","prompt":"Using this worker, how many lines of code (excluding blank and comment lines) are in this Rust source string: 'fn a() {}\n// note\nfn b() {}\n' ? Return the single integer.","reference_sql":"SELECT code.main.loc('fn a() {}\n// note\nfn b() {}\n', 'rust')","ignore_column_names":true},
                  {"name":"extract_comments","prompt":"Using this worker, extract the comment texts from this Rust source string: '// header\nfn a() {}\n' . Return one comment per row.","reference_sql":"SELECT UNNEST(code.main.extract_comments('// header\nfn a() {}\n', 'rust'))","ignore_column_names":true,"unordered":true},
                  {"name":"extract_strings","prompt":"Using this worker, extract the string literals from this Rust source string: 'let s = \"hello\";\n' . Return one string literal per row (each with its surrounding quotes).","reference_sql":"SELECT UNNEST(code.main.extract_strings('let s = \"hello\";\n', 'rust'))","ignore_column_names":true,"unordered":true},
                  {"name":"ts_query_names","prompt":"Using this worker, run the tree-sitter query '(function_item name: (identifier) @n)' over this Rust source string: 'fn alpha() {}\nfn beta() {}\n' and return each captured node's text, one per row.","reference_sql":"SELECT UNNEST(code.main.ts_query('fn alpha() {}\nfn beta() {}\n', 'rust', '(function_item name: (identifier) @n)'))","ignore_column_names":true,"unordered":true},
                  {"name":"ts_nodes_captures","prompt":"Using this worker, using the table-valued form, run the tree-sitter query '(function_item name: (identifier) @n)' over the Rust source 'fn alpha() {}\nfn beta() {}\n' and return the captured node texts, one per row.","reference_sql":"SELECT text FROM code.main.ts_nodes('fn alpha() {}\nfn beta() {}\n', 'rust', '(function_item name: (identifier) @n)') ORDER BY seq","ignore_column_names":true}
                ]"#
                    .to_string(),
            ),
        ],
        source_url: Some("https://github.com/Query-farm/vgi-code".to_string()),
        // VGI328: publish the build version as catalog metadata (read from
        // vgi_catalogs() without spending a query) instead of a parameterless
        // code_version() scalar.
        implementation_version: Some(version().to_string()),
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
                    r#"["code","tree-sitter","language_of","count_lines","loc","count_functions","extract_imports","extract_comments","extract_strings","ts_query","symbols","ts_nodes","supported_languages","parsing","static analysis","syntax tree"]"#
                        .to_string(),
                ),
                // VGI123 classifying tags (bare keys: domain/category/topic) for faceting.
                ("domain".to_string(), "software-engineering".to_string()),
                ("category".to_string(), "code-analysis".to_string()),
                ("topic".to_string(), "source-structure".to_string()),
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
                    "# Source-code structure\n\n\
                     The single schema of the `code` worker. It exposes source-code structure to \
                     SQL over Apache Arrow, powered by [tree-sitter](https://tree-sitter.github.io/): \
                     detect a file's language, measure it, and pull structural facts out of it \
                     without a checkout, language server, or external service.\n\n\
                     ## What lives here\n\n\
                     Scalar functions work per row and return a value or a `VARCHAR[]` you can \
                     `UNNEST`; table functions take a constant source and return joinable result \
                     sets. The work falls into a few groups — language detection and discovery, \
                     code metrics, structure and content extraction, and arbitrary tree-sitter \
                     queries.\n\n\
                     ## Languages\n\n\
                     Parsing supports rust, python, javascript, typescript, go, java, c, cpp and \
                     json. Language ids are lowercase; malformed or truncated input yields empty \
                     results rather than an error, while an unknown language or a bad query is \
                     reported clearly."
                        .to_string(),
                ),
                // VGI506/VGI515 representative example queries for the schema, as a
                // JSON list of {description, sql} so every example carries a
                // human-readable description (the native examples carrier drops it).
                (
                    "vgi.example_queries".to_string(),
                    r#"[
                      {"description":"Detect a file's language from its extension.","sql":"SELECT code.main.language_of('src/main.rs')"},
                      {"description":"Count the lines of code (excluding blank and comment lines) in a Rust source string.","sql":"SELECT code.main.loc('fn a() {}\n// note\nfn b() {}\n', 'rust')"},
                      {"description":"Count the function definitions in a Python source string.","sql":"SELECT code.main.count_functions('def a(): pass\ndef b(): pass\n', 'python')"},
                      {"description":"Extract each Python import statement, one per row.","sql":"SELECT UNNEST(code.main.extract_imports('import os\nimport sys\n', 'python')) AS import"},
                      {"description":"List the function symbols and their line spans in a Rust source string.","sql":"SELECT name, start_line, end_line FROM code.main.symbols('fn a() {}\nfn b() {}\n', 'rust') WHERE kind = 'function'"},
                      {"description":"List every language id the worker can parse.","sql":"SELECT language FROM code.main.supported_languages ORDER BY language"}
                    ]"#
                        .to_string(),
                ),
                // VGI413: ordered category registry. Every function/table declares
                // a matching `vgi.category`; these drive the worker's navigation,
                // listing sections and SEO descriptions.
                (
                    "vgi.categories".to_string(),
                    r#"[{"name":"Language Detection & Discovery","description":"Identify a source file's programming language and list the languages this worker can parse."},{"name":"Code Metrics","description":"Quantify source: physical line counts, lines-of-code, and function-definition counts."},{"name":"Structure & Extraction","description":"Pull structural facts out of source — symbols (functions, classes, structs, …), imports, comments, and string literals."},{"name":"Tree-sitter Queries","description":"Run arbitrary tree-sitter queries over source and return the matches as scalar arrays or rows."}]"#
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            // Expose the parameterless `supported_languages()` table function as a
            // regular table too, so consumers can `SELECT * FROM
            // code.main.supported_languages` (no parentheses). VGI311. The scan is
            // backed by the same `SupportedLanguages` table function; `with_function`
            // inlines and auto-registers it (deduped against `table::register`).
            tables: vec![supported_languages_table()],
        }],
        ..Default::default()
    }
}

/// The catalog `supported_languages` TABLE: a function-backed table scanning the
/// `supported_languages()` table function so it can be queried as
/// `SELECT * FROM code.main.supported_languages` (no parentheses). VGI311.
fn supported_languages_table() -> CatTable {
    let mut t = CatTable::with_function(
        "supported_languages",
        table::supported_languages_schema(),
        Arc::new(table::SupportedLanguages),
        Some(
            "The language ids this worker can parse, one per row (the accepted \
             `language` argument values for the other functions)."
                .to_string(),
        ),
        Some(parsing::SUPPORTED.len() as i64),
    );
    // `language` (column 0) uniquely identifies each row, so declare it the
    // primary key (VGI807) — this also satisfies the catalog-level
    // "no constraints declared" nudge (VGI806).
    t.primary_key = vec![vec![0]];
    t.not_null = vec![0];
    t.tags = vec![
        (
            "vgi.title".to_string(),
            "Parseable Source Languages".to_string(),
        ),
        (
            "vgi.doc_llm".to_string(),
            "The set of language ids this worker can parse, one per row in the \
             `language` column. These are the exact values accepted as the `language` \
             argument by loc, count_functions, the extract_* functions, ts_query, \
             symbols and ts_nodes. Query it to discover which languages are available."
                .to_string(),
        ),
        (
            "vgi.doc_md".to_string(),
            "The language ids this worker can parse (column: `language`)."
                .to_string(),
        ),
        (
            "vgi.keywords".to_string(),
            r#"["supported languages","list languages","available languages","language ids","discovery","grammars"]"#
                .to_string(),
        ),
        // VGI123 classifying tags for faceting.
        ("domain".to_string(), "software-engineering".to_string()),
        ("category".to_string(), "code-analysis".to_string()),
        ("topic".to_string(), "source-structure".to_string()),
        // VGI411/VGI413: place this table in one of the schema's `vgi.categories`.
        (
            "vgi.category".to_string(),
            "Language Detection & Discovery".to_string(),
        ),
        // VGI501 example queries for the table.
        (
            "vgi.example_queries".to_string(),
            r#"[{"description":"List every language id the worker can parse.","sql":"SELECT language FROM code.main.supported_languages ORDER BY language"},{"description":"Check whether a language is supported.","sql":"SELECT count(*) > 0 AS supported FROM code.main.supported_languages WHERE language = 'rust'"}]"#
                .to_string(),
        ),
    ];
    t
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
