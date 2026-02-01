#!/usr/bin/env python3
"""Aggregate benchmark results and generate comparison reports.

Reads individual run results from bench/results/raw/, computes
per-task per-condition statistics, and generates comparison tables
and CSV output.
"""

import argparse
import csv
import json
import statistics
import sys
from pathlib import Path

BENCH_DIR = Path(__file__).parent
RESULTS_DIR = BENCH_DIR / "results" / "raw"
REPORTS_DIR = BENCH_DIR / "results" / "reports"

METRICS = [
    ("total_input_tokens", "Total In Tokens", "{:.0f}"),
    ("input_tokens", "  Non-cached", "{:.0f}"),
    ("cache_read_input_tokens", "  Cache Read", "{:.0f}"),
    ("cache_creation_input_tokens", "  Cache Create", "{:.0f}"),
    ("output_tokens", "Output Tokens", "{:.0f}"),
    ("total_cost_usd", "Cost (USD)", "{:.5f}"),
    ("num_turns", "Turns", "{:.1f}"),
    ("duration_ms", "Duration (ms)", "{:.0f}"),
    ("quality_score", "Quality (0-10)", "{:.1f}"),
    ("keyword_hits", "Keyword Hits", "{:.1f}"),
]


def load_results() -> list[dict]:
    """Load all individual result files."""
    results = []
    for path in sorted(RESULTS_DIR.glob("*.json")):
        if path.name == "all_results.json":
            continue
        with open(path) as f:
            data = json.load(f)
        if "error" in data:
            continue
        # Compute total input tokens (non-cached + cache read + cache creation)
        data["total_input_tokens"] = (
            data.get("input_tokens", 0)
            + data.get("cache_read_input_tokens", 0)
            + data.get("cache_creation_input_tokens", 0)
        )
        results.append(data)
    return results


def compute_stats(values: list[float]) -> dict:
    """Compute mean, median, stdev for a list of values."""
    if not values:
        return {"mean": 0, "median": 0, "stdev": 0, "n": 0}
    return {
        "mean": statistics.mean(values),
        "median": statistics.median(values),
        "stdev": statistics.stdev(values) if len(values) > 1 else 0,
        "n": len(values),
    }


def aggregate(results: list[dict]) -> list[dict]:
    """Group results by (task_id, condition) and compute statistics."""
    grouped: dict[tuple[str, str], list[dict]] = {}
    for r in results:
        key = (r["task_id"], r["condition"])
        grouped.setdefault(key, []).append(r)

    report = []
    for (task_id, condition), runs in sorted(grouped.items()):
        entry = {
            "task_id": task_id,
            "condition": condition,
            "n_runs": len(runs),
        }

        for metric_key, _, _ in METRICS:
            values = [r.get(metric_key, 0) for r in runs if r.get(metric_key) is not None]
            # Filter out -1 scores (failed judge)
            if metric_key == "quality_score":
                values = [v for v in values if v >= 0]
            stats = compute_stats(values)
            entry[f"{metric_key}_mean"] = stats["mean"]
            entry[f"{metric_key}_median"] = stats["median"]
            entry[f"{metric_key}_stdev"] = stats["stdev"]

        # Total tokens (input + output)
        total_tokens = [
            r.get("input_tokens", 0) + r.get("output_tokens", 0) for r in runs
        ]
        entry["total_tokens_mean"] = statistics.mean(total_tokens) if total_tokens else 0

        report.append(entry)

    return report


