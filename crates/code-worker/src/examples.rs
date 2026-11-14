//! Composed example queries: realistic SQL that combines two or more of this
//! worker's own objects (a scalar feeding another scalar, two table functions
//! joined on line spans, scalars applied to a table function's rows).
//!
//! Each `(description, sql)` pair is defined once here and referenced from the
//! schema-level `vgi.example_queries` list (`lib.rs`) and from the per-object
//! example list of the object that drives the query, so the copies cannot
//! drift. Every query is offline and deterministic (inline source literals).

/// `language_of` routes each file to the right grammar for `loc`,
/// `count_functions` and `extract_imports` — per-file metrics over a table of
/// `(path, source)` rows. Driven by `language_of`.
pub const FILE_METRICS: (&str, &str) = (
    "Per-file code metrics over a table of (path, source) rows: detect each file's language from \
     its path with language_of and pass it to loc, count_functions and extract_imports.",
    "SELECT f.path, code.main.language_of(f.path) AS lang, code.main.loc(f.src, code.main.language_of(f.path)) AS code_lines, code.main.count_functions(f.src, code.main.language_of(f.path)) AS functions, len(code.main.extract_imports(f.src, code.main.language_of(f.path))) AS imports FROM (VALUES ('src/lib.rs', 'use std::fmt;\n// helper\nfn a() {}\nfn b() {}\n'), ('app/main.py', 'import os\n\ndef run():\n    pass\n')) AS f(path, src) ORDER BY f.path",
);

/// A tiny call graph: `symbols` gives each function's line span, `ts_nodes`
/// gives every call site's line; joining one into the other attributes each
/// call to the function that makes it. Driven by `symbols`.
pub const CALL_GRAPH: (&str, &str) = (
    "Build a call graph: join the function spans from symbols with the call sites captured by \
     ts_nodes to list which function calls which.",
    "SELECT s.name AS caller, c.text AS callee, c.start_line FROM code.main.symbols('fn main() {\n    parse();\n    run();\n}\nfn run() {\n    log();\n}\n', 'rust') AS s JOIN code.main.ts_nodes('fn main() {\n    parse();\n    run();\n}\nfn run() {\n    log();\n}\n', 'rust', '(call_expression function: (identifier) @callee)') AS c ON c.start_line BETWEEN s.start_line AND s.end_line WHERE s.kind = 'function' ORDER BY c.start_line",
);

/// `ts_nodes` captures each whole function's source text; the scalars then
/// measure each one — its name (`ts_query`), lines of code (`loc`) and comment
/// count (`extract_comments`). Driven by `ts_nodes`.
pub const FUNCTION_METRICS: (&str, &str) = (
    "Per-function metrics: capture each Rust function's source with ts_nodes, then read its name \
     with ts_query and measure it with loc and extract_comments.",
    "SELECT code.main.ts_query(n.text, 'rust', '(function_item name: (identifier) @n)')[1] AS function_name, code.main.loc(n.text, 'rust') AS code_lines, len(code.main.extract_comments(n.text, 'rust')) AS comments FROM code.main.ts_nodes('fn small() {}\nfn add(a: i32, b: i32) -> i32 {\n    // sum the inputs\n    a + b\n}\n', 'rust', '(function_item) @f') AS n ORDER BY n.seq",
);
