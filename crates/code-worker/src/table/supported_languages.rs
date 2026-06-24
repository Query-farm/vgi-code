//! `supported_languages() -> (language VARCHAR)` — the set of language ids the
//! worker can parse, for discovery.

use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::parsing;

pub struct SupportedLanguages;

fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new(
        "language",
        DataType::Utf8,
        false,
    )]))
}

impl TableFunction for SupportedLanguages {
    fn name(&self) -> &str {
        "supported_languages"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "List the language ids this worker can parse".into(),
            tags: vec![(
                "vgi.columns_md".into(),
                "| column | type | description |\n\
                 |---|---|---|\n\
                 | `language` | VARCHAR | A language id accepted by the other functions, e.g. \
                 `rust`, `python`, `go`. |"
                    .into(),
            )],
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        Ok(Box::new(LangsProducer {
            schema: params.output_schema.clone(),
            done: false,
        }))
    }
}

struct LangsProducer {
    schema: SchemaRef,
    done: bool,
}

impl TableProducer for LangsProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        let mut b = StringBuilder::new();
        for l in parsing::SUPPORTED {
            b.append_value(l);
        }
        let col: ArrayRef = Arc::new(b.finish());
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), vec![col])
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
