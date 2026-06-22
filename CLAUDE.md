# CLAUDE.md — vgi-code

Contributor/agent notes. User-facing docs live in `README.md`; this is the
"how it's built and where the sharp edges are" companion.

## What this is

A [VGI](https://query.farm) worker (Rust, compiled binary) exposing
**source-code structure** to DuckDB/SQL over Arrow IPC. Built on the `vgi` crate
(crates.io), modeled on `vgi-image` / `vgi-barcode`. Catalog name `code` (single
`main` schema). Parsing via [`tree-sitter`](https://crates.io/crates/tree-sitter)
plus a curated set of grammar crates.

## Layout

```
Cargo.toml                          workspace; pins vgi = "0.5.0", tree-sitter + grammars
crates/code-worker/
  src/main.rs                       Worker::new(); registers scalars + tables
  src/parsing.rs                    PURE logic (no Arrow): language registry, parse,
                                    symbols/imports/comments/strings/loc/queries + unit tests
  src/arrow_io.rs                   VARCHAR cell reads + LIST(VARCHAR) builder + in-process scalar harness
  src/scalar/{language,counts,lists,query,version,mod}.rs   thin Arrow scalar adapters
  src/table/{symbols,ts_nodes,supported_languages,mod}.rs   thin Arrow table-producer adapters
  tests/analyze.rs                  integration tests over `parsing` across rust/python/js/go
test/sql/*.test                     haybarn-unittest sqllogictest — authoritative E2E
Makefile                            test / test-unit / test-sql / lint / fmt / build / clean
```

Pattern: keep computation in `parsing.rs` (pure, unit-tested), keep Arrow
marshalling in `arrow_io.rs` + `scalar/*.rs` + `table/*.rs` (thin, harness-tested).

## Library: tree-sitter + grammars

`tree-sitter` 0.25 is the parsing core. Each grammar crate exposes a
`LANGUAGE: LanguageFn` (or `LANGUAGE_TYPESCRIPT` for TS) constant; `parsing.rs`
maps the `Lang` enum to the grammar via `.into()`.

Per-language tree-sitter **queries** drive every extraction:
- `symbols_query` captures `@<kind>.def` (the whole definition node, for the line
  span) sibling-paired with `@<kind>.name` (the identifier). `parsing::symbols`
  pairs them and normalizes kinds to function/method/class/struct/enum/
  interface/trait.
- `imports_query` / `comments_query` / `strings_query` capture the relevant
  nodes; their text is returned as a `VARCHAR[]`.
- `ts_query` / `ts_nodes` run a *user-supplied* query — the power feature.

## Sharp edges (learned the hard way)

1. **tree-sitter ABI / version pinning (the fiddly one).** The grammar crates
   are at *divergent* versions (rust 0.24, python/js/go 0.25, ts/java/cpp 0.23,
   c/json 0.24), yet they all link against **one** `tree-sitter` core. This works
   because every grammar — and the core — depends only on the ABI-stable
   `tree-sitter-language ^0.1` shim (resolved to `0.1.7`), which defines the
   `LanguageFn` type the grammars export. So the core version is chosen
   independently: we pin `tree-sitter = "0.25"` (resolves to **0.25.10**), whose
   MSRV (1.76) is under the workspace `rust-version = 1.86`. (0.26 also works; we
   stay on the well-worn 0.25 line.) Do NOT try to match grammar version numbers
   to the core — they intentionally diverge.

2. **`tree-sitter` 0.25 query matches are a `streaming-iterator`.** `QueryCursor::
   matches(...)` returns a `StreamingIterator`, not a plain `Iterator`. Iterate it
   with `while let Some(m) = matches.next()` (the `streaming_iterator::Streaming
   Iterator` trait is imported in `parsing.rs`). Hence the explicit
   `streaming-iterator` dependency.

3. **`haybarn-unittest` skips `require vgi`** — `.test` files use explicit
   `statement ok` + `LOAD vgi;`. Functions live under the `code` catalog, so each
   file does `SET search_path = 'code.main'`, then `USE memory` before
   `DETACH code`.

4. **LIST(VARCHAR) return type must match between bind and process.** The Arrow
   `DataType::List` published in `on_bind` must exactly equal the array built in
   `process` (child field name `item`, nullable). Both go through
   `arrow_io::list_varchar_type()` / `list_builder()` so they cannot drift — same
   discipline `vgi-image` uses for its MAP/STRUCT returns.

5. **Scalars are positional-only; table functions take constants.** The scalar
   `language`/`query` arguments are per-row VARCHAR *columns* (DuckDB constant-
   folds a literal), so one call can mix languages across rows. The table
   functions (`symbols`, `ts_nodes`) take bind-time *constant* `source`/`language`
   /`query`, passed positionally — they read them via `const_str(i)` and validate
   the language/query at `on_bind` for a clear early error. (The SDK registers
   these as positional `const_arg`s; DuckDB's binder rejects a `name := value`
   call form for them, so the `.test` suite calls them positionally.)

6. **Robustness.** Input is bounded (`MAX_SOURCE_BYTES = 16 MiB`). tree-sitter
   recovers from any malformed source, so extraction is best-effort: garbage in →
   few/no symbols, empty lists, no panic. The only *hard* errors are an unknown
   language and a malformed user query — both caller mistakes, surfaced clearly.
   NULL in → NULL out / no rows.

## Testing

```sh
cargo test --workspace                                   # pure unit + arrow-boundary harness + integration
cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all -- --check
make test-sql                                            # builds release, sets VGI_CODE_WORKER, haybarn over test/sql/*
make test                                                # cargo test + sql
```

CI (`.github/workflows/ci.yml`) runs fmt/clippy/build/test plus a gated
`e2e-sql` job (installs `uv` + `haybarn-unittest`, runs `make test-sql`).

## Function surface

Scalars: `language_of`, `count_lines`, `loc`, `count_functions`,
`extract_imports`, `extract_comments`, `extract_strings`, `ts_query`,
`code_version`. Tables: `symbols`, `ts_nodes`, `supported_languages`. Garbage /
empty / oversized source → graceful empty / NULL / no rows; an unknown language
or malformed query is a clear error.