def print_comparison(report: list[dict]):
    """Print a side-by-side comparison table to stdout."""
    tasks = sorted(set(r["task_id"] for r in report))
    conditions = sorted(set(r["condition"] for r in report))

    if len(conditions) < 2:
        print("Need at least 2 conditions for comparison.")
        print_single_condition(report)
        return

    c1, c2 = conditions[0], conditions[1]

    header = f"{'Task':<25} | {'Metric':<16} | {c1:>14} | {c2:>14} | {'Delta':>12} | {'%':>8}"
    sep = "-" * len(header)

    print("\n" + "=" * len(header))
    print("BENCHMARK COMPARISON REPORT")
    print("=" * len(header))
    print(header)
    print(sep)

    # Per-task metrics
    for task in tasks:
        r1 = next((r for r in report if r["task_id"] == task and r["condition"] == c1), None)
        r2 = next((r for r in report if r["task_id"] == task and r["condition"] == c2), None)

        if not r1 or not r2:
            continue

        first_metric = True
        for metric_key, label, fmt in METRICS:
            v1 = r1.get(f"{metric_key}_mean", 0)
            v2 = r2.get(f"{metric_key}_mean", 0)

            if v1 is None and v2 is None:
                continue

            v1 = v1 or 0
            v2 = v2 or 0
            delta = v2 - v1
            pct = (delta / v1 * 100) if v1 != 0 else 0

            task_col = task if first_metric else ""
            first_metric = False

            print(
                f"{task_col:<25} | {label:<16} | "
                f"{fmt.format(v1):>14} | {fmt.format(v2):>14} | "
                f"{fmt.format(delta):>12} | {pct:>+7.1f}%"
            )

        # Also show total tokens
        t1 = r1.get("total_tokens_mean", 0)
        t2 = r2.get("total_tokens_mean", 0)
        delta = t2 - t1
        pct = (delta / t1 * 100) if t1 != 0 else 0
        print(
            f"{'':25} | {'Total Tokens':<16} | "
            f"{t1:>14.0f} | {t2:>14.0f} | "
            f"{delta:>12.0f} | {pct:>+7.1f}%"
        )
        print(sep)

    # Summary row (averages across all tasks)
    print()
    print("AVERAGES ACROSS ALL TASKS:")
    print(sep)

    for metric_key, label, fmt in METRICS:
        vals1 = [
            r.get(f"{metric_key}_mean", 0)
            for r in report
            if r["condition"] == c1 and r.get(f"{metric_key}_mean") is not None
        ]
        vals2 = [
            r.get(f"{metric_key}_mean", 0)
            for r in report
            if r["condition"] == c2 and r.get(f"{metric_key}_mean") is not None
        ]

        if not vals1 or not vals2:
            continue

        avg1 = statistics.mean(vals1)
        avg2 = statistics.mean(vals2)
        delta = avg2 - avg1
        pct = (delta / avg1 * 100) if avg1 != 0 else 0

        print(
            f"{'AVERAGE':<25} | {label:<16} | "
            f"{fmt.format(avg1):>14} | {fmt.format(avg2):>14} | "
            f"{fmt.format(delta):>12} | {pct:>+7.1f}%"
        )

    print(sep)
    print()


def print_single_condition(report: list[dict]):
    """Print results for a single condition."""
    print(f"\n{'Task':<25} | {'Turns':>6} | {'Tokens':>10} | {'Cost':>10} | {'Time(s)':>8} | {'Quality':>8}")
    print("-" * 80)
    for r in report:
        print(
            f"{r['task_id']:<25} | "
            f"{r.get('num_turns_mean', 0):>6.1f} | "
            f"{r.get('total_tokens_mean', 0):>10.0f} | "
            f"${r.get('total_cost_usd_mean', 0):>9.5f} | "
            f"{r.get('duration_ms_mean', 0) / 1000:>8.1f} | "
            f"{r.get('quality_score_mean', -1):>8.1f}"
        )


def write_csv(report: list[dict]):
    """Write results to CSV."""
    REPORTS_DIR.mkdir(parents=True, exist_ok=True)
    path = REPORTS_DIR / "summary.csv"

    if not report:
        return

    fieldnames = sorted(report[0].keys())
    with open(path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(report)

    print(f"CSV written to {path}")


def write_json(report: list[dict]):
    """Write full report as JSON."""
    REPORTS_DIR.mkdir(parents=True, exist_ok=True)
    path = REPORTS_DIR / "full_report.json"
    with open(path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"JSON written to {path}")


def main():
    parser = argparse.ArgumentParser(description="Benchmark Results Analyzer")
    parser.add_argument("--no-csv", action="store_true", help="Skip CSV output")
    parser.add_argument("--no-json", action="store_true", help="Skip JSON output")

    args = parser.parse_args()

    results = load_results()
    if not results:
        print("No results found in bench/results/raw/", file=sys.stderr)
        sys.exit(1)

    print(f"Loaded {len(results)} results")

    report = aggregate(results)
    print_comparison(report)

    if not args.no_csv:
        write_csv(report)
    if not args.no_json:
        write_json(report)


if __name__ == "__main__":
    main()
