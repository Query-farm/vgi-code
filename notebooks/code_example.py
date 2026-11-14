"""Plot SQL results from the source-structure examples; no source is executed."""

import matplotlib.pyplot as plt
from matplotlib.patches import Patch
import numpy as np
from vgi_notebooks import finish

SOURCE = "Authored source text · VGI Code v0.4.0 · Tree-sitter structure; source never executed"


def plot_spans(spans, threshold):
    fig, ax = plt.subplots(figsize=(10.8, 5.7))
    y = np.arange(len(spans))
    colors = ["#456fba" if value else "#267358" for value in spans.nested]
    ax.barh(y, spans.span_lines, left=spans.start_line - 0.5, color=colors, height=0.58)
    for i, row in enumerate(spans.itertuples(index=False)):
        ax.text(
            row.start_line + (row.span_lines - 1) / 2,
            i,
            f"{row.start_line}–{row.end_line}",
            ha="center",
            va="center",
            color="white",
            weight="bold",
        )
    ax.set_yticks(y, spans.name)
    ax.invert_yaxis()
    ax.set_xlim(0.5, 24.5)
    ax.set_xticks([1, 4, 8, 12, 16, 20, 24])
    ax.set_xlabel("Physical line number in example.py (one-based)")
    ax.grid(axis="y", visible=False)
    fig.legend(
        handles=[
            Patch(facecolor="#267358", label="Top-level function"),
            Patch(facecolor="#456fba", label="Nested function"),
        ],
        loc="center",
        bbox_to_anchor=(0.55, 0.82),
        ncol=2,
        frameon=False,
    )
    finish(
        fig,
        "Nested definitions share their parent's physical lines",
        "Inclusive function spans · comments, docstrings and blank lines inside the span count",
        SOURCE,
        name=f"function-spans-{threshold}",
    )


def plot_queue(queue, threshold):
    fig, ax = plt.subplots(figsize=(10.8, 5.7))
    colors = ["#267358" if value else "#aab6af" for value in queue.selected]
    ax.barh(queue.name, queue.span_lines, color=colors, height=0.6)
    for i, value in enumerate(queue.span_lines):
        ax.text(value + 0.2, i, str(int(value)), va="center", fontsize=11)
    ax.axvline(threshold, color="#bb722f", linestyle="--", linewidth=2)
    ax.set_xlim(0, max(int(queue.span_lines.max()) + 2, threshold + 2))
    ax.set_xticks(sorted(set([0, 4, 8, 12, threshold])))
    ax.invert_yaxis()
    ax.set_xlabel("Inclusive function span (physical lines)")
    ax.set_ylabel("")
    ax.grid(axis="y", visible=False)
    fig.legend(
        handles=[
            Patch(facecolor="#267358", label="Selected for review"),
            Patch(facecolor="#aab6af", label="Below threshold"),
        ],
        loc="center",
        bbox_to_anchor=(0.55, 0.82),
        ncol=2,
        frameon=False,
    )
    finish(
        fig,
        "A review queue follows the threshold you choose",
        f"Select spans ≥ {threshold} lines · dashed line: chosen threshold · nested spans retained",
        SOURCE,
        name=f"review-queue-{threshold}",
    )


def plot_imports(counts, include_decoys):
    fig, ax = plt.subplots(figsize=(10.8, 5.8))
    bars = ax.barh(
        counts.measure, counts.observed, color=["#879a8d", "#267358", "#c57b40", "#456fba"], height=0.6
    )
    ax.bar_label(bars, padding=7, fontsize=11)
    ax.invert_yaxis()
    ax.set_xlim(0, 5)
    ax.set_xticks(range(6))
    ax.set_xlabel("Count · one import statement per line in this fixture")
    ax.grid(axis="x", visible=True)
    finish(
        fig,
        "An import-shaped line need not be import syntax",
        "One fixed module · comment and string decoys " + ("included" if include_decoys else "removed"),
        SOURCE,
        name=f"import-syntax-{str(include_decoys).lower()}",
    )
