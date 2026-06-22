//! Scalar functions exposed by the code worker, registered under `code.main`.
//!
//! All scalars are positional-only (DuckDB scalar functions take no named args).
//! Every per-row value is read from a column; the `language`/`query` arguments
//! are likewise per-row VARCHAR columns (constant-folded by DuckDB when a literal
//! is passed), so a single call can mix languages across rows.

mod counts;
mod language;
mod lists;
mod query;
mod version;

use vgi::Worker;

/// Register every scalar function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_scalar(version::CodeVersion);
    worker.register_scalar(language::LanguageOf);
    worker.register_scalar(counts::CountLines);
    worker.register_scalar(counts::Loc);
    worker.register_scalar(counts::CountFunctions);
    worker.register_scalar(lists::ExtractList::imports());
    worker.register_scalar(lists::ExtractList::comments());
    worker.register_scalar(lists::ExtractList::strings());
    worker.register_scalar(query::TsQuery);
}
