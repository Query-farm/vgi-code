//! `symbols(source, language) -> (kind VARCHAR, name VARCHAR, start_line INT,
//! end_line INT)` — every function/class/method/struct/enum/… in one source doc,
//! one row each, ordered by start line.
//!
//! `source` and `language` are bind-time constants. NULL source → no rows.
//! Unparseable source is best-effort (tree-sitter recovers) → whatever symbols
//! it can find. An unknown language is a clear error at bind.

use std::sync::Arc;

use arrow_array::builder::{Int32Builder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::parsing::{self, Symbol};

fn ve(e: impl std::fmt::Display) -> RpcError {
    RpcError::value_error(e.to_string())
}

pub struct Symbols;

fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("kind", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("start_line", DataType::Int32, false),
        Field::new("end_line", DataType::Int32, false),
    ]))
}

impl TableFunction for Symbols {
    fn name(&self) -> &str {
        "symbols"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Source Symbols Listing",
            "List the structural symbols of one source document — functions, methods, classes, \
             structs, enums, interfaces and traits — one row each, ordered by start line, with \
             their kind, name and 1-based line span. `source` and `language` are bind-time \
             constants. NULL source \u{2192} no rows; malformed source is best-effort (tree-\
             sitter recovers); an unknown language is a clear error. Use it to outline or index \
             a file's definitions.",
            "Structural symbols of a source doc as `(kind, name, start_line, end_line)` rows, \
             e.g. `symbols('fn a() {}', 'rust')`.",
            "symbols, outline, definitions, functions, classes, methods, structs, enums, traits, \
             interfaces, code index, navigation, table of contents",
            "Structure & Extraction",
            "table/symbols.rs",
        );
        // VGI307/VGI321: static result schema as structured JSON {name,type,description}.
        // Types match `output_schema()` exactly (Utf8→VARCHAR, Int32→INTEGER) so
        // VGI910 (schema matches what the function returns under --execute) holds.
        tags.push((
            "vgi.result_columns_schema".into(),
            r#"[
  {"name":"kind","type":"VARCHAR","description":"Symbol kind: one of function, method, class, struct, enum, interface, or trait."},
  {"name":"name","type":"VARCHAR","description":"The symbol's identifier (NULL when the definition is anonymous)."},
  {"name":"start_line","type":"INTEGER","description":"1-based line where the definition starts."},
  {"name":"end_line","type":"INTEGER","description":"1-based line where the definition ends."}
]"#
            .into(),
        ));
        FunctionMetadata {
            description:
                "Structural symbols (functions, classes, methods, structs, enums, …) of a source doc"
                    .into(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::const_arg(
                "source",
                0,
                "varchar",
                "The source document to outline, parsed with tree-sitter using \
                 the grammar selected by `language` (a bind-time constant).",
            ),
            ArgSpec::const_arg(
                "language",
                1,
                "varchar",
                "The language id selecting the parser grammar, e.g. 'rust', \
                 'python', 'go'; must be one of supported_languages() \
                 (a bind-time constant).",
            )
            .with_choices(parsing::SUPPORTED.iter().copied()),
        ]
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        // Validate the language eagerly at bind (clear error for unknown).
        if let Some(lang) = params.arguments.const_str(1) {
            parsing::resolve(&lang).map_err(ve)?;
        }
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        let rows = match (params.arguments.const_str(0), params.arguments.const_str(1)) {
            (Some(src), Some(lang_name)) => {
                let lang = parsing::resolve(&lang_name).map_err(ve)?;
                parsing::symbols(lang, &src).map_err(ve)?
            }
            // NULL source or language → no rows.
            _ => Vec::new(),
        };
        Ok(Box::new(SymbolsProducer {
            schema: params.output_schema.clone(),
            rows,
            done: false,
        }))
    }
}

struct SymbolsProducer {
    schema: SchemaRef,
    rows: Vec<Symbol>,
    done: bool,
}

impl TableProducer for SymbolsProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;

        let mut kind = StringBuilder::new();
        let mut name = StringBuilder::new();
        let mut start = Int32Builder::new();
        let mut end = Int32Builder::new();
        for s in &self.rows {
            kind.append_value(&s.kind);
            name.append_value(&s.name);
            start.append_value(s.start_line);
            end.append_value(s.end_line);
        }
        let cols: Vec<ArrayRef> = vec![
            Arc::new(kind.finish()),
            Arc::new(name.finish()),
            Arc::new(start.finish()),
            Arc::new(end.finish()),
        ];
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), cols)
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
