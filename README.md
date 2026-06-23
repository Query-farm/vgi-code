<p align="center">
  <img src="https://raw.githubusercontent.com/Query-farm/vgi/main/docs/vgi-logo.png" alt="Vector Gateway Interface (VGI)" width="320">
</p>

<p align="center"><em>A <a href="https://query.farm">Query.Farm</a> VGI worker for DuckDB.</em></p>

# vgi-code

A [VGI](https://query.farm) worker (Rust, a compiled binary) that brings
**source-code structure** — symbols, imports, comments, string literals, line
counts, and arbitrary [tree-sitter](https://tree-sitter.github.io/tree-sitter/)
queries — to DuckDB / SQL over Apache Arrow. DuckDB launches the worker and talks
to it over Arrow IPC; the functions appear under the catalog `code`, schema
`main`.

Parsing is powered by [tree-sitter](https://crates.io/crates/tree-sitter) and a
curated set of grammar crates. Source is treated as **text** (never executed),
and tree-sitter is an **error-recovering** parser, so malformed input is parsed
best-effort and never crashes the worker.

```sql
LOAD vgi;
ATTACH 'code' (TYPE vgi, LOCATION './target/release/code-worker');
SET search_path = 'code.main';

-- Infer a language from a filename.
SELECT language_of('main.rs');                 -- → 'rust'

-- Per-row analysis of a source column.
SELECT path,
       count_functions(content, language_of(path)) AS fns,
       loc(content, language_of(path))            AS code_lines
FROM files;

-- One import per row.
SELECT UNNEST(extract_imports(content, 'python')) FROM files;

-- The power feature: run any tree-sitter query.
SELECT UNNEST(ts_query(content, 'rust',
       '(function_item name: (identifier) @n)')) FROM files;

-- Structural symbols of one document (table function).
SELECT * FROM symbols('fn add(a:i32,b:i32)->i32{a+b}', 'rust');
-- kind     | name | start_line | end_line
-- function | add  |          1 |        1

-- Arbitrary query matches as rows.
SELECT * FROM ts_nodes('func A(){}', 'go', '(function_declaration) @f');

-- Discover what the worker can parse.
SELECT * FROM supported_languages();
```

## Functions

### Scalar (positional-only)

| Function | Returns | Description |
| --- | --- | --- |
| `language_of(filename)` | `VARCHAR` | Language inferred from the file extension (`NULL` if unknown). |
| `count_lines(source)` | `INT` | Total physical lines. |
| `loc(source, language)` | `INT` | Lines of code: non-blank, non-comment lines. |
| `count_functions(source, language)` | `INT` | Function / method definition count. |
| `extract_imports(source, language)` | `VARCHAR[]` | Import / use / require statements. |
| `extract_comments(source, language)` | `VARCHAR[]` | Comment texts. |
| `extract_strings(source, language)` | `VARCHAR[]` | String-literal texts. |
| `ts_query(source, language, query)` | `VARCHAR[]` | Captured node texts of a tree-sitter query. |
| `code_version()` | `VARCHAR` | Worker version string. |

### Table (constant arguments, passed positionally)

| Function | Columns | Description |
| --- | --- | --- |
| `symbols(source, language)` | `kind VARCHAR, name VARCHAR, start_line INT, end_line INT` | Functions, classes, methods, structs, enums, … |
| `ts_nodes(source, language, query)` | `seq BIGINT, capture VARCHAR, text VARCHAR, start_line INT, end_line INT` | Every capture of a tree-sitter query. |
| `supported_languages()` | `language VARCHAR` | Language ids the worker can parse. |

> DuckDB table functions take **constant** arguments (no subqueries), so the
> `source` passed to `symbols` / `ts_nodes` must be a constant-foldable
> expression (a string literal). For per-row analysis of a column, use the
> scalar functions (`count_functions`, `extract_imports`, `ts_query`, …).

## Supported languages

`rust`, `python`, `javascript`, `typescript`, `go`, `java`, `c`, `cpp`, `json`.
The `language` argument is a string id (common aliases like `py`, `js`, `c++`
are accepted). An unknown language is a clear error.

## Behavior & robustness

* Source is **text**, never executed.
* tree-sitter is an **error-recovering** parser: malformed / truncated source is
  parsed best-effort and yields whatever structure can be found — never a crash.
* Input is **bounded** (16 MiB per source) to guard against pathological input.
* `NULL` input → `NULL` output (scalars) or no rows (table functions).
* An **unknown language** or a **malformed tree-sitter query** is a clear DuckDB
  error (both are caller mistakes, not untrusted data).

## Building & testing

```sh
cargo build --release                                    # build the worker
cargo test --workspace                                   # unit + integration tests
cargo clippy --all-targets --all-features -- -D warnings # lint
cargo fmt --all -- --check                               # format check
make test-sql                                            # DuckDB SQL end-to-end
```

`make test-sql` builds the release worker, points `VGI_CODE_WORKER` at it, and
runs the [`haybarn-unittest`](https://pypi.org/project/haybarn-unittest/)
sqllogictest suite under `test/sql/`. Install the runner once with
`uv tool install haybarn-unittest`.

## Licensing of dependencies

The worker is MIT (see [LICENSE](LICENSE)). `tree-sitter` and every grammar crate
(`tree-sitter-rust`, `-python`, `-javascript`, `-typescript`, `-go`, `-java`,
`-c`, `-cpp`, `-json`) are MIT-licensed (a few are dual MIT/Apache-2.0), all
compatible with this project's MIT license.

## License

MIT — see [LICENSE](LICENSE).

---

## Authorship & License

Written by [Query.Farm](https://query.farm) — every VGI worker is designed and built by Query.Farm.

Copyright 2026 Query Farm LLC - https://query.farm

