//! Integration tests over the pure analysis surface across several languages.
//! These exercise the same `parsing` functions the Arrow adapters call, but as a
//! standalone consumer of the crate's library-style API (re-declared via the bin
//! crate is not possible, so we re-include the module under test).
//!
//! tree-sitter parsing is the load-bearing behavior; the Arrow boundary is unit-
//! tested in-process inside each scalar module (`cargo test --workspace`).

// The worker is a binary crate, so to test `parsing` as an integration test we
// include the source module directly. It has no dependencies on the Arrow layer.
#[path = "../src/parsing.rs"]
mod parsing;

use parsing::{
    count_functions, count_lines, extract_comments, extract_imports, extract_strings,
    language_of_filename, loc, query_texts, resolve, symbols, AnalyzeError, Lang, SUPPORTED,
};

const RUST: &str = r#"use std::collections::HashMap;
use std::fmt::Debug;

/// A doc comment.
fn compute(n: i32) -> i32 {
    let s = "literal";
    n * 2
}

struct Widget {
    id: u32,
}

enum State {
    On,
    Off,
}
"#;

const PYTHON: &str = r#"import os
from typing import List

# top-level comment
def transform(items):
    return [x for x in items]

class Pipeline:
    def run(self):
        return "done"
"#;

const JS: &str = r#"import { render } from "./ui";

// entry
function boot() {
  return 42;
}

class App {
  start() {
    return `running`;
  }
}
"#;

const GO: &str = "package main\n\nimport (\n\t\"fmt\"\n)\n\n// Add sums two ints.\nfunc Add(a, b int) int {\n\treturn a + b\n}\n\ntype Vec struct {\n\tX, Y float64\n}\n";

#[test]
fn rust_full_surface() {
    let syms = symbols(Lang::Rust, RUST).unwrap();
    assert!(syms
        .iter()
        .any(|s| s.kind == "function" && s.name == "compute"));
    assert!(syms
        .iter()
        .any(|s| s.kind == "struct" && s.name == "Widget"));
    assert!(syms.iter().any(|s| s.kind == "enum" && s.name == "State"));
    // start lines are 1-based and ordered.
    let compute = syms.iter().find(|s| s.name == "compute").unwrap();
    assert_eq!(compute.start_line, 5);

    assert_eq!(count_functions(Lang::Rust, RUST), 1);
    assert_eq!(extract_imports(Lang::Rust, RUST).len(), 2);
    assert!(extract_comments(Lang::Rust, RUST)
        .iter()
        .any(|c| c.contains("doc comment")));
    assert!(extract_strings(Lang::Rust, RUST)
        .iter()
        .any(|s| s.contains("literal")));

    // ts_query: capture the function name.
    let names = query_texts(Lang::Rust, RUST, "(function_item name: (identifier) @n)").unwrap();
    assert_eq!(names, vec!["compute"]);

    // loc < count_lines (blank + comment lines dropped).
    assert!(loc(Lang::Rust, RUST).unwrap() < count_lines(RUST));
}

#[test]
fn python_surface() {
    let syms = symbols(Lang::Python, PYTHON).unwrap();
    assert!(syms
        .iter()
        .any(|s| s.kind == "class" && s.name == "Pipeline"));
    assert!(syms
        .iter()
        .any(|s| s.kind == "function" && s.name == "transform"));
    assert!(syms.iter().any(|s| s.kind == "function" && s.name == "run"));
    assert_eq!(count_functions(Lang::Python, PYTHON), 2);
    assert_eq!(extract_imports(Lang::Python, PYTHON).len(), 2);
}

#[test]
fn js_surface() {
    let syms = symbols(Lang::JavaScript, JS).unwrap();
    assert!(syms
        .iter()
        .any(|s| s.kind == "function" && s.name == "boot"));
    assert!(syms.iter().any(|s| s.kind == "class" && s.name == "App"));
    assert!(syms.iter().any(|s| s.kind == "method" && s.name == "start"));
    assert_eq!(extract_imports(Lang::JavaScript, JS).len(), 1);
}

#[test]
fn go_surface() {
    let syms = symbols(Lang::Go, GO).unwrap();
    assert!(syms.iter().any(|s| s.kind == "function" && s.name == "Add"));
    assert!(syms.iter().any(|s| s.kind == "struct" && s.name == "Vec"));
    assert_eq!(extract_imports(Lang::Go, GO).len(), 1);
}

#[test]
fn language_inference() {
    assert_eq!(language_of_filename("x.py"), Some("python"));
    assert_eq!(language_of_filename("main.rs"), Some("rust"));
    assert_eq!(language_of_filename("app.go"), Some("go"));
    assert_eq!(language_of_filename("Component.tsx"), Some("typescript"));
    assert_eq!(language_of_filename("data.json"), Some("json"));
    assert_eq!(language_of_filename("README"), None);
}

#[test]
fn supported_set_is_complete() {
    assert_eq!(SUPPORTED.len(), 9);
    for name in SUPPORTED {
        assert!(Lang::from_name(name).is_some());
    }
}

#[test]
fn garbage_and_empty_never_panic() {
    assert!(symbols(Lang::Rust, "").unwrap().is_empty());
    assert_eq!(count_lines(""), 0);
    let garbage = ")))(((@@@ not code at all 123 fn fn";
    // tree-sitter recovers; we just get few/no symbols, no panic.
    let _ = symbols(Lang::Rust, garbage).unwrap();
    let _ = symbols(Lang::Python, garbage).unwrap();
    let _ = symbols(Lang::Go, garbage).unwrap();
    assert_eq!(count_functions(Lang::Rust, garbage), 0);
}

#[test]
fn unknown_language_is_an_error() {
    assert!(matches!(
        resolve("haskell"),
        Err(AnalyzeError::UnknownLanguage(_))
    ));
}
