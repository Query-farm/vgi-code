# CI: the vgi-code worker integration suite

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs fmt/clippy/build,
the Rust unit + integration tests, and this repo's sqllogictest suite
(`test/sql/*.test`) against the vgi-code VGI worker through the **real DuckDB
`vgi` extension** on every push / PR.

## How it works (no C++ build)

Rather than building the vgi DuckDB extension from source, the integration job
drives a **prebuilt** standalone `haybarn-unittest` (the DuckDB/Haybarn
sqllogictest runner, published in Haybarn's releases) and installs the
**signed** `vgi` extension from the Haybarn community channel:

1. **Build the worker** — `cargo build --release --bin code-worker`. The
   compiled `target/release/code-worker` is a self-contained stdio worker the
   extension spawns (the `.test` files `ATTACH` it via `${VGI_CODE_WORKER}`).
2. **Download the runner** — the matching `haybarn_unittest-*` asset per
   platform from the latest Haybarn release.
3. **Preprocess** — the standalone runner links none of the extensions the
   tests gate on, so [`preprocess-require.awk`](preprocess-require.awk) rewrites
   each `require <ext>` into an explicit signed `INSTALL <ext> FROM
   {community,core}; LOAD <ext>;`. `require-env` and everything else pass
   through untouched.
4. **Run** — [`run-integration.sh`](run-integration.sh) stages the preprocessed
   tree, points `VGI_CODE_WORKER` at the release binary, warms the
   extension cache once (`INSTALL vgi FROM community;` — this is what makes the
   tests' explicit `LOAD vgi;` succeed), then runs the suite in a single
   `haybarn-unittest` invocation. Any failed assertion exits non-zero and
   fails the job.

## Run it locally

```bash
cargo build --release --bin code-worker
HAYBARN_UNITTEST=/path/to/haybarn-unittest \
VGI_CODE_WORKER="$PWD/target/release/code-worker" \
  ci/run-integration.sh
```

Or use the Makefile target `make test-sql`, which builds the release worker and
runs the suite against a `haybarn-unittest` on `PATH` (`uv tool install
haybarn-unittest`).
