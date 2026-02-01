#!/usr/bin/env python3
"""Generate benchmark visualizations for gnapsis vs baseline comparison."""

import json
import os

import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import numpy as np
import pandas as pd
import seaborn as sns

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DATA_PATH = os.path.join(SCRIPT_DIR, "..", "..", "..", "bench", "results", "reports", "full_report.json")
OUT_DIR = SCRIPT_DIR

TASK_SHORT = {
    "T1-architecture-layers": "T1\nArchitecture",
    "T2-dependency-trace": "T2\nDep. Trace",
    "T3-error-propagation": "T3\nError Prop.",
    "T4-impact-analysis": "T4\nImpact",
    "T5-command-pattern": "T5\nCommand",
    "T6-bfs-algorithm": "T6\nBFS",
    "T7-find-duplication": "T7\nDuplication",
}

DESIGN_TASKS = {"T1-architecture-layers", "T4-impact-analysis", "T7-find-duplication"}

PALETTE = {"Baseline": "#5B8DEF", "Gnapsis": "#F0883E"}
CONDITION_MAP = {"baseline": "Baseline", "with-gnapsis": "Gnapsis"}


def load_data() -> pd.DataFrame:
    with open(DATA_PATH) as f:
        raw = json.load(f)
    df = pd.DataFrame(raw)
    df["condition"] = df["condition"].map(CONDITION_MAP)
    df["task_short"] = df["task_id"].map(TASK_SHORT)
    df["duration_s"] = df["duration_ms_mean"] / 1000
    df["category"] = df["task_id"].apply(
        lambda t: "Design" if t in DESIGN_TASKS else "Implementation"
    )
    return df


def style_ax(ax, ylabel, title=None):
    ax.set_xlabel("")
    ax.set_ylabel(ylabel, fontsize=11)
    if title:
        ax.set_title(title, fontsize=13, fontweight="bold", pad=10)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.tick_params(axis="x", labelsize=9)
    ax.tick_params(axis="y", labelsize=9)


def add_category_spans(ax, df):
    """Add subtle background shading for design vs implementation tasks."""
    tasks = list(dict.fromkeys(df["task_id"]))  # preserve order
    design_idx = [i for i, t in enumerate(tasks) if t in DESIGN_TASKS]
    impl_idx = [i for i, t in enumerate(tasks) if t not in DESIGN_TASKS]

    for idx_list, color, label in [
        (design_idx, "#FFF3E0", "Design tasks"),
        (impl_idx, "#E3F2FD", "Impl. tasks"),
    ]:
        if not idx_list:
            continue
        for i in idx_list:
            ax.axvspan(i - 0.45, i + 0.45, alpha=0.4, color=color, zorder=0)


