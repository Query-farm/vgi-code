//! `extract_imports`, `extract_comments`, `extract_strings`:
//! each `(source, language) -> VARCHAR[]`.
//!
//! All three share one impl ([`ExtractList`]) differing only in which per-language
//! tree-sitter query they run. They return a `LIST(VARCHAR)` whose Arrow `DataType`
//! is built identically at `on_bind` and `process` (the explicit match LIST returns
//! require). NULL source → NULL list; an unknown language → clear error.

use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
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
    example_sql: &'static str,
    example_desc: &'static str,
    title: &'static str,
    llm_desc: &'static str,
    md_desc: &'static str,
    keywords: &'static str,
}

impl ExtractList {
    pub fn imports() -> Self {
        ExtractList {
            kind: Kind::Imports,
            name: "extract_imports",
            desc: "Import / use / require statements in the source, as a VARCHAR[]",
            example_sql:
                "SELECT UNNEST(code.main.extract_imports('import os\nimport sys\n', 'python'));",
            example_desc: "Extract each import statement from Python source, one per row.",
            title: "Extract Import Statements",
            llm_desc:
                "Extract the import / use / require statements from a source string for a given \
                 language, returned as a VARCHAR[] of the statement texts. NULL source or \
                 language \u{2192} NULL list; an empty source \u{2192} empty list; an unknown \
                 language is a clear error. Use it to discover a file's dependencies.",
            md_desc: "Import/use/require statements as a `VARCHAR[]`, e.g. \
                 `extract_imports(src, 'python')`.",
            keywords: "imports, extract imports, dependencies, use statements, require, include, \
                 import list, modules",
        }
    }
    pub fn comments() -> Self {
        ExtractList {
            kind: Kind::Comments,
            name: "extract_comments",
            desc: "Comment texts in the source, as a VARCHAR[]",
            example_sql: "SELECT code.main.extract_comments('// header\nfn a() {}\n', 'rust');",
            example_desc: "Collect the comment texts from Rust source as a VARCHAR[].",
            title: "Extract Comment Texts",
            llm_desc:
                "Extract the comment texts (line and block comments) from a source string for a \
                 given language, returned as a VARCHAR[]. NULL source or language \u{2192} NULL \
                 list; empty source \u{2192} empty list; an unknown language is a clear error. \
                 Use it to mine documentation, TODOs, or license headers.",
            md_desc: "Comment texts as a `VARCHAR[]`, e.g. `extract_comments(src, 'rust')`.",
            keywords: "comments, extract comments, docstrings, todo, fixme, documentation, \
                 license header, annotations",
        }
    }
    pub fn strings() -> Self {
        ExtractList {
            kind: Kind::Strings,
            name: "extract_strings",
            desc: "String-literal texts in the source, as a VARCHAR[]",
            example_sql: "SELECT code.main.extract_strings('let s = \"hello\";\n', 'rust');",
            example_desc: "Collect the string-literal texts from Rust source as a VARCHAR[].",
            title: "Extract String Literals",
            llm_desc:
                "Extract the string-literal texts from a source string for a given language, \
                 returned as a VARCHAR[] (the literals including their quotes). NULL source or \
                 language \u{2192} NULL list; empty source \u{2192} empty list; an unknown \
                 language is a clear error. Use it to find hard-coded strings, secrets, or URLs.",
            md_desc: "String-literal texts as a `VARCHAR[]`, e.g. `extract_strings(src, 'rust')`.",
            keywords:
                "strings, extract strings, string literals, hardcoded strings, secrets, urls, \
                 text literals",
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
            examples: vec![FunctionExample {
                sql: self.example_sql.into(),
                description: self.example_desc.into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                self.title,
                self.llm_desc,
                self.md_desc,
                self.keywords,
                "Structure & Extraction",
                "scalar/lists.rs",
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
                "The source code to extract from, parsed with tree-sitter using \
                 the grammar selected by `language`.",
            ),
            ArgSpec::column(
                "language",
                1,
                "varchar",
                "The language id selecting the parser grammar, e.g. 'rust', \
                 'python', 'go'; must be one of supported_languages().",
            )
            .with_choices(parsing::SUPPORTED.iter().copied()),
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
