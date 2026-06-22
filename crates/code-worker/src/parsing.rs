//! Pure source-code analysis (no Arrow): the tree-sitter language registry plus
//! every structural extraction the worker exposes — symbols, imports, comments,
//! strings, line counts, and arbitrary tree-sitter queries.
//!
//! Everything here is plain Rust over `&str` source and a `Language` enum; the
//! `scalar/` and `table/` modules are thin Arrow adapters over these functions.
//!
//! ## Robustness
//! tree-sitter is an *error-recovering* parser: malformed source never makes it
//! crash — it produces a tree with `ERROR`/`MISSING` nodes and we extract what we
//! can. We additionally bound the input size ([`MAX_SOURCE_BYTES`]) and treat the
//! source as text (never executed), so the attack surface is small. Unknown
//! languages are a *caller* error and surface a clear message; unparseable source
//! is best-effort and yields empty results, never an error.

use std::fmt;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

/// Upper bound on source size we will parse, guarding against pathological input.
/// 16 MiB is far larger than any realistic single source file.
pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

/// A supported source language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    Json,
}

/// The canonical language identifier strings, in a stable order. Drives
/// `supported_languages()` and the README table.
pub const SUPPORTED: &[&str] = &[
    "rust",
    "python",
    "javascript",
    "typescript",
    "go",
    "java",
    "c",
    "cpp",
    "json",
];

impl Lang {
    /// Parse a language identifier (case-insensitive, common aliases accepted).
    /// Returns `None` for an unknown language.
    pub fn from_name(name: &str) -> Option<Lang> {
        match name.trim().to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Lang::Rust),
            "python" | "py" => Some(Lang::Python),
            "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
            "typescript" | "ts" => Some(Lang::TypeScript),
            "go" | "golang" => Some(Lang::Go),
            "java" => Some(Lang::Java),
            "c" | "h" => Some(Lang::C),
            "cpp" | "c++" | "cxx" | "cc" | "hpp" | "hxx" => Some(Lang::Cpp),
            "json" => Some(Lang::Json),
            _ => None,
        }
    }

    /// The canonical identifier string for this language.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::Json => "json",
        }
    }

    /// The tree-sitter grammar for this language.
    pub fn grammar(self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::C => tree_sitter_c::LANGUAGE.into(),
            Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Lang::Json => tree_sitter_json::LANGUAGE.into(),
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An error from a language-analysis call. The only *hard* error is an unknown
/// language or oversized input — parsing itself is always best-effort.
#[derive(Debug)]
pub enum AnalyzeError {
    UnknownLanguage(String),
    TooLarge(usize),
    Parse(String),
}

impl fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalyzeError::UnknownLanguage(s) => write!(
                f,
                "unknown language '{s}'; supported: {}",
                SUPPORTED.join(", ")
            ),
            AnalyzeError::TooLarge(n) => {
                write!(
                    f,
                    "source is {n} bytes, exceeds the {MAX_SOURCE_BYTES}-byte limit"
                )
            }
            AnalyzeError::Parse(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for AnalyzeError {}

/// Resolve a language name or return [`AnalyzeError::UnknownLanguage`].
pub fn resolve(language: &str) -> Result<Lang, AnalyzeError> {
    Lang::from_name(language).ok_or_else(|| AnalyzeError::UnknownLanguage(language.to_string()))
}

/// Infer a language from a filename's extension. Returns `None` if unknown.
pub fn language_of_filename(filename: &str) -> Option<&'static str> {
    // Take the substring after the last '.', case-insensitively.
    let ext = filename.rsplit('.').next()?;
    if ext == filename {
        // No '.' at all → no extension.
        return None;
    }
    let lang = match ext.to_ascii_lowercase().as_str() {
        "rs" => Lang::Rust,
        "py" | "pyi" => Lang::Python,
        "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
        "ts" | "tsx" => Lang::TypeScript,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "c" | "h" => Lang::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Lang::Cpp,
        "json" => Lang::Json,
        _ => return None,
    };
    Some(lang.name())
}

/// Parse `source` with the grammar for `lang`. tree-sitter recovers from errors,
/// so this only returns `None` if the parser cannot be configured at all.
fn parse(lang: Lang, source: &str) -> Result<Tree, AnalyzeError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(AnalyzeError::TooLarge(source.len()));
    }
    let mut parser = Parser::new();
    parser
        .set_language(&lang.grammar())
        .map_err(|e| AnalyzeError::Parse(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| AnalyzeError::Parse("parser returned no tree".to_string()))
}

