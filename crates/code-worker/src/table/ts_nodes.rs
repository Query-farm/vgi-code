//! `ts_nodes(source, language, query) -> (seq BIGINT, capture VARCHAR,
//! text VARCHAR, start_line INT, end_line INT)` — every capture of a tree-sitter
//! query against one source doc, one row each, in document order.
//!
//! `source` / `language` / `query` are bind-time constants. NULL source → no
//! rows. An unknown language or malformed query is a clear error at bind.

use std::sync::Arc;

use arrow_array::builder::{Int32Builder, Int64Builder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::parsing::{self, Capture};

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

pub struct TsNodes;

fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("seq", DataType::Int64, false),
        Field::new("capture", DataType::Utf8, true),
        Field::new("text", DataType::Utf8, true),
        Field::new("start_line", DataType::Int32, false),
        Field::new("end_line", DataType::Int32, false),
    ]))
}

impl TableFunction for TsNodes {
    fn name(&self) -> &str {
        "ts_nodes"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description:
                "Tree-sitter query matches over a source doc as (seq, capture, text, start_line, end_line) rows"
                    .into(),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::const_arg("source", 0, "varchar", "Source code (VARCHAR)"),
            ArgSpec::const_arg("language", 1, "varchar", "Language id, e.g. 'rust'"),
            ArgSpec::const_arg("query", 2, "varchar", "tree-sitter S-expression query"),
        ]
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        // Validate language + query eagerly at bind for a clear early error.
        if let (Some(src), Some(lang), Some(q)) = (
            params.arguments.const_str(0),
            params.arguments.const_str(1),
            params.arguments.const_str(2),
        ) {
            let lang = parsing::resolve(&lang).map_err(ve)?;
            parsing::run_query(lang, &src, &q).map_err(ve)?;
        } else if let Some(lang) = params.arguments.const_str(1) {
            parsing::resolve(&lang).map_err(ve)?;
        }
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        let rows = match (
            params.arguments.const_str(0),
            params.arguments.const_str(1),
            params.arguments.const_str(2),
        ) {
            (Some(src), Some(lang_name), Some(query)) => {
                let lang = parsing::resolve(&lang_name).map_err(ve)?;
                parsing::run_query(lang, &src, &query).map_err(ve)?
            }
            _ => Vec::new(),
        };
        Ok(Box::new(NodesProducer {
            schema: params.output_schema.clone(),
            rows,
            done: false,
        }))
    }
}

struct NodesProducer {
    schema: SchemaRef,
    rows: Vec<Capture>,
    done: bool,
}

impl TableProducer for NodesProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;

        let mut seq = Int64Builder::new();
        let mut capture = StringBuilder::new();
        let mut text = StringBuilder::new();
        let mut start = Int32Builder::new();
        let mut end = Int32Builder::new();
        for (i, c) in self.rows.iter().enumerate() {
            seq.append_value(i as i64);
            capture.append_value(&c.capture);
            text.append_value(&c.text);
            start.append_value(c.start_line);
            end.append_value(c.end_line);
        }
        let cols: Vec<ArrayRef> = vec![
            Arc::new(seq.finish()),
            Arc::new(capture.finish()),
            Arc::new(text.finish()),
            Arc::new(start.finish()),
            Arc::new(end.finish()),
        ];
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), cols)
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
