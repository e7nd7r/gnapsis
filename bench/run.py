#!/usr/bin/env python3
"""Benchmark runner for gnapsis MCP tool effectiveness.

Compares Claude Code performance on code understanding tasks
with and without gnapsis knowledge graph tools available.
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

BENCH_DIR = Path(__file__).parent
PROJECT_DIR = BENCH_DIR.parent
CONFIG_DIR = BENCH_DIR / "config"
TASKS_FILE = BENCH_DIR / "tasks" / "tasks.json"
RESULTS_DIR = BENCH_DIR / "results" / "raw"

CONDITIONS = {
    "baseline": CONFIG_DIR / "mcp-baseline.json",
    "with-gnapsis": CONFIG_DIR / "mcp-with-gnapsis.json",
}

# Append to system prompt to keep answers focused
APPEND_PROMPT = (
    "Answer the question directly and concisely. "
    "Do not ask follow-up questions. "
    "Do not offer to do additional work."
)


def load_tasks(task_ids: list[str] | None = None) -> list[dict]:
    """Load task definitions, optionally filtering by ID."""
    with open(TASKS_FILE) as f:
        tasks = json.load(f)["tasks"]
    if task_ids:
        tasks = [t for t in tasks if t["id"] in task_ids]
    return tasks


def run_task(
    task: dict,
    condition: str,
    run_id: int,
    model: str,
    max_turns: int | None = None,
    dry_run: bool = False,
) -> dict | None:
    """Run a single task under one condition and return metrics."""
    mcp_config = CONDITIONS[condition]
    turns = max_turns or task.get("max_turns", 15)

    cmd = [
        "claude",
        "-p", task["prompt"],
        "--output-format", "json",
        "--strict-mcp-config",
        "--mcp-config", str(mcp_config),
        "--no-session-persistence",
        "--dangerously-skip-permissions",
        "--max-turns", str(turns),
        "--model", model,
        "--append-system-prompt", APPEND_PROMPT,
    ]

    if dry_run:
        print(f"  [DRY RUN] {' '.join(cmd[:6])}... --mcp-config {mcp_config.name}")
        return None

    env = os.environ.copy()
    env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"] = "1"

    start = time.time()
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=600,  # 10-minute safety timeout
            cwd=str(PROJECT_DIR),
            env=env,
        )
    except subprocess.TimeoutExpired:
        print(f"  TIMEOUT after 600s")
        return {
            "task_id": task["id"],
            "condition": condition,
            "run_id": run_id,
            "error": "timeout",
        }

    elapsed = time.time() - start

    if proc.returncode != 0 and not proc.stdout.strip():
        print(f"  ERROR (exit {proc.returncode}): {proc.stderr[:200]}")
        return {
            "task_id": task["id"],
            "condition": condition,
            "run_id": run_id,
            "error": proc.stderr[:500],
        }

    try:
        response = json.loads(proc.stdout)
    except json.JSONDecodeError:
        print(f"  ERROR: Failed to parse JSON output")
        return {
            "task_id": task["id"],
            "condition": condition,
            "run_id": run_id,
            "error": f"JSON parse error: {proc.stdout[:200]}",
        }

    usage = response.get("usage", {})

    result = {
        "task_id": task["id"],
        "condition": condition,
        "run_id": run_id,
        "model": model,
        "num_turns": response.get("num_turns", 0),
        "duration_ms": response.get("duration_ms", 0),
        "duration_api_ms": response.get("duration_api_ms", 0),
        "total_cost_usd": response.get("total_cost_usd", 0),
        "input_tokens": usage.get("input_tokens", 0),
        "output_tokens": usage.get("output_tokens", 0),
        "cache_read_input_tokens": usage.get("cache_read_input_tokens", 0),
        "cache_creation_input_tokens": usage.get("cache_creation_input_tokens", 0),
        "is_error": response.get("is_error", False),
        "session_id": response.get("session_id", ""),
        "result_text": response.get("result", ""),
        "wall_time_s": round(elapsed, 2),
    }

    return result


def save_result(result: dict):
    """Save individual result to disk."""
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    fname = f"{result['task_id']}_{result['condition']}_run{result['run_id']}.json"
    path = RESULTS_DIR / fname
    with open(path, "w") as f:
        json.dump(result, f, indent=2)
    return path


def run_benchmark(
    tasks: list[dict],
    num_runs: int,
    conditions: list[str],
    model: str,
    max_turns: int | None,
    dry_run: bool,
    pause: float,
):
    """Run all tasks under all conditions for multiple runs."""
    total = num_runs * len(tasks) * len(conditions)
    completed = 0

    print(f"\nBenchmark: {len(tasks)} tasks x {len(conditions)} conditions x {num_runs} runs = {total} runs")
    print(f"Model: {model}")
    print(f"Conditions: {', '.join(conditions)}")
    print()

    all_results = []

    for run_id in range(1, num_runs + 1):
        for task in tasks:
            for condition in conditions:
                completed += 1
                tag = f"[{completed}/{total}]"
                print(f"{tag} Run {run_id} | {task['id']} | {condition}")

                result = run_task(task, condition, run_id, model, max_turns, dry_run)

                if result:
                    all_results.append(result)
                    path = save_result(result)

                    if "error" in result:
                        print(f"  -> ERROR: {result['error'][:80]}")
                    else:
                        tokens = result["input_tokens"] + result["output_tokens"]
                        print(
                            f"  -> turns={result['num_turns']} "
                            f"tokens={tokens} "
                            f"cost=${result['total_cost_usd']:.4f} "
                            f"time={result['wall_time_s']}s"
                        )

                    if pause > 0 and completed < total:
                        time.sleep(pause)

    # Save combined results
    if all_results:
        combined = RESULTS_DIR / "all_results.json"
        with open(combined, "w") as f:
            json.dump(all_results, f, indent=2)
        print(f"\nAll results saved to {combined}")

    return all_results


def main():
    parser = argparse.ArgumentParser(
        description="Gnapsis MCP Benchmark Runner",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  python3 bench/run.py --runs 1 --tasks T5-command-pattern    # Quick single test
  python3 bench/run.py --runs 3                                # Full benchmark
  python3 bench/run.py --dry-run                               # Preview commands
  python3 bench/run.py --conditions baseline                   # Baseline only
""",
    )
    parser.add_argument(
        "--runs", type=int, default=3, help="Number of runs per task per condition (default: 3)"
    )
    parser.add_argument(
        "--tasks", nargs="*", help="Run only these task IDs (default: all)"
    )
    parser.add_argument(
        "--conditions",
        nargs="*",
        default=["baseline", "with-gnapsis"],
        choices=list(CONDITIONS.keys()),
        help="Conditions to run (default: baseline with-gnapsis)",
    )
    parser.add_argument(
        "--model", default="sonnet", help="Claude model to use (default: sonnet)"
    )
    parser.add_argument(
        "--max-turns", type=int, default=None, help="Override max turns for all tasks"
    )
    parser.add_argument(
        "--pause", type=float, default=3.0, help="Seconds between runs (default: 3)"
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="Print commands without executing"
    )

    args = parser.parse_args()

    tasks = load_tasks(args.tasks)
    if not tasks:
        print("No tasks found.", file=sys.stderr)
        sys.exit(1)

    print(f"Loaded {len(tasks)} tasks")

    run_benchmark(
        tasks=tasks,
        num_runs=args.runs,
        conditions=args.conditions,
        model=args.model,
        max_turns=args.max_turns,
        dry_run=args.dry_run,
        pause=args.pause,
    )


if __name__ == "__main__":
    main()