/// Total number of lines in `source` (a trailing newline does not add a line).
/// Empty source is 0 lines.
pub fn count_lines(source: &str) -> i32 {
    if source.is_empty() {
        return 0;
    }
    let mut n = source.lines().count();
    // `str::lines` already ignores a single trailing newline; nothing else to do.
    if n == 0 {
        n = 1;
    }
    n as i32
}

/// Lines of code: non-blank lines that are not *entirely* a comment. Uses the
/// language's comment node ranges to decide which lines are comment-only.
pub fn loc(lang: Lang, source: &str) -> Result<i32, AnalyzeError> {
    if source.is_empty() {
        return Ok(0);
    }
    let tree = parse(lang, source)?;
    // Mark every line that a comment node touches.
    let total_lines = count_lines(source) as usize;
    let mut comment_line: Vec<bool> = vec![false; total_lines + 1];
    let mut has_noncomment: Vec<bool> = vec![false; total_lines + 1];

    walk_comment_spans(tree.root_node(), &mut |start, end, is_comment| {
        for line in start..=end {
            if line < comment_line.len() {
                if is_comment {
                    comment_line[line] = true;
                } else {
                    has_noncomment[line] = true;
                }
            }
        }
    });

    let mut count = 0i32;
    for (i, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue; // blank
        }
        // A line counts as code unless it is comment-touched and has no
        // non-comment token on it.
        if comment_line[i] && !has_noncomment[i] {
            continue;
        }
        count += 1;
    }
    Ok(count)
}

/// Walk the tree, invoking `f(start_line, end_line, is_comment)` for leaf-ish
/// nodes so the caller can classify each line as comment-only or code-bearing.
fn walk_comment_spans(node: Node, f: &mut impl FnMut(usize, usize, bool)) {
    let mut cursor = node.walk();
    let kind = node.kind();
    let is_comment = kind.contains("comment");
    if node.child_count() == 0 || is_comment {
        // A leaf, or a comment subtree we treat atomically.
        let start = node.start_position().row;
        let end = node.end_position().row;
        f(start, end, is_comment);
        if is_comment {
            return; // do not descend into comment internals
        }
    }
    for child in node.children(&mut cursor) {
        walk_comment_spans(child, f);
    }
}

/// A structural symbol (function, class, method, struct, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub kind: String,
    pub name: String,
    /// 1-based start line.
    pub start_line: i32,
    /// 1-based end line.
    pub end_line: i32,
}

/// A captured tree-sitter query node.
#[derive(Debug, Clone)]
pub struct Capture {
    pub capture: String,
    pub text: String,
    /// 1-based start line.
    pub start_line: i32,
    /// 1-based end line.
    pub end_line: i32,
}

/// Run an arbitrary tree-sitter `query` against `source`, returning every
/// captured node in document order. A malformed query is a caller error.
pub fn run_query(lang: Lang, source: &str, query: &str) -> Result<Vec<Capture>, AnalyzeError> {
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let grammar = lang.grammar();
    let tree = parse(lang, source)?;
    let q = Query::new(&grammar, query)
        .map_err(|e| AnalyzeError::Parse(format!("invalid tree-sitter query: {e}")))?;
    let names = q.capture_names();
    let mut cursor = QueryCursor::new();
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut matches = cursor.matches(&q, tree.root_node(), bytes);
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;
            let text = node.utf8_text(bytes).unwrap_or("").to_string();
            out.push(Capture {
                capture: names
                    .get(cap.index as usize)
                    .copied()
                    .unwrap_or("")
                    .to_string(),
                text,
                start_line: node.start_position().row as i32 + 1,
                end_line: node.end_position().row as i32 + 1,
            });
        }
    }
    Ok(out)
}

/// Just the captured node texts of a query, for the scalar `ts_query`.
pub fn query_texts(lang: Lang, source: &str, query: &str) -> Result<Vec<String>, AnalyzeError> {
    Ok(run_query(lang, source, query)?
        .into_iter()
        .map(|c| c.text)
        .collect())
}

/// Run a query that is allowed to be *empty* for languages we don't have one for,
/// mapping every captured node to its text (deduplication left to caller).
fn capture_texts(lang: Lang, source: &str, query: &str) -> Vec<String> {
    if query.is_empty() {
        return Vec::new();
    }
    run_query(lang, source, query)
        .map(|caps| caps.into_iter().map(|c| c.text).collect())
        .unwrap_or_default()
}

