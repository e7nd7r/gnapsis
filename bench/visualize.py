#!/usr/bin/env python3
"""Generate benchmark visualizations for gnapsis vs baseline comparison.

Usage:
    uv run visualize.py --data path/to/full_report.json --out path/to/output/
"""

import argparse
import json
import os

import matplotlib.lines as mlines
import matplotlib.patches as mpatches
import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import numpy as np
import pandas as pd
import seaborn as sns

# -- Tokyo Night palette ---------------------------------------------------- #
BG = "#1a1b26"
BG_HL = "#1f2335"
FG = "#a9b1d6"
FG_DARK = "#8089b3"
COMMENT = "#51597d"
BLUE = "#7aa2f7"
CYAN = "#7dcfff"
GREEN = "#9ece6a"
RED = "#f7768e"
YELLOW = "#e0af68"

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

PALETTE = {"Baseline": COMMENT, "Gnapsis": BLUE}
CONDITION_MAP = {"baseline": "Baseline", "with-gnapsis": "Gnapsis"}


# -- Graph-network background pattern -------------------------------------- #
def draw_graph_pattern(fig, n_nodes=50, seed=42):
    """Draw a subtle graph/network pattern on the figure background."""
    rng = np.random.default_rng(seed)
    xs = rng.uniform(0.02, 0.98, n_nodes)
    ys = rng.uniform(0.02, 0.98, n_nodes)

    edge_threshold = 0.18
    node_alpha = 0.12
    edge_alpha = 0.07
    node_color = COMMENT
    edge_color = COMMENT

    # Draw edges first (behind nodes)
    for i in range(n_nodes):
        for j in range(i + 1, n_nodes):
            dist = np.hypot(xs[i] - xs[j], ys[i] - ys[j])
            if dist < edge_threshold:
                line = mlines.Line2D(
                    [xs[i], xs[j]], [ys[i], ys[j]],
                    transform=fig.transFigure, color=edge_color,
                    alpha=edge_alpha, linewidth=0.6, zorder=0,
                )
                fig.add_artist(line)

    # Draw nodes
    for x, y in zip(xs, ys):
        size = rng.uniform(0.003, 0.008)
        circle = mpatches.Circle(
            (x, y), size, transform=fig.transFigure,
            color=node_color, alpha=node_alpha, zorder=0,
        )
        fig.add_artist(circle)


# -- Helpers ---------------------------------------------------------------- #
def load_data(data_path: str) -> pd.DataFrame:
    with open(data_path) as f:
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
    ax.set_ylabel(ylabel, fontsize=10, color=FG)
    if title:
        ax.set_title(title, fontsize=13, fontweight="semibold", pad=12, color="#c0caf5")
    ax.set_facecolor("none")
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.spines["left"].set_color(COMMENT)
    ax.spines["bottom"].set_color(COMMENT)
    ax.tick_params(axis="x", labelsize=9, colors=FG_DARK)
    ax.tick_params(axis="y", labelsize=9, colors=FG_DARK)
    ax.yaxis.grid(True, color=BG_HL, linewidth=0.8)
    ax.set_axisbelow(True)


def add_category_spans(ax, df):
    """Add subtle background shading for design vs implementation tasks."""
    tasks = list(dict.fromkeys(df["task_id"]))
    design_idx = [i for i, t in enumerate(tasks) if t in DESIGN_TASKS]
    impl_idx = [i for i, t in enumerate(tasks) if t not in DESIGN_TASKS]

    for idx_list, color in [(design_idx, YELLOW), (impl_idx, CYAN)]:
        if not idx_list:
            continue
        for i in idx_list:
            ax.axvspan(i - 0.45, i + 0.45, alpha=0.06, color=color, zorder=0)


def dark_legend(ax, **kwargs):
    """Style a legend for dark backgrounds."""
    leg = ax.legend(**kwargs)
    leg.get_frame().set_facecolor(BG)
    leg.get_frame().set_edgecolor("none")
    for text in leg.get_texts():
        text.set_color(FG)
    return leg


def make_figure(figsize=(10, 5)):
    """Create a figure with dark background and graph pattern."""
    fig, ax = plt.subplots(figsize=figsize)
    draw_graph_pattern(fig)
    return fig, ax


def save_figure(fig, out_dir, name):
    """Save figure preserving dark background."""
    fig.tight_layout()
    fig.savefig(
        os.path.join(out_dir, name), dpi=150,
        bbox_inches="tight", facecolor=fig.get_facecolor(),
    )
    plt.close(fig)
    print(f"  -> {name}")


