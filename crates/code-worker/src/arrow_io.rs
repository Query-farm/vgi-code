//! Small Arrow helpers shared across the scalar functions: reading VARCHAR input
//! cells and constructing the `LIST(VARCHAR)` output type/array in a way that
//! `on_bind` and `process` agree on exactly (the explicit Arrow `DataType` match
//! that LIST/STRUCT returns require). The in-process test harness below drives a
//! `ScalarFunction` end to end without the RPC/IPC plumbing.

use std::sync::Arc;

use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::cast::AsArray;
use arrow_array::{Array, ArrayRef, ListArray};
use arrow_schema::{DataType, Field};
use vgi_rpc::{Result, RpcError};

/// Borrow the UTF-8 text of a VARCHAR cell at `row`, or `None` if null. Errors if
/// the column isn't a string type.
pub fn text_str(col: &ArrayRef, row: usize) -> Result<Option<&str>> {
    if col.is_null(row) {
        return Ok(None);
    }
    Ok(Some(match col.data_type() {
        DataType::Utf8 => col.as_string::<i32>().value(row),
        DataType::LargeUtf8 => col.as_string::<i64>().value(row),
        other => {
            return Err(RpcError::value_error(format!(
                "expected a VARCHAR (string) argument, got {other:?}"
            )))
        }
    }))
}

/// The child `Field` of our `LIST(VARCHAR)` outputs. DuckDB list children are
/// conventionally named `item` and nullable.
fn list_item_field() -> Arc<Field> {
    Arc::new(Field::new("item", DataType::Utf8, true))
}

/// The exact `DataType::List` our [`list_builder`] produces, so `on_bind` can
/// publish an output schema that matches the array built in `process`.
pub fn list_varchar_type() -> DataType {
    DataType::List(list_item_field())
}

/// A fresh `ListBuilder<StringBuilder>` whose finished array has the canonical
/// `item`-named child field (matching [`list_varchar_type`]).
pub fn list_builder() -> ListBuilder<StringBuilder> {
    ListBuilder::new(StringBuilder::new()).with_field(list_item_field())
}

/// Append one list row of strings.
pub fn append_list_row(builder: &mut ListBuilder<StringBuilder>, items: &[String]) {
    for s in items {
        builder.values().append_value(s);
    }
    builder.append(true);
}

/// Append a NULL list row.
pub fn append_list_null(builder: &mut ListBuilder<StringBuilder>) {
    builder.append(false);
}

/// Finish a list builder into an `ArrayRef`.
pub fn finish_list(mut builder: ListBuilder<StringBuilder>) -> ArrayRef {
    let arr: ListArray = builder.finish();
    Arc::new(arr)
}

/// Test-only helpers shared by the scalar Arrow-boundary unit tests. These let a
/// `#[cfg(test)]` block drive a `ScalarFunction` end to end in-process (build a
/// VARCHAR input `RecordBatch`, run `on_bind` + `process`, inspect the result)
/// without the RPC/IPC plumbing.
#[cfg(test)]
pub mod test_support {
    use std::sync::Arc;

    use arrow_array::builder::StringBuilder;
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use arrow_schema::{Field, Schema, SchemaRef};
    use vgi::arguments::Arguments;
    use vgi::{BindParams, ProcessParams, ScalarFunction};
    use vgi_rpc::Result;

    /// Positional const args as DuckDB would hand them: each `Some(s)` becomes a
    /// 1-row VARCHAR positional argument, `None` a NULL one.
    pub fn str_args(values: &[Option<&str>]) -> Arguments {
        let cols: Vec<ArrayRef> = values
            .iter()
            .map(|v| Arc::new(StringArray::from(vec![*v])) as ArrayRef)
            .collect();
        let bytes = Arguments::serialize_positional(&cols).unwrap();
        Arguments::parse(&bytes).unwrap()
    }

    /// A multi-column VARCHAR input batch (one column per arg). Column `c`'s
    /// row values come from `cols[c]`; all columns must be the same length.
    pub fn text_batch(cols: &[&[Option<&str>]]) -> RecordBatch {
        let mut arrays: Vec<ArrayRef> = Vec::new();
        let mut fields: Vec<Field> = Vec::new();
        for (i, col) in cols.iter().enumerate() {
            let mut b = StringBuilder::new();
            for r in *col {
                match r {
                    Some(s) => b.append_value(s),
                    None => b.append_null(),
                }
            }
            let arr: ArrayRef = Arc::new(b.finish());
            fields.push(Field::new(format!("a{i}"), arr.data_type().clone(), true));
            arrays.push(arr);
        }
        let schema = Arc::new(Schema::new(fields));
        RecordBatch::try_new(schema, arrays).unwrap()
    }

    /// Build a `ProcessParams` carrying the given output schema and arguments.
    pub fn process_params(output_schema: SchemaRef, arguments: Arguments) -> ProcessParams {
        ProcessParams {
            output_schema,
            input_schema: None,
            execution_id: Vec::new(),
            init_opaque_data: Vec::new(),
            arguments,
            settings: Default::default(),
            secrets: Default::default(),
            auth_principal: None,
            projection_ids: None,
            pushdown_filters: None,
            join_keys: Vec::new(),
            storage: None,
            order_by_column: None,
            order_by_direction: None,
            order_by_null_order: None,
            order_by_limit: None,
            tablesample_percentage: None,
            tablesample_seed: None,
            attach_opaque_data: None,
            at_unit: None,
            at_value: None,
        }
    }

    /// Run a scalar function over a prebuilt input batch: call `on_bind` to obtain
    /// the declared output schema, then `process`, returning the single result
    /// column. The `arguments` apply to both bind and process.
    pub fn run_scalar_on<F: ScalarFunction>(
        f: &F,
        batch: RecordBatch,
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        let bind = BindParams {
            input_schema: Some(batch.schema()),
            arguments: arguments.clone(),
            ..Default::default()
        };
        let bound = f.on_bind(&bind)?;
        let params = process_params(bound.output_schema.clone(), arguments);
        let out = f.process(&params, &batch)?;
        Ok(out.column(0).clone())
    }

    /// Run a scalar whose only input is a single VARCHAR column (`source`).
    pub fn run_scalar_text1<F: ScalarFunction>(
        f: &F,
        rows: &[Option<&str>],
        arguments: Arguments,
    ) -> Result<ArrayRef> {
        run_scalar_on(f, text_batch(&[rows]), arguments)
    }

    /// The declared output `DataType` from `on_bind` for a scalar with no
    /// bind-time argument requirements.
    pub fn bound_type<F: ScalarFunction>(f: &F) -> arrow_schema::DataType {
        let bind = BindParams::default();
        let bound = f.on_bind(&bind).unwrap();
        bound.output_schema.field(0).data_type().clone()
    }
}