/// Extract import / use / require statements as their source text.
pub fn extract_imports(lang: Lang, source: &str) -> Vec<String> {
    capture_texts(lang, source, imports_query(lang))
}

/// Extract comment texts (the raw comment node source).
pub fn extract_comments(lang: Lang, source: &str) -> Vec<String> {
    capture_texts(lang, source, comments_query(lang))
}

/// Extract string-literal texts (including quotes, as written in source).
pub fn extract_strings(lang: Lang, source: &str) -> Vec<String> {
    capture_texts(lang, source, strings_query(lang))
}

/// Count function-like definitions. Built on the symbols query, filtered to the
/// function/method kinds.
pub fn count_functions(lang: Lang, source: &str) -> i32 {
    symbols(lang, source)
        .unwrap_or_default()
        .iter()
        .filter(|s| s.kind == "function" || s.kind == "method")
        .count() as i32
}

/// Extract structural symbols (functions, methods, classes, structs, enums,
/// interfaces, traits) using the per-language symbols query.
pub fn symbols(lang: Lang, source: &str) -> Result<Vec<Symbol>, AnalyzeError> {
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let caps = run_query(lang, source, symbols_query(lang))?;
    // The symbols query captures `@<kind>.def` (the whole definition node, for the
    // line span) paired with `@<kind>.name` (the identifier). We pair them by
    // matching start position: a name capture immediately follows its def capture
    // within the same match, so we walk the flat capture list and pair a `.name`
    // with the most recent `.def` of the same kind.
    let mut out: Vec<Symbol> = Vec::new();
    let mut pending: Option<(String, i32, i32)> = None; // (kind, start, end)
    for c in caps {
        if let Some(kind) = c.capture.strip_suffix(".def") {
            pending = Some((kind.to_string(), c.start_line, c.end_line));
        } else if let Some(kind) = c.capture.strip_suffix(".name") {
            if let Some((pkind, start, end)) = pending.take() {
                if pkind == kind {
                    out.push(Symbol {
                        kind: pkind,
                        name: c.text,
                        start_line: start,
                        end_line: end,
                    });
                    continue;
                }
            }
            // A name without a matching def (shouldn't happen) — emit with the
            // name's own span.
            out.push(Symbol {
                kind: kind.to_string(),
                name: c.text,
                start_line: c.start_line,
                end_line: c.end_line,
            });
        }
    }
    out.sort_by_key(|s| (s.start_line, s.end_line));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Per-language tree-sitter queries.
//
// Each `@<kind>.def` capture is the whole definition node (its line span is the
// symbol's span); the sibling `@<kind>.name` capture is the identifier. Kinds
// are normalized to: function, method, class, struct, enum, interface, trait.
// ---------------------------------------------------------------------------

fn symbols_query(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => {
            r#"
            (function_item name: (identifier) @function.name) @function.def
            (struct_item name: (type_identifier) @struct.name) @struct.def
            (enum_item name: (type_identifier) @enum.name) @enum.def
            (trait_item name: (type_identifier) @trait.name) @trait.def
            (impl_item) @impl.def
            "#
        }
        Lang::Python => {
            r#"
            (function_definition name: (identifier) @function.name) @function.def
            (class_definition name: (identifier) @class.name) @class.def
            "#
        }
        Lang::JavaScript => {
            r#"
            (function_declaration name: (identifier) @function.name) @function.def
            (method_definition name: (property_identifier) @method.name) @method.def
            (class_declaration name: (identifier) @class.name) @class.def
            "#
        }
        Lang::TypeScript => {
            r#"
            (function_declaration name: (identifier) @function.name) @function.def
            (method_definition name: (property_identifier) @method.name) @method.def
            (class_declaration name: (type_identifier) @class.name) @class.def
            (interface_declaration name: (type_identifier) @interface.name) @interface.def
            (enum_declaration name: (identifier) @enum.name) @enum.def
            "#
        }
        Lang::Go => {
            r#"
            (function_declaration name: (identifier) @function.name) @function.def
            (method_declaration name: (field_identifier) @method.name) @method.def
            (type_declaration (type_spec name: (type_identifier) @struct.name (struct_type))) @struct.def
            (type_declaration (type_spec name: (type_identifier) @interface.name (interface_type))) @interface.def
            "#
        }
        Lang::Java => {
            r#"
            (method_declaration name: (identifier) @method.name) @method.def
            (class_declaration name: (identifier) @class.name) @class.def
            (interface_declaration name: (identifier) @interface.name) @interface.def
            (enum_declaration name: (identifier) @enum.name) @enum.def
            "#
        }
        Lang::C => {
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @function.name)) @function.def
            (struct_specifier name: (type_identifier) @struct.name) @struct.def
            (enum_specifier name: (type_identifier) @enum.name) @enum.def
            "#
        }
        Lang::Cpp => {
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @function.name)) @function.def
            (class_specifier name: (type_identifier) @class.name) @class.def
            (struct_specifier name: (type_identifier) @struct.name) @struct.def
            (enum_specifier name: (type_identifier) @enum.name) @enum.def
            "#
        }
        Lang::Json => "",
    }
}

