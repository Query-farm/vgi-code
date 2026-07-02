//! Line / function counting scalars:
//! - `count_lines(source) -> INT` — total physical lines.
//! - `loc(source, language) -> INT` — non-blank, non-comment lines.
//! - `count_functions(source, language) -> INT` — function/method definitions.
//!
//! `language` is a per-row VARCHAR column. An unknown language is a clear error
//! (the caller named it); NULL source → NULL.

use std::sync::Arc;

use arrow_array::builder::Int32Builder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::text_str;
use crate::parsing;

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

/// `count_lines(source) -> INT`.
pub struct CountLines;

impl ScalarFunction for CountLines {
    fn name(&self) -> &str {
        "count_lines"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Total number of physical lines in the source (NULL → NULL)".into(),
            return_type: Some(DataType::Int32),
            examples: vec![FunctionExample {
                sql: "SELECT code.main.count_lines('fn a() {}\nfn b() {}\n');".into(),
                description: "Count the physical lines in a source string (→ 2).".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Count Physical Lines",
                "Count the total number of physical lines in a source string, including blank \
                 and comment lines. Language-agnostic (no parsing). A single trailing newline is \
                 NOT counted as an extra line, so 'a\\nb\\n' and 'a\\nb' both report 2; an empty \
                 string reports 0. Returns NULL when the source is NULL. Use it for raw file-size \
                 / line-count metrics.",
                "Total physical lines in the source, e.g. \
                 `count_lines('a\\nb\\n')` \u{2192} `2`. NULL in \u{2192} NULL.",
                "count lines, line count, physical lines, total lines, file size, metrics, wc -l",
                "Code Metrics",
                "scalar/counts.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::column(
            "source",
            0,
            "varchar",
            "The source code to count lines in; counted verbatim with no parsing.",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Int32))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = Int32Builder::new();
        for i in 0..rows {
            match text_str(col, i)? {
                None => out.append_null(),
                Some(src) => out.append_value(parsing::count_lines(src)),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

/// `loc(source, language) -> INT`.
pub struct Loc;

impl ScalarFunction for Loc {
    fn name(&self) -> &str {
        "loc"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Lines of code: non-blank, non-comment lines for the given language"
                .into(),
            return_type: Some(DataType::Int32),
            examples: vec![FunctionExample {
                sql: "SELECT code.main.loc('fn a() {}\n// note\nfn b() {}\n', 'rust');".into(),
                description: "Count lines of code, excluding blank and comment lines (→ 2).".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Lines Of Code",
                "Count the lines of code (LOC) in a source string for a given language: physical \
                 lines that are neither blank nor purely comments. Returns NULL when the source \
                 or language is NULL; an unknown language id is a clear error. Use it for code-\
                 size metrics that ignore whitespace and comments.",
                "Non-blank, non-comment lines for a language, e.g. \
                 `loc(src, 'rust')`. NULL in \u{2192} NULL; unknown language errors.",
                "lines of code, loc, sloc, code metrics, non-comment lines, effective lines, \
                 source lines",
                "Code Metrics",
                "scalar/counts.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::column(
                "source",
                0,
                "varchar",
                "The source code to analyze, parsed with tree-sitter using the \
                 grammar selected by `language`.",
            ),
            ArgSpec::column(
                "language",
                1,
                "varchar",
                "The language id selecting the parser grammar, e.g. 'rust', \
                 'python', 'go'; must be one of supported_languages().",
            ),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Int32))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let src_col = batch.column(0);
        let lang_col = batch.column(1);
        let rows = batch.num_rows();
        let mut out = Int32Builder::new();
        for i in 0..rows {
            match (text_str(src_col, i)?, text_str(lang_col, i)?) {
                (Some(src), Some(lang_name)) => {
                    let lang = parsing::resolve(lang_name).map_err(ve)?;
                    out.append_value(parsing::loc(lang, src).map_err(ve)?);
                }
                // NULL source or NULL language → NULL.
                _ => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

/// `count_functions(source, language) -> INT`.
pub struct CountFunctions;

impl ScalarFunction for CountFunctions {
    fn name(&self) -> &str {
        "count_functions"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Number of function/method definitions for the given language".into(),
            return_type: Some(DataType::Int32),
            examples: vec![FunctionExample {
                sql:
                    "SELECT code.main.count_functions('def a(): pass\ndef b(): pass\n', 'python');"
                        .into(),
                description: "Count function/method definitions in the source (→ 2).".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Count Function Definitions",
                "Count the function and method definitions in a source string for a given \
                 language, using tree-sitter to find definition nodes. Returns NULL when the \
                 source or language is NULL; an unknown language id is a clear error. Use it as a \
                 quick structural metric of how many functions a file defines.",
                "Number of function/method definitions for a language, e.g. \
                 `count_functions(src, 'python')` \u{2192} `2`.",
                "count functions, function count, number of functions, methods, definitions, \
                 code metrics, complexity",
                "Code Metrics",
                "scalar/counts.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::column(
                "source",
                0,
                "varchar",
                "The source code to analyze, parsed with tree-sitter using the \
                 grammar selected by `language`.",
            ),
            ArgSpec::column(
                "language",
                1,
                "varchar",
                "The language id selecting the parser grammar, e.g. 'rust', \
                 'python', 'go'; must be one of supported_languages().",
            ),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Int32))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let src_col = batch.column(0);
        let lang_col = batch.column(1);
        let rows = batch.num_rows();
        let mut out = Int32Builder::new();
        for i in 0..rows {
            match (text_str(src_col, i)?, text_str(lang_col, i)?) {
                (Some(src), Some(lang_name)) => {
                    let lang = parsing::resolve(lang_name).map_err(ve)?;
                    out.append_value(parsing::count_functions(lang, src));
                }
                _ => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{bound_type, run_scalar_on, str_args, text_batch};
    use arrow_array::cast::AsArray;
    use arrow_array::types::Int32Type;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    const RUST: &str = "fn a() {}\n// c\nfn b() {}\n";

    #[test]
    fn count_lines_binds_int_and_counts() {
        assert_eq!(bound_type(&CountLines), DataType::Int32);
        let out = run_scalar_on(
            &CountLines,
            text_batch(&[&[Some(RUST), Some(""), None]]),
            Arguments::default(),
        )
        .unwrap();
        let v = out.as_primitive::<Int32Type>();
        assert_eq!(v.value(0), 3);
        assert_eq!(v.value(1), 0);
        assert!(out.is_null(2));
    }

    #[test]
    fn loc_excludes_comments() {
        let batch = text_batch(&[&[Some(RUST)], &[Some("rust")]]);
        let out = run_scalar_on(&Loc, batch, str_args(&[Some(RUST), Some("rust")])).unwrap();
        // 2 fn lines are code; the `// c` comment line is excluded.
        assert_eq!(out.as_primitive::<Int32Type>().value(0), 2);
    }

    #[test]
    fn count_functions_counts_two() {
        let batch = text_batch(&[&[Some(RUST)], &[Some("rust")]]);
        let out = run_scalar_on(
            &CountFunctions,
            batch,
            str_args(&[Some(RUST), Some("rust")]),
        )
        .unwrap();
        assert_eq!(out.as_primitive::<Int32Type>().value(0), 2);
    }

    #[test]
    fn unknown_language_errors() {
        let batch = text_batch(&[&[Some(RUST)], &[Some("cobol")]]);
        let err = run_scalar_on(&Loc, batch, str_args(&[Some(RUST), Some("cobol")]));
        assert!(err.is_err(), "unknown language must be a clear error");
    }
}
