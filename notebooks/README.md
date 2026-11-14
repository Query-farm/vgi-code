# Source-Code Structure Analysis

Two SQL-first examples use the released VGI Code worker to analyze small, visible
Python source fixtures. The fixtures are text only and are never executed.

| Notebook | Question | Experiment |
| --- | --- | --- |
| [Function spans and a review queue](01_review_functions_without_double_counting_lines.ipynb) | How do nested definitions change line accounting? | Raise `MIN_SPAN_LINES` from 4 to 8. |
| [Import syntax and text decoys](02_import_syntax_and_text_decoys.ipynb) | Which import-shaped strings are actual statements? | Set `INCLUDE_DECOYS` to `False`. |

Each notebook includes an **Open in Colab** link and installs the shared base
package when needed. Downloading a notebook alone is sufficient: its setup fetches
the verified plotting companion. GitHub displays saved tables and sharp SVG
figures; connection SQL and analytical steps stay visible.

To execute locally with [uv](https://docs.astral.sh/uv/):

```sh
cd notebooks
uv sync --locked
uv run --locked vgi-notebooks
```

Select `notebooks/.venv/bin/python` in your notebook editor to run interactively.
The locked local environment includes the shared runner. Colab installs only the
base package, preserving compatible existing kernel and plotting libraries.

The connection uses the published native release:

```sql
INSTALL vgi FROM community;
LOAD vgi;
ATTACH 'code' (TYPE vgi,
  LOCATION 'github-auto://Query-farm/vgi-code@v0.4.0');
```

Haybarn downloads and verifies the release; no Rust compiler or local worker build
is needed. Initial setup needs network access. Source analysis then runs locally.
Execution was verified on macOS arm64; hosted Colab itself was not manually run.
An isolated preloaded-kernel check exercises the one-file bootstrap and preserves
compatible pandas, Matplotlib and Jupyter versions.

Run the experiments into separate directories to retain the saved defaults:

```sh
uv run --locked vgi-notebooks 01_review_functions_without_double_counting_lines.ipynb \
  --parameters '{"MIN_SPAN_LINES":8}' --output-dir artifacts/threshold-8
uv run --locked vgi-notebooks 02_import_syntax_and_text_decoys.ipynb \
  --parameters '{"INCLUDE_DECOYS":false}' --output-dir artifacts/no-decoys
```

In the first story, summed function spans include a nested helper twice. Distinct
physical lines, file length and worker LOC answer different questions. Parser
recovery is checked before selecting a review queue; an absence of recovery nodes
is not a compiler or correctness check. The experiment changes the queue from
three functions to one while leaving source analysis and line accounting fixed.

In the second story, removing a comment and a string literal changes the substring
scan from four lines to two while both actual import statements remain. The
one-line-per-statement fixture makes that comparison valid. Syntax extraction does
not resolve packages or establish which dependencies run.

The examples cover **11/11 logical public SQL surfaces** in release v0.4.0. The
same 11 are mapped to current public source through a retained registration and
metadata-only diff review; actual notebook execution used the release. Complete
catalogs, successful execution receipts and reviewed mappings are in
[coverage.json](coverage.json) and [coverage/](coverage/).

The shared [notebook standard](https://github.com/Query-farm/vgi-example-notebooks/blob/main/docs/standard.md)
owns branding, setup, execution and SVG publication. This directory keeps only
worker-specific stories, the plotting companion and their evidence.
