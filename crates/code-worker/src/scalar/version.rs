//! `code_version()` — return the worker's version string.

use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

pub struct CodeVersion;

impl ScalarFunction for CodeVersion {
    fn name(&self) -> &str {
        "code_version"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Returns the code worker version string".into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT code.main.code_version();".into(),
                description: "Return the running code worker's version string.".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Code Worker Version",
                "Return the running code worker's semantic version string. Takes no arguments \
                 and always returns one VARCHAR. Use it to confirm which build is attached or to \
                 gate behavior on a worker version.",
                "Return the code worker's version string, e.g. `code_version()` \u{2192} `'0.1.0'`.",
                "version, code_version, build, semantic version, worker version, release",
                "Language Detection & Discovery",
                "scalar/version.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let rows = batch.num_rows();
        let out: ArrayRef = Arc::new(StringArray::from(vec![crate::version(); rows]));
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}
