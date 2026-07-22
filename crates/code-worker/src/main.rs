//! The `code` VGI worker — native binary entrypoint.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'code' (TYPE vgi, LOCATION '…')`). All function registration and
//! catalog metadata live in the library crate ([`code_worker::build_worker`]) so
//! the browser (wasm) build can serve the exact same `Worker` over a different
//! transport. This binary only initializes logging and runs the worker over the
//! native stdio/HTTP transport.

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    code_worker::build_worker().run();
}
