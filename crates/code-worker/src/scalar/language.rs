//! `language_of(filename) -> VARCHAR` — infer the source language from a file
//! extension. NULL filename → NULL; an unknown extension → NULL.

use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::text_str;
use crate::parsing;

pub struct LanguageOf;

impl ScalarFunction for LanguageOf {
    fn name(&self) -> &str {
        "language_of"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Infer the source language from a filename's extension (NULL if unknown)"
                .into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT code.main.language_of('src/main.rs');".into(),
                description: "Detect the source language of a file from its extension (→ 'rust')."
                    .into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Detect Source Language",
                "Infer the programming language of a file from its name or path extension, e.g. \
                 'src/main.rs' \u{2192} 'rust', 'app.py' \u{2192} 'python'. Returns NULL when the \
                 filename is NULL or its extension maps to no supported language. Use it to route \
                 files to the right language id for the other functions.",
                "Infer a file's language from its extension, e.g. \
                 `language_of('main.rs')` \u{2192} `'rust'`. NULL if unknown.",
                "language detection, language_of, detect language, file extension, file type, \
                 guess language, rust, python, javascript, typescript, go, java, c, cpp, json",
                "scalar/language.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column(
            "filename",
            0,
            "File name or path (VARCHAR)",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = StringBuilder::new();
        for i in 0..rows {
            match text_str(col, i)? {
                None => out.append_null(),
                Some(name) => match parsing::language_of_filename(name) {
                    Some(lang) => out.append_value(lang),
                    None => out.append_null(),
                },
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
    use crate::arrow_io::test_support::{bound_type, run_scalar_text1};
    use arrow_array::cast::AsArray;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    #[test]
    fn binds_varchar() {
        assert_eq!(bound_type(&LanguageOf), DataType::Utf8);
    }

    #[test]
    fn infers_known_and_nulls_unknown() {
        let out = run_scalar_text1(
            &LanguageOf,
            &[Some("x.py"), Some("main.rs"), Some("Makefile"), None],
            Arguments::default(),
        )
        .unwrap();
        let s = out.as_string::<i32>();
        assert_eq!(s.value(0), "python");
        assert_eq!(s.value(1), "rust");
        assert!(out.is_null(2), "no known extension → NULL");
        assert!(out.is_null(3), "NULL in → NULL out");
    }
}
