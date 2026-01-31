#!/usr/bin/env python3
"""LLM-as-judge quality evaluator for benchmark results.

Scores each benchmark answer against its task rubric using Claude
as a blind judge (doesn't know which condition produced the answer).
Also computes automated keyword hit counts.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

BENCH_DIR = Path(__file__).parent
PROJECT_DIR = BENCH_DIR.parent
CONFIG_DIR = BENCH_DIR / "config"
TASKS_FILE = BENCH_DIR / "tasks" / "tasks.json"
RESULTS_DIR = BENCH_DIR / "results" / "raw"


def load_tasks() -> dict[str, dict]:
    """Load tasks indexed by ID."""
    with open(TASKS_FILE) as f:
        tasks = json.load(f)["tasks"]
    return {t["id"]: t for t in tasks}


def load_results(task_ids: list[str] | None = None) -> list[tuple[Path, dict]]:
    """Load all result files, optionally filtering by task ID."""
    results = []
    for path in sorted(RESULTS_DIR.glob("*.json")):
        if path.name == "all_results.json":
            continue
        with open(path) as f:
            data = json.load(f)
        if "error" in data:
            continue
        if task_ids and data["task_id"] not in task_ids:
            continue
        results.append((path, data))
    return results


def count_keyword_hits(text: str, keywords: list[str]) -> tuple[int, list[str]]:
    """Count how many expected keywords appear in the answer text."""
    text_lower = text.lower()
    hits = []
    for kw in keywords:
        if kw.lower() in text_lower:
            hits.append(kw)
    return len(hits), hits


def judge_answer(task: dict, answer_text: str, model: str) -> dict:
    """Use Claude as a blind judge to score an answer against the rubric."""
    keywords_json = json.dumps(task["expected_answer_keywords"], indent=2)

    judge_prompt = f"""You are evaluating the quality of an answer to a code understanding question.

QUESTION:
{task['prompt']}

EXPECTED ANSWER SHOULD CONTAIN THESE CONCEPTS:
{keywords_json}

SCORING RUBRIC:
{task['rubric']}

ANSWER TO EVALUATE:
{answer_text}

Score this answer from 0 to 10 based on the rubric. Return ONLY valid JSON (no markdown fences):
{{"score": <0-10>, "reasoning": "<brief explanation of score>"}}"""

    cmd = [
        "claude",
        "-p", judge_prompt,
        "--output-format", "json",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config", str(CONFIG_DIR / "mcp-baseline.json"),
        "--model", model,
        "--max-turns", "1",
    ]

    env = os.environ.copy()
    env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"] = "1"

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(PROJECT_DIR),
            env=env,
        )
    except subprocess.TimeoutExpired:
        return {"score": -1, "reasoning": "Judge timed out"}

    if proc.returncode != 0 and not proc.stdout.strip():
        return {"score": -1, "reasoning": f"Judge error: {proc.stderr[:200]}"}

    try:
        response = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {"score": -1, "reasoning": "Judge JSON parse error"}

    result_text = response.get("result", "")
    judge_cost = response.get("total_cost_usd", 0)

    # Try to parse the score JSON from the result text
    try:
        # Strip markdown code fences if present
        clean = re.sub(r"```json\s*|\s*```", "", result_text).strip()
        score_data = json.loads(clean)
    except (json.JSONDecodeError, TypeError):
        # Try to extract score with regex
        match = re.search(r'"score"\s*:\s*(\d+)', result_text)
        if match:
            score_data = {
                "score": int(match.group(1)),
                "reasoning": result_text[:200],
            }
        else:
            score_data = {"score": -1, "reasoning": f"Failed to parse: {result_text[:200]}"}

    score_data["judge_cost_usd"] = judge_cost
    return score_data


def main():
    parser = argparse.ArgumentParser(description="LLM Judge for Benchmark Results")
    parser.add_argument("--tasks", nargs="*", help="Judge only these task IDs")
    parser.add_argument("--model", default="sonnet", help="Judge model (default: sonnet)")
    parser.add_argument("--force", action="store_true", help="Re-judge already scored results")
    parser.add_argument("--pause", type=float, default=2.0, help="Seconds between judge calls")
    parser.add_argument(
        "--keywords-only", action="store_true", help="Only compute keyword hits, skip LLM judge"
    )

    args = parser.parse_args()

    tasks = load_tasks()
    results = load_results(args.tasks)

    if not results:
        print("No results found to judge.", file=sys.stderr)
        sys.exit(1)

    print(f"Judging {len(results)} results")
    total_judge_cost = 0.0

    for i, (path, data) in enumerate(results):
        task = tasks.get(data["task_id"])
        if not task:
            print(f"  SKIP {path.name}: task {data['task_id']} not found")
            continue

        answer = data.get("result_text", "")
        if not answer:
            print(f"  SKIP {path.name}: no result text")
            continue

        # Always compute keyword hits
        hits, hit_list = count_keyword_hits(answer, task["expected_answer_keywords"])
        data["keyword_hits"] = hits
        data["keyword_total"] = len(task["expected_answer_keywords"])
        data["keyword_list"] = hit_list

        # LLM judge (unless keywords-only)
        if not args.keywords_only:
            if not args.force and "quality_score" in data and data["quality_score"] >= 0:
                print(f"  [{i+1}/{len(results)}] {path.name}: already scored ({data['quality_score']}/10), skipping")
                continue

            print(f"  [{i+1}/{len(results)}] {path.name}: judging...", end="", flush=True)

            score_data = judge_answer(task, answer, args.model)
            data["quality_score"] = score_data.get("score", -1)
            data["quality_reasoning"] = score_data.get("reasoning", "")
            data["judge_cost_usd"] = score_data.get("judge_cost_usd", 0)
            total_judge_cost += data["judge_cost_usd"]

            print(f" score={data['quality_score']}/10 keywords={hits}/{data['keyword_total']}")

            if args.pause > 0 and i < len(results) - 1:
                time.sleep(args.pause)
        else:
            print(f"  [{i+1}/{len(results)}] {path.name}: keywords={hits}/{data['keyword_total']}")

        # Write back
        with open(path, "w") as f:
            json.dump(data, f, indent=2)

    if total_judge_cost > 0:
        print(f"\nTotal judge cost: ${total_judge_cost:.4f}")


if __name__ == "__main__":
    main()
