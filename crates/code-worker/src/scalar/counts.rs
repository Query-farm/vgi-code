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
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
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
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column("source", 0, "Source code (VARCHAR)")]
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
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("source", 0, "Source code (VARCHAR)"),
            ArgSpec::any_column("language", 1, "Language id, e.g. 'rust' (VARCHAR)"),
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
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("source", 0, "Source code (VARCHAR)"),
            ArgSpec::any_column("language", 1, "Language id, e.g. 'rust' (VARCHAR)"),
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
