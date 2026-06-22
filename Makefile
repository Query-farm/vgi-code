# vgi-code worker — dev, test, and lint targets.
#
# Usage:
#   make test         # cargo unit/integration tests + SQL E2E
#   make test-unit    # cargo test --workspace (pure-Rust + Arrow-boundary tests)
#   make test-sql     # build the release worker, run the DuckDB sqllogictest suite
#   make lint         # clippy (deny warnings) + rustfmt --check
#   make fmt          # rustfmt the workspace
#
# The SQL E2E suite drives the compiled worker through DuckDB via
# `haybarn-unittest` (install with: `uv tool install haybarn-unittest`).

# Path to the released worker binary handed to DuckDB as the ATTACH LOCATION.
WORKER         ?= $(CURDIR)/target/release/code-worker
# DuckDB sqllogictest runner (haybarn-unittest; on PATH after `uv tool install`).
SQL_RUNNER     ?= haybarn-unittest
TEST_DIR        = .
TEST_PATTERN    = test/sql/*

.PHONY: test test-unit test-sql lint fmt build clean

# Full local gate: everything CI runs.
test: test-unit test-sql

# Pure-Rust unit + integration tests (includes the in-process Arrow-boundary
# tests for the scalar dispatch/marshalling layer).
test-unit:
	cargo test --workspace --all-features

# Build the release worker, then run the SQL E2E suite against it. The worker is
# a compiled binary, so the build must happen before the tests run.
test-sql: build
	VGI_CODE_WORKER="$(WORKER)" $(SQL_RUNNER) --test-dir "$(TEST_DIR)" "$(TEST_PATTERN)"

# clippy (warnings are errors) + formatting check.
lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

fmt:
	cargo fmt --all

build:
	cargo build --release --bin code-worker

clean:
	cargo clean