fn imports_query(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => "(use_declaration) @import",
        Lang::Python => {
            r#"
            (import_statement) @import
            (import_from_statement) @import
            "#
        }
        Lang::JavaScript | Lang::TypeScript => {
            r#"
            (import_statement) @import
            "#
        }
        Lang::Go => {
            r#"
            (import_declaration) @import
            "#
        }
        Lang::Java => "(import_declaration) @import",
        Lang::C | Lang::Cpp => "(preproc_include) @import",
        Lang::Json => "",
    }
}

fn comments_query(lang: Lang) -> &'static str {
    match lang {
        // tree-sitter-rust / -java split comments into line_comment + block_comment
        // (there is no unified `comment` node).
        Lang::Rust | Lang::Java => {
            r#"
            (line_comment) @comment
            (block_comment) @comment
            "#
        }
        // JSON has no comment node in the grammar; nothing to match.
        Lang::Json => "",
        // Python, JS/TS, Go, C and C++ all expose a single `comment` node.
        _ => "(comment) @comment",
    }
}

fn strings_query(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => {
            r#"
            (string_literal) @string
            (raw_string_literal) @string
            "#
        }
        Lang::Python => {
            r#"
            (string) @string
            "#
        }
        Lang::JavaScript | Lang::TypeScript => {
            r#"
            (string) @string
            (template_string) @string
            "#
        }
        Lang::Go => {
            r#"
            (interpreted_string_literal) @string
            (raw_string_literal) @string
            "#
        }
        Lang::Java => {
            r#"
            (string_literal) @string
            "#
        }
        Lang::C | Lang::Cpp => "(string_literal) @string",
        Lang::Json => "(string) @string",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_SRC: &str = r#"// a leading comment
use std::collections::HashMap;
use std::fmt;

/// Doc comment
fn add(a: i32, b: i32) -> i32 {
    a + b // inline
}

struct Point {
    x: i32,
    y: i32,
}

enum Color {
    Red,
    Green,
}
"#;

    const PY_SRC: &str = r#"import os
from sys import argv

# a comment
def greet(name):
    return "hello " + name

class Greeter:
    def __init__(self):
        self.x = 1
"#;

    const JS_SRC: &str = r#"import { foo } from "bar";

// comment
function add(a, b) {
  return a + b;
}

class Widget {
  render() {
    return `tpl`;
  }
}
"#;

    const GO_SRC: &str = "package main\n\nimport \"fmt\"\n\n// doc\nfunc Add(a int, b int) int {\n\treturn a + b\n}\n\ntype Point struct {\n\tX int\n}\n";

    #[test]
    fn language_resolution_and_inference() {
        assert_eq!(Lang::from_name("RUST"), Some(Lang::Rust));
        assert_eq!(Lang::from_name("py"), Some(Lang::Python));
        assert_eq!(Lang::from_name("nope"), None);
        assert_eq!(language_of_filename("x.py"), Some("python"));
        assert_eq!(language_of_filename("a/b/main.rs"), Some("rust"));
        assert_eq!(language_of_filename("App.tsx"), Some("typescript"));
        assert_eq!(language_of_filename("Makefile"), None);
        assert_eq!(language_of_filename("noext"), None);
        assert_eq!(language_of_filename("x.unknownext"), None);
    }

    #[test]
    fn rust_symbols_names_and_lines() {
        let s = symbols(Lang::Rust, RUST_SRC).unwrap();
        let names: Vec<_> = s
            .iter()
            .map(|x| (x.kind.as_str(), x.name.as_str()))
            .collect();
        assert!(names.contains(&("function", "add")));
        assert!(names.contains(&("struct", "Point")));
        assert!(names.contains(&("enum", "Color")));
        let add = s.iter().find(|x| x.name == "add").unwrap();
        assert_eq!(add.start_line, 6, "fn add starts on line 6 (1-based)");
    }

    #[test]
    fn python_symbols_and_functions() {
        let s = symbols(Lang::Python, PY_SRC).unwrap();
        assert!(s.iter().any(|x| x.kind == "class" && x.name == "Greeter"));
        assert!(s.iter().any(|x| x.kind == "function" && x.name == "greet"));
        // greet + __init__ are both function_definition nodes.
        assert_eq!(count_functions(Lang::Python, PY_SRC), 2);
    }

    #[test]
    fn js_symbols_and_method() {
        let s = symbols(Lang::JavaScript, JS_SRC).unwrap();
        assert!(s.iter().any(|x| x.kind == "function" && x.name == "add"));
        assert!(s.iter().any(|x| x.kind == "class" && x.name == "Widget"));
        assert!(s.iter().any(|x| x.kind == "method" && x.name == "render"));
    }

    #[test]
    fn go_symbols() {
        let s = symbols(Lang::Go, GO_SRC).unwrap();
        assert!(s.iter().any(|x| x.kind == "function" && x.name == "Add"));
        assert!(s.iter().any(|x| x.kind == "struct" && x.name == "Point"));
    }

    #[test]
    fn imports_extracted() {
        let imps = extract_imports(Lang::Rust, RUST_SRC);
        assert_eq!(imps.len(), 2);
        assert!(imps[0].contains("HashMap"));
        let py = extract_imports(Lang::Python, PY_SRC);
        assert_eq!(py.len(), 2);
        let js = extract_imports(Lang::JavaScript, JS_SRC);
        assert_eq!(js.len(), 1);
        assert!(js[0].contains("bar"));
        let go = extract_imports(Lang::Go, GO_SRC);
        assert_eq!(go.len(), 1);
    }

    #[test]
    fn comments_and_strings() {
        let comments = extract_comments(Lang::Rust, RUST_SRC);
        assert!(comments.iter().any(|c| c.contains("leading comment")));
        assert!(comments.iter().any(|c| c.contains("inline")));
        let strings = extract_strings(Lang::Python, PY_SRC);
        assert!(strings.iter().any(|s| s.contains("hello")));
    }

    #[test]
    fn ts_query_capture() {
        // Capture every function name in the Rust source.
        let texts = query_texts(
            Lang::Rust,
            RUST_SRC,
            "(function_item name: (identifier) @n)",
        )
        .unwrap();
        assert_eq!(texts, vec!["add"]);
    }

    #[test]
    fn loc_vs_count_lines() {
        // count_lines counts every physical line; loc drops blank + comment-only.
        let total = count_lines(RUST_SRC);
        let code = loc(Lang::Rust, RUST_SRC).unwrap();
        assert!(total > code, "loc ({code}) must be < total lines ({total})");
        // The two pure comment lines + blank lines are excluded.
        assert!(code > 0);
    }

    #[test]
    fn empty_and_garbage_are_graceful() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(loc(Lang::Rust, "").unwrap(), 0);
        assert!(symbols(Lang::Rust, "").unwrap().is_empty());
        assert!(extract_imports(Lang::Rust, "").is_empty());
        // Garbage source: tree-sitter recovers, we just get few/no symbols, no panic.
        let garbage = "fn fn fn ;;; {{{ not valid rust @@@@";
        let _ = symbols(Lang::Rust, garbage).unwrap();
        let _ = extract_imports(Lang::Rust, garbage);
        assert_eq!(count_functions(Lang::Rust, garbage), 0);
    }

    #[test]
    fn bad_query_is_an_error() {
        assert!(run_query(Lang::Rust, RUST_SRC, "(this is not valid").is_err());
    }

    #[test]
    fn unknown_language_errors() {
        assert!(matches!(
            resolve("cobol"),
            Err(AnalyzeError::UnknownLanguage(_))
        ));
    }

    #[test]
    fn all_supported_languages_parse() {
        // Every advertised language must have a working grammar.
        for name in SUPPORTED {
            let lang = Lang::from_name(name).unwrap();
            // A trivial source parses without panic.
            let _ = parse(lang, "x").unwrap();
        }
    }
}
