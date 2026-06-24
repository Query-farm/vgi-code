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

/// Guaranteed-runnable, catalog-qualified examples (VGI509). Each `sql` is
/// self-contained and re-runnable against an attached `code` worker. We omit
/// `expected_result` deliberately — the linter only needs each query to execute
/// cleanly, and the version string / row order would make exact output brittle.
const EXECUTABLE_EXAMPLES: &str = r#"[
  {
    "description": "List every language id the worker can parse.",
    "sql": "SELECT language FROM code.main.supported_languages() ORDER BY language"
  },
  {
    "description": "Detect a file's language from its extension.",
    "sql": "SELECT code.main.language_of('src/main.rs') AS lang"
  },
  {
    "description": "Count the physical lines in a source string.",
    "sql": "SELECT code.main.count_lines('fn a() {}\nfn b() {}\n') AS lines"
  },
  {
    "description": "Count the lines of code, excluding blank and comment lines.",
    "sql": "SELECT code.main.loc('fn a() {}\n// note\nfn b() {}\n', 'rust') AS loc"
  },
  {
    "description": "Count function/method definitions in Python source.",
    "sql": "SELECT code.main.count_functions('def a(): pass\ndef b(): pass\n', 'python') AS fns"
  },
  {
    "description": "Extract the import statements from Python source.",
    "sql": "SELECT code.main.extract_imports('import os\nimport sys\n', 'python') AS imports"
  },
  {
    "description": "List the structural symbols of a Rust source doc.",
    "sql": "SELECT kind, name, start_line FROM code.main.symbols('fn alpha() {}\nfn beta() {}\n', 'rust') ORDER BY start_line"
  },
  {
    "description": "Run a tree-sitter query and read its captures as rows.",
    "sql": "SELECT capture, text FROM code.main.ts_nodes('fn alpha() {}\n', 'rust', '(function_item name: (identifier) @n)') ORDER BY seq"
  },
  {
    "description": "Report the running code worker's version.",
    "sql": "SELECT code.main.code_version() AS version"
  }
]"#;

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
        let mut tags = crate::meta::object_tags(
            "Supported Languages Catalog",
            "List every language id this worker can parse, one per row. These are the exact \
             values accepted as the `language` argument by language_of, loc, count_functions, \
             the extract_* functions, ts_query, symbols and ts_nodes. Use it to discover which \
             languages are available.",
            "List the language ids this worker can parse. Column: `language`.",
            "supported languages, list languages, available languages, language ids, discovery, \
             what languages, grammars",
            "table/supported_languages.rs",
        );
        tags.push((
            "vgi.result_columns_md".into(),
            "| column | type | description |\n\
             |---|---|---|\n\
             | `language` | VARCHAR | A language id accepted by the other functions, e.g. \
             `rust`, `python`, `go`. |"
                .into(),
        ));
        tags.push(("vgi.executable_examples".into(), EXECUTABLE_EXAMPLES.into()));
        FunctionMetadata {
            description: "List the language ids this worker can parse".into(),
            tags,
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
