//! `ts_query(source, language, query) -> VARCHAR[]` — run an arbitrary
//! tree-sitter query and return every captured node's text. The power feature.
//!
//! NULL source / language / query → NULL list. An unknown language or a malformed
//! query is a clear error (both are caller mistakes).

use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{
    append_list_null, append_list_row, finish_list, list_builder, list_varchar_type, text_str,
};
use crate::parsing;

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

pub struct TsQuery;

impl ScalarFunction for TsQuery {
    fn name(&self) -> &str {
        "ts_query"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description:
                "Run a tree-sitter query over the source and return the captured node texts (VARCHAR[])"
                    .into(),
            return_type: Some(list_varchar_type()),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("source", 0, "Source code (VARCHAR)"),
            ArgSpec::any_column("language", 1, "Language id, e.g. 'rust' (VARCHAR)"),
            ArgSpec::any_column("query", 2, "tree-sitter S-expression query (VARCHAR)"),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(list_varchar_type()))
    }

    fn process(
        &self,
        params: &ProcessParams,
        batch: &arrow_array::RecordBatch,
    ) -> Result<arrow_array::RecordBatch> {
        let src_col = batch.column(0);
        let lang_col = batch.column(1);
        let q_col = batch.column(2);
        let rows = batch.num_rows();
        let mut builder = list_builder();
        for i in 0..rows {
            match (
                text_str(src_col, i)?,
                text_str(lang_col, i)?,
                text_str(q_col, i)?,
            ) {
                (Some(src), Some(lang_name), Some(query)) => {
                    let lang = parsing::resolve(lang_name).map_err(ve)?;
                    let items = parsing::query_texts(lang, src, query).map_err(ve)?;
                    append_list_row(&mut builder, &items);
                }
                _ => append_list_null(&mut builder),
            }
        }
        let out = finish_list(builder);
        arrow_array::RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{bound_type, run_scalar_on, str_args, text_batch};
    use arrow_array::cast::AsArray;
    use arrow_array::Array;

    const RUST: &str = "fn alpha() {}\nfn beta() {}\n";
    const Q: &str = "(function_item name: (identifier) @n)";

    #[test]
    fn binds_list() {
        assert_eq!(bound_type(&TsQuery), list_varchar_type());
    }

    #[test]
    fn captures_function_names() {
        let batch = text_batch(&[&[Some(RUST)], &[Some("rust")], &[Some(Q)]]);
        let out = run_scalar_on(
            &TsQuery,
            batch,
            str_args(&[Some(RUST), Some("rust"), Some(Q)]),
        )
        .unwrap();
        let row = out.as_list::<i32>().value(0);
        let strs = row.as_string::<i32>();
        let got: Vec<&str> = (0..strs.len()).map(|i| strs.value(i)).collect();
        assert_eq!(got, vec!["alpha", "beta"]);
    }

    #[test]
    fn bad_query_errors() {
        let bad = "(this is not valid";
        let batch = text_batch(&[&[Some(RUST)], &[Some("rust")], &[Some(bad)]]);
        let err = run_scalar_on(
            &TsQuery,
            batch,
            str_args(&[Some(RUST), Some("rust"), Some(bad)]),
        );
        assert!(err.is_err());
    }

    #[test]
    fn null_args_yield_null() {
        let batch = text_batch(&[&[None], &[Some("rust")], &[Some(Q)]]);
        let out = run_scalar_on(&TsQuery, batch, str_args(&[None, Some("rust"), Some(Q)])).unwrap();
        assert!(out.is_null(0));
    }
}
