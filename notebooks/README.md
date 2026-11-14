# Source-structure notebooks

[01 · Build a review queue from source structure](01_review_functions_without_double_counting_lines.ipynb)
uses a synthetic Python module to show per-row source analysis, custom tree-sitter
queries, symbol/capture joins and threshold-based review. Two charts show nested
function spans and the chosen review queue. SQL counts unique physical lines so
nested definitions do not inflate the coverage total.

## Run from this directory

Install [uv](https://docs.astral.sh/uv/), then:

```sh
cd notebooks
uv sync --locked
uv run --locked python execute.py
```

In a notebook editor select `notebooks/.venv/bin/python` and run all cells. The
locked environment supports Python 3.13–3.14 and uses Haybarn 2.0.0rc11, JupySQL,
Seaborn and Matplotlib. The complete notebook directory also runs outside the
worker repository; no local source package, Rust compiler or worker build is
used. The source fixture is text only and is never imported or executed.

The connection SQL remains visible:

```sql
INSTALL vgi FROM community;
LOAD vgi;
ATTACH 'code' (TYPE vgi,
 LOCATION 'github-auto://Query-farm/vgi-code@v0.4.0');
```

Haybarn downloads, verifies and caches the native release. Initial installation
needs network access; source analysis is local and offline. The public release
provides macOS arm64 and Linux amd64/arm64 binaries; this notebook was verified
on macOS arm64. Change the visible tag and rerun both thresholds when upgrading.
Parameters and analytical SQL remain expanded; setup, chart implementation and
cleanup are folded with useful outputs visible. Official local Strata Sun assets
appear at the top and final bottom, with the VGI logo and product link near the top.

## One-parameter experiment

`MIN_SPAN_LINES` accepts integers 1–24 inclusive. A function is selected when its
inclusive physical span is at least the threshold. The default is 4; the experiment
raises it to 8 while keeping source, parser results and line accounting fixed.

```sh
uv run --locked python execute.py --parameters '{"MIN_SPAN_LINES":8}' \
  --output-dir validation/threshold-8
```

The runner uses fresh kernels and saves only successful runs. Overrides require
a separate destination; unknown names and source-directory overwrites are rejected.
Experiment copies include their branding, helpers, guide and locked environment.
Logs, extracted charts and copies under `validation/` are ignored; successful
outputs remain embedded in the main notebook.

## Provenance and definitions

All inputs are visible SQL literals: one 24-line teaching Python module, an
incomplete Python fragment, a missing source value and an unrecognized filename
extension. A literal-valued SQL macro makes the complete module reusable as a
constant-foldable argument for table functions. Scalar functions handle the
per-row source inventory; no correlated table-function arguments are used.

The incomplete fragment illustrates error recovery: the parser still returns
function definitions, with a zero-width MISSING capture whose text is empty.
The notebook checks both ERROR and MISSING captures and counts captures rather
than nonempty texts. Unknown language and missing source retain NULL analysis
metrics. Absence of recovery nodes is not a compiler or semantic-validity check.

`symbols` and a custom `ts_nodes` function-definition query supply one-based
start/end lines. The notebook verifies unique spans before joining and checks
that each captured definition's physical line count matches its inclusive span.
A nested definition is also part of its parent's span. SQL expands line ranges
and counts distinct line numbers to separate summed memberships from actual
physical coverage.

Inclusive spans count comments, docstrings, blank lines and nested definitions
within their bounds. Worker LOC counts nonblank, noncomment lines; the fixture's
Python docstrings are string nodes and remain in LOC. File length, unique covered
lines and summed spans are reported separately. A threshold selects candidates
for review and is not a measure of correctness, execution, complexity or developer
productivity. These results describe the small fixed fixture and pinned grammars.

References: [worker documentation](https://github.com/Query-farm/vgi-code),
[pinned release](https://github.com/Query-farm/vgi-code/releases/tag/v0.4.0).
