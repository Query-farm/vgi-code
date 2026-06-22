//! The `code` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'code' (TYPE vgi, LOCATION '…')`). It parses source code with
//! tree-sitter and exposes its structure — symbols, imports, comments, strings,
//! line counts and arbitrary tree-sitter queries — to SQL under the catalog
//! `code`, schema `main`:
//!
//! ```sql
//! ATTACH 'code' (TYPE vgi, LOCATION './target/release/code-worker');
//! SET search_path = 'code.main';
//!
//! SELECT language_of('main.rs');                       -- → 'rust'
//! SELECT count_functions(src, 'rust') FROM files;      -- function count
//! SELECT UNNEST(extract_imports(src, 'python'));        -- one import per row
//! SELECT * FROM symbols('fn a(){}', 'rust');           -- kind/name/start/end
//! SELECT * FROM ts_nodes(src, 'go', '(function_declaration) @f');
//! ```
//!
//! Pure analysis (no Arrow) lives in `parsing.rs`; the `scalar/` and `table/`
//! modules are thin Arrow adapters over it.

mod arrow_io;
mod parsing;
mod scalar;
mod table;

use vgi::Worker;

/// Worker version string, surfaced by `code_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    // The catalog name DuckDB sees in `ATTACH 'code' (TYPE vgi, …)`. Default to
    // `code`, but honor an explicit override so a test harness can rename it.
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "code");
    }

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.run();
}