def plot_quality(df):
    fig, ax = plt.subplots(figsize=(10, 5))
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="quality_score_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor="white", linewidth=0.8,
    )
    add_category_spans(ax, df)
    ax.set_ylim(7, 10.5)
    ax.yaxis.set_major_locator(mticker.MultipleLocator(0.5))
    style_ax(ax, "Quality Score (0-10)", "Quality: LLM Judge Scores")
    ax.legend(title="", loc="lower left", frameon=True, fontsize=10)

    # Annotate deltas
    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["quality_score_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["quality_score_mean"].values[0]
        if gnap > base:
            delta = ((gnap - base) / base) * 100
            ax.annotate(
                f"+{delta:.0f}%", xy=(i, max(base, gnap) + 0.1),
                ha="center", va="bottom", fontsize=8, fontweight="bold", color="#2E7D32",
            )

    fig.tight_layout()
    fig.savefig(os.path.join(OUT_DIR, "quality.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> quality.png")


def plot_duration(df):
    fig, ax = plt.subplots(figsize=(10, 5))
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="duration_s", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor="white", linewidth=0.8,
    )
    add_category_spans(ax, df)
    style_ax(ax, "Duration (seconds)", "Duration: Time to Complete")
    ax.legend(title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["duration_s"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["duration_s"].values[0]
        if gnap < base:
            delta = ((base - gnap) / base) * 100
            ax.annotate(
                f"-{delta:.0f}%", xy=(i, gnap + 1),
                ha="center", va="bottom", fontsize=8, fontweight="bold", color="#2E7D32",
            )

    fig.tight_layout()
    fig.savefig(os.path.join(OUT_DIR, "duration.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> duration.png")


def plot_cost(df):
    fig, ax = plt.subplots(figsize=(10, 5))
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="total_cost_usd_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor="white", linewidth=0.8,
    )
    add_category_spans(ax, df)
    ax.yaxis.set_major_formatter(mticker.FormatStrFormatter("$%.2f"))
    style_ax(ax, "Cost (USD)", "Cost per Task")
    ax.legend(title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["total_cost_usd_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["total_cost_usd_mean"].values[0]
        delta_pct = ((gnap - base) / base) * 100
        color = "#2E7D32" if delta_pct < 0 else "#C62828"
        sign = "" if delta_pct < 0 else "+"
        ax.annotate(
            f"{sign}{delta_pct:.0f}%", xy=(i, max(base, gnap) + 0.005),
            ha="center", va="bottom", fontsize=7, fontweight="bold", color=color,
        )

    fig.tight_layout()
    fig.savefig(os.path.join(OUT_DIR, "cost.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> cost.png")


def plot_turns(df):
    fig, ax = plt.subplots(figsize=(10, 5))
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="num_turns_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor="white", linewidth=0.8,
    )
    add_category_spans(ax, df)
    style_ax(ax, "Turns (API round-trips)", "Turns: Agent Effort")
    ax.legend(title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["num_turns_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["num_turns_mean"].values[0]
        if abs(gnap - base) > 0.5:
            delta = ((gnap - base) / base) * 100
            color = "#2E7D32" if delta < 0 else "#C62828"
            sign = "" if delta < 0 else "+"
            ax.annotate(
                f"{sign}{delta:.0f}%",
                xy=(i, max(base, gnap) + 0.3),
                ha="center", va="bottom", fontsize=8, fontweight="bold", color=color,
            )

    fig.tight_layout()
    fig.savefig(os.path.join(OUT_DIR, "turns.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> turns.png")


def plot_summary(df):
    """2x2 panel: design vs implementation comparison."""
    design = df[df["category"] == "Design"]
    impl = df[df["category"] == "Implementation"]

    metrics = [
        ("quality_score_mean", "Quality Score", "higher is better"),
        ("duration_s", "Duration (s)", "lower is better"),
        ("total_cost_usd_mean", "Cost (USD)", "lower is better"),
        ("num_turns_mean", "Turns", "lower is better"),
    ]

    summary_data = []
    for cat_name, cat_df in [("Design\nTasks", design), ("Implementation\nTasks", impl)]:
        for cond in ["Baseline", "Gnapsis"]:
            cond_df = cat_df[cat_df["condition"] == cond]
            summary_data.append({
                "Category": cat_name,
                "Condition": cond,
                "Quality": cond_df["quality_score_mean"].mean(),
                "Duration (s)": cond_df["duration_s"].mean(),
                "Cost (USD)": cond_df["total_cost_usd_mean"].mean(),
                "Turns": cond_df["num_turns_mean"].mean(),
            })

    sdf = pd.DataFrame(summary_data)

    fig, axes = plt.subplots(2, 2, figsize=(10, 8))
    fig.suptitle("Gnapsis: Design Tasks vs Implementation Tasks", fontsize=14, fontweight="bold", y=0.98)

    plot_configs = [
        ("Quality", "Quality (0-10)", axes[0, 0]),
        ("Duration (s)", "Duration (seconds)", axes[0, 1]),
        ("Cost (USD)", "Cost (USD)", axes[1, 0]),
        ("Turns", "Turns", axes[1, 1]),
    ]

    for col, ylabel, ax in plot_configs:
        sns.barplot(
            data=sdf, x="Category", y=col, hue="Condition",
            palette=PALETTE, ax=ax, edgecolor="white", linewidth=0.8,
        )
        style_ax(ax, ylabel)
        ax.legend(title="", fontsize=9, loc="upper right" if col != "Quality" else "lower left")

        # Annotate delta
        for i, cat in enumerate(sdf["Category"].unique()):
            base_val = sdf[(sdf["Category"] == cat) & (sdf["Condition"] == "Baseline")][col].values[0]
            gnap_val = sdf[(sdf["Category"] == cat) & (sdf["Condition"] == "Gnapsis")][col].values[0]
            if base_val == 0:
                continue
            delta = ((gnap_val - base_val) / base_val) * 100
            # For quality, positive is good; for others, negative is good
            if col == "Quality":
                color = "#2E7D32" if delta > 0 else "#C62828"
            else:
                color = "#2E7D32" if delta < 0 else "#C62828"
            sign = "+" if delta > 0 else ""
            ax.annotate(
                f"{sign}{delta:.0f}%",
                xy=(i, max(base_val, gnap_val) * 1.03),
                ha="center", va="bottom", fontsize=9, fontweight="bold", color=color,
            )

    fig.tight_layout(rect=[0, 0, 1, 0.95])
    fig.savefig(os.path.join(OUT_DIR, "summary.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> summary.png")


def plot_token_breakdown(df):
    """Stacked bar chart showing token composition per task."""
    fig, ax = plt.subplots(figsize=(10, 5))

    tasks = list(dict.fromkeys(df["task_id"]))
    x = np.arange(len(tasks))
    width = 0.35

    for offset, cond, label in [(-width / 2, "Baseline", "Baseline"), (width / 2, "Gnapsis", "Gnapsis")]:
        cache_read = []
        cache_create = []
        uncached = []
        for task in tasks:
            row = df[(df["task_id"] == task) & (df["condition"] == cond)].iloc[0]
            cache_read.append(row["cache_read_input_tokens_mean"] / 1000)
            cache_create.append(row["cache_creation_input_tokens_mean"] / 1000)
            uncached.append(row["input_tokens_mean"] / 1000)

        color_base = PALETTE[label]
        ax.bar(x + offset, cache_read, width, label=f"{label} - Cache Read" if label == "Baseline" else None,
               color=color_base, alpha=0.4)
        ax.bar(x + offset, cache_create, width, bottom=cache_read,
               label=f"{label} - Cache Write" if label == "Baseline" else None,
               color=color_base, alpha=0.7)
        bottoms = [a + b for a, b in zip(cache_read, cache_create)]
        ax.bar(x + offset, uncached, width, bottom=bottoms,
               label=f"{label} - Uncached" if label == "Baseline" else None,
               color=color_base, alpha=1.0)

    ax.set_xticks(x)
    ax.set_xticklabels([TASK_SHORT[t] for t in tasks], fontsize=9)
    style_ax(ax, "Input Tokens (K)", "Token Composition by Task")

    # Custom legend
    from matplotlib.patches import Patch
    legend_elements = [
        Patch(facecolor=PALETTE["Baseline"], alpha=1.0, label="Baseline"),
        Patch(facecolor=PALETTE["Gnapsis"], alpha=1.0, label="Gnapsis"),
        Patch(facecolor="gray", alpha=0.4, label="Cache Read"),
        Patch(facecolor="gray", alpha=0.7, label="Cache Write"),
        Patch(facecolor="gray", alpha=1.0, label="Uncached"),
    ]
    ax.legend(handles=legend_elements, loc="upper right", fontsize=8, ncol=2)

    fig.tight_layout()
    fig.savefig(os.path.join(OUT_DIR, "tokens.png"), dpi=150, bbox_inches="tight")
    plt.close(fig)
    print("  -> tokens.png")


def main():
    sns.set_theme(style="whitegrid", font_scale=1.0)
    plt.rcParams["figure.facecolor"] = "white"

    print("Loading data...")
    df = load_data()

    print("Generating charts:")
    plot_quality(df)
    plot_duration(df)
    plot_cost(df)
    plot_turns(df)
    plot_summary(df)
    plot_token_breakdown(df)
    print(f"\nAll charts saved to {OUT_DIR}/")


if __name__ == "__main__":
    main()
