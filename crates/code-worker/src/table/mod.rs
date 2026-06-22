//! Table functions exposed by the code worker, registered under `code.main`.
//!
//! DuckDB table functions take *constant* arguments (no row columns, no
//! subqueries), so `source` / `language` / `query` are bind-time constants,
//! passed positionally (e.g. `symbols('…', 'rust')`).

mod supported_languages;
mod symbols;
mod ts_nodes;

use vgi::Worker;

/// Register every table function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_table(symbols::Symbols);
    worker.register_table(ts_nodes::TsNodes);
    worker.register_table(supported_languages::SupportedLanguages);
}
