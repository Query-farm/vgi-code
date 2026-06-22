//! `extract_imports`, `extract_comments`, `extract_strings`:
//! each `(source, language) -> VARCHAR[]`.
//!
//! All three share one impl ([`ExtractList`]) differing only in which per-language
//! tree-sitter query they run. They return a `LIST(VARCHAR)` whose Arrow `DataType`
//! is built identically at `on_bind` and `process` (the explicit match LIST returns
//! require). NULL source → NULL list; an unknown language → clear error.

use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{
    append_list_null, append_list_row, finish_list, list_builder, list_varchar_type, text_str,
};
use crate::parsing::{self, Lang};

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

/// Which extraction a given instance performs.
#[derive(Clone, Copy)]
enum Kind {
    Imports,
    Comments,
    Strings,
}

impl Kind {
    fn run(self, lang: Lang, source: &str) -> Vec<String> {
        match self {
            Kind::Imports => parsing::extract_imports(lang, source),
            Kind::Comments => parsing::extract_comments(lang, source),
            Kind::Strings => parsing::extract_strings(lang, source),
        }
    }
}

pub struct ExtractList {
    kind: Kind,
    name: &'static str,
    desc: &'static str,
}

impl ExtractList {
    pub fn imports() -> Self {
        ExtractList {
            kind: Kind::Imports,
            name: "extract_imports",
            desc: "Import / use / require statements in the source, as a VARCHAR[]",
        }
    }
    pub fn comments() -> Self {
        ExtractList {
            kind: Kind::Comments,
            name: "extract_comments",
            desc: "Comment texts in the source, as a VARCHAR[]",
        }
    }
    pub fn strings() -> Self {
        ExtractList {
            kind: Kind::Strings,
            name: "extract_strings",
            desc: "String-literal texts in the source, as a VARCHAR[]",
        }
    }
}

impl ScalarFunction for ExtractList {
    fn name(&self) -> &str {
        self.name
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: self.desc.into(),
            return_type: Some(list_varchar_type()),
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
        Ok(BindResponse::result(list_varchar_type()))
    }

    fn process(
        &self,
        params: &ProcessParams,
        batch: &arrow_array::RecordBatch,
    ) -> Result<arrow_array::RecordBatch> {
        let src_col = batch.column(0);
        let lang_col = batch.column(1);
        let rows = batch.num_rows();
        let mut builder = list_builder();
        for i in 0..rows {
            match (text_str(src_col, i)?, text_str(lang_col, i)?) {
                (Some(src), Some(lang_name)) => {
                    let lang = parsing::resolve(lang_name).map_err(ve)?;
                    let items = self.kind.run(lang, src);
                    append_list_row(&mut builder, &items);
                }
                // NULL source or NULL language → NULL list.
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

    const RUST: &str = "use std::fmt;\nuse std::io;\n// hi\nfn a() { let s = \"x\"; }\n";

    fn run(f: &ExtractList, src: &str, lang: &str) -> arrow_array::ArrayRef {
        let batch = text_batch(&[&[Some(src)], &[Some(lang)]]);
        run_scalar_on(f, batch, str_args(&[Some(src), Some(lang)])).unwrap()
    }

    #[test]
    fn bind_matches_built_list_type() {
        assert_eq!(bound_type(&ExtractList::imports()), list_varchar_type());
    }

    #[test]
    fn imports_listed() {
        let out = run(&ExtractList::imports(), RUST, "rust");
        let list = out.as_list::<i32>();
        let row = list.value(0);
        let strs = row.as_string::<i32>();
        assert_eq!(strs.len(), 2);
        assert!(strs.value(0).contains("fmt"));
    }

    #[test]
    fn comments_and_strings_listed() {
        let c = run(&ExtractList::comments(), RUST, "rust");
        let crow = c.as_list::<i32>().value(0);
        assert!(crow.as_string::<i32>().value(0).contains("hi"));

        let s = run(&ExtractList::strings(), RUST, "rust");
        let srow = s.as_list::<i32>().value(0);
        assert_eq!(srow.as_string::<i32>().value(0), "\"x\"");
    }

    #[test]
    fn null_source_yields_null_list() {
        let batch = text_batch(&[&[None], &[Some("rust")]]);
        let out = run_scalar_on(
            &ExtractList::imports(),
            batch,
            str_args(&[None, Some("rust")]),
        )
        .unwrap();
        assert!(out.is_null(0));
    }

    #[test]
    fn empty_source_is_empty_list_not_null() {
        let out = run(&ExtractList::imports(), "", "rust");
        let list = out.as_list::<i32>();
        assert!(!out.is_null(0));
        assert_eq!(list.value(0).len(), 0);
    }

    #[test]
    fn unknown_language_errors() {
        let batch = text_batch(&[&[Some(RUST)], &[Some("cobol")]]);
        let err = run_scalar_on(
            &ExtractList::imports(),
            batch,
            str_args(&[Some(RUST), Some("cobol")]),
        );
        assert!(err.is_err());
    }
}