# -- Chart functions -------------------------------------------------------- #
def plot_quality(df, out_dir):
    fig, ax = make_figure()
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="quality_score_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor=BG, linewidth=0.8,
    )
    add_category_spans(ax, df)
    ax.set_ylim(7, 10.5)
    ax.yaxis.set_major_locator(mticker.MultipleLocator(0.5))
    style_ax(ax, "Quality Score (0-10)", "Quality: LLM Judge Scores")
    dark_legend(ax, title="", loc="lower left", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["quality_score_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["quality_score_mean"].values[0]
        if gnap > base and base > 0:
            delta = ((gnap - base) / base) * 100
            ax.annotate(
                f"+{delta:.0f}%", xy=(i, max(base, gnap) + 0.1),
                ha="center", va="bottom", fontsize=8, color=GREEN,
            )

    save_figure(fig, out_dir, "quality.png")


def plot_duration(df, out_dir):
    fig, ax = make_figure()
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="duration_s", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor=BG, linewidth=0.8,
    )
    add_category_spans(ax, df)
    style_ax(ax, "Duration (seconds)", "Duration: Time to Complete")
    dark_legend(ax, title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["duration_s"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["duration_s"].values[0]
        if gnap < base:
            delta = ((base - gnap) / base) * 100
            ax.annotate(
                f"-{delta:.0f}%", xy=(i, gnap + 1),
                ha="center", va="bottom", fontsize=8, color=GREEN,
            )

    save_figure(fig, out_dir, "duration.png")


def plot_cost(df, out_dir):
    fig, ax = make_figure()
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="total_cost_usd_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor=BG, linewidth=0.8,
    )
    add_category_spans(ax, df)
    ax.yaxis.set_major_formatter(mticker.FormatStrFormatter("$%.2f"))
    style_ax(ax, "Cost (USD)", "Cost per Task")
    dark_legend(ax, title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["total_cost_usd_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["total_cost_usd_mean"].values[0]
        if base == 0:
            continue
        delta_pct = ((gnap - base) / base) * 100
        color = GREEN if delta_pct < 0 else RED
        sign = "" if delta_pct < 0 else "+"
        ax.annotate(
            f"{sign}{delta_pct:.0f}%", xy=(i, max(base, gnap) + 0.005),
            ha="center", va="bottom", fontsize=7, color=color,
        )

    save_figure(fig, out_dir, "cost.png")


def plot_turns(df, out_dir):
    fig, ax = make_figure()
    order = list(dict.fromkeys(df["task_short"]))
    sns.barplot(
        data=df, x="task_short", y="num_turns_mean", hue="condition",
        order=order, palette=PALETTE, ax=ax, edgecolor=BG, linewidth=0.8,
    )
    add_category_spans(ax, df)
    style_ax(ax, "Turns (API round-trips)", "Turns: Agent Effort")
    dark_legend(ax, title="", loc="upper right", frameon=True, fontsize=10)

    tasks_order = list(dict.fromkeys(df["task_id"]))
    for i, task in enumerate(tasks_order):
        base = df[(df["task_id"] == task) & (df["condition"] == "Baseline")]["num_turns_mean"].values[0]
        gnap = df[(df["task_id"] == task) & (df["condition"] == "Gnapsis")]["num_turns_mean"].values[0]
        if abs(gnap - base) > 0.5:
            delta = ((gnap - base) / base) * 100
            color = GREEN if delta < 0 else RED
            sign = "" if delta < 0 else "+"
            ax.annotate(
                f"{sign}{delta:.0f}%",
                xy=(i, max(base, gnap) + 0.3),
                ha="center", va="bottom", fontsize=8, color=color,
            )

    save_figure(fig, out_dir, "turns.png")


def plot_summary(df, out_dir):
    """2x2 panel: design vs implementation comparison."""
    design = df[df["category"] == "Design"]
    impl = df[df["category"] == "Implementation"]

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
    draw_graph_pattern(fig, n_nodes=70)
    fig.suptitle(
        "Gnapsis: Design Tasks vs Implementation Tasks",
        fontsize=14, fontweight="semibold", y=0.98, color="#c0caf5",
    )

    plot_configs = [
        ("Quality", "Quality (0-10)", axes[0, 0]),
        ("Duration (s)", "Duration (seconds)", axes[0, 1]),
        ("Cost (USD)", "Cost (USD)", axes[1, 0]),
        ("Turns", "Turns", axes[1, 1]),
    ]

    for col, ylabel, ax in plot_configs:
        sns.barplot(
            data=sdf, x="Category", y=col, hue="Condition",
            palette=PALETTE, ax=ax, edgecolor=BG, linewidth=0.8,
        )
        style_ax(ax, ylabel)
        dark_legend(
            ax, title="", fontsize=9, frameon=True,
            loc="upper right" if col != "Quality" else "lower left",
        )

        for i, cat in enumerate(sdf["Category"].unique()):
            base_val = sdf[(sdf["Category"] == cat) & (sdf["Condition"] == "Baseline")][col].values[0]
            gnap_val = sdf[(sdf["Category"] == cat) & (sdf["Condition"] == "Gnapsis")][col].values[0]
            if base_val == 0:
                continue
            delta = ((gnap_val - base_val) / base_val) * 100
            if col == "Quality":
                color = GREEN if delta > 0 else RED
            else:
                color = GREEN if delta < 0 else RED
            sign = "+" if delta > 0 else ""
            ax.annotate(
                f"{sign}{delta:.0f}%",
                xy=(i, max(base_val, gnap_val) * 1.03),
                ha="center", va="bottom", fontsize=9, color=color,
            )

    fig.tight_layout(rect=[0, 0, 1, 0.95])
    fig.savefig(
        os.path.join(out_dir, "summary.png"), dpi=150,
        bbox_inches="tight", facecolor=fig.get_facecolor(),
    )
    plt.close(fig)
    print("  -> summary.png")


def plot_token_breakdown(df, out_dir):
    """Stacked bar chart showing token composition per task."""
    fig, ax = make_figure()

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
        ax.bar(x + offset, cache_read, width, color=color_base, alpha=0.35)
        ax.bar(x + offset, cache_create, width, bottom=cache_read, color=color_base, alpha=0.65)
        bottoms = [a + b for a, b in zip(cache_read, cache_create)]
        ax.bar(x + offset, uncached, width, bottom=bottoms, color=color_base, alpha=1.0)

    ax.set_xticks(x)
    ax.set_xticklabels([TASK_SHORT[t] for t in tasks], fontsize=9)
    style_ax(ax, "Input Tokens (K)", "Token Composition by Task")

    legend_elements = [
        mpatches.Patch(facecolor=PALETTE["Baseline"], alpha=1.0, label="Baseline"),
        mpatches.Patch(facecolor=PALETTE["Gnapsis"], alpha=1.0, label="Gnapsis"),
        mpatches.Patch(facecolor=FG_DARK, alpha=0.35, label="Cache Read"),
        mpatches.Patch(facecolor=FG_DARK, alpha=0.65, label="Cache Write"),
        mpatches.Patch(facecolor=FG_DARK, alpha=1.0, label="Uncached"),
    ]
    leg = ax.legend(handles=legend_elements, loc="upper right", fontsize=8, ncol=2, frameon=True)
    leg.get_frame().set_facecolor(BG)
    leg.get_frame().set_edgecolor("none")
    for text in leg.get_texts():
        text.set_color(FG)

    save_figure(fig, out_dir, "tokens.png")


# -- Main ------------------------------------------------------------------ #
def main():
    parser = argparse.ArgumentParser(description="Generate benchmark visualizations")
    parser.add_argument("--data", required=True, help="Path to full_report.json")
    parser.add_argument("--out", required=True, help="Output directory for charts")
    args = parser.parse_args()

    sns.set_theme(style="dark", font_scale=1.0)
    plt.rcParams.update({
        "figure.facecolor": BG,
        "axes.facecolor": BG,
        "text.color": FG,
        "axes.labelcolor": FG,
        "xtick.color": FG_DARK,
        "ytick.color": FG_DARK,
        "font.family": "sans-serif",
    })

    os.makedirs(args.out, exist_ok=True)

    print(f"Loading data from {args.data}")
    df = load_data(args.data)

    print(f"Generating charts to {args.out}:")
    plot_quality(df, args.out)
    plot_duration(df, args.out)
    plot_cost(df, args.out)
    plot_turns(df, args.out)
    plot_summary(df, args.out)
    plot_token_breakdown(df, args.out)
    print(f"\nAll charts saved to {args.out}")


if __name__ == "__main__":
    main()
