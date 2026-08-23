# /// script
# requires-python = ">=3.10"
# ///
"""Compare two benchmark runs and print regressions and improvements.

Inputs can be a run directory, an aggregate.json file, or a raw benchmark .txt file.

Usage:
    uv run --project scripts python -m scripts.compare_benchmarks <baseline> <candidate>
    uv run --project scripts python -m scripts.compare_benchmarks <baseline> <candidate> --min-change-pct 5
    uv run --project scripts python -m scripts.compare_benchmarks <baseline> <candidate> --show-noisy --show-missing
"""

from __future__ import annotations

import argparse
import pathlib

from scripts.bench_utils import (
    METRIC_CHOICES,
    compare_result_maps,
    load_aggregate_input,
    print_change_section,
    print_path_section,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare two benchmark inputs. Accepts a run directory, "
            "an aggregate.json file, or a single raw benchmark output."
        )
    )
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("candidate", type=pathlib.Path)
    parser.add_argument(
        "--metric",
        choices=METRIC_CHOICES,
        default="median",
        help="Metric to aggregate when raw benchmark files are provided.",
    )
    parser.add_argument(
        "--min-change-pct",
        type=float,
        default=3.0,
        help="Ignore rows whose absolute percentage change is smaller than this threshold.",
    )
    parser.add_argument(
        "--noisy-threshold-pct",
        type=float,
        default=5.0,
        help="Mark rows noisy when spread exceeds this threshold. Default: 5.",
    )
    parser.add_argument(
        "--show-noisy",
        action="store_true",
        help="Include noisy rows in comparison output.",
    )
    parser.add_argument(
        "--show-missing",
        action="store_true",
        help="Also print benchmarks that exist in only one input.",
    )
    parser.add_argument(
        "--allow-empty-overlap",
        action="store_true",
        help="Return success when inputs have no comparable benchmark rows.",
    )
    return parser


def format_metadata(label: str, metadata: dict[str, str]) -> str:
    if not metadata:
        return f"{label} metadata: none"
    return (
        f"{label} metadata: "
        + ", ".join(f"{key}={value}" for key, value in sorted(metadata.items()))
    )


def main() -> int:
    args = build_parser().parse_args()

    baseline = load_aggregate_input(
        args.baseline,
        metric=args.metric,
        noisy_threshold_pct=args.noisy_threshold_pct,
    )
    candidate = load_aggregate_input(
        args.candidate,
        metric=args.metric,
        noisy_threshold_pct=args.noisy_threshold_pct,
    )

    shared_paths = sorted(set(baseline.rows) & set(candidate.rows))
    noisy_rows = {
        path
        for path in shared_paths
        if baseline.rows[path].noisy or candidate.rows[path].noisy
    }

    baseline_values = {
        path: row.value_s
        for path, row in baseline.rows.items()
        if args.show_noisy or path not in noisy_rows
    }
    candidate_values = {
        path: row.value_s
        for path, row in candidate.rows.items()
        if args.show_noisy or path not in noisy_rows
    }

    comparable_paths = sorted(set(baseline_values) & set(candidate_values))

    regressions, improvements, baseline_only, candidate_only = compare_result_maps(
        baseline_values,
        candidate_values,
        min_change_pct=args.min_change_pct,
    )

    print(
        f"Baseline: {baseline.path} | Candidate: {candidate.path} | Metric: {baseline.metric}"
    )
    print(format_metadata("Baseline", baseline.metadata))
    print(format_metadata("Candidate", candidate.metadata))
    print()

    if not shared_paths:
        print("Error: inputs have no shared benchmark rows; they are not comparable.")
        print_path_section("Only in baseline", set(baseline.rows) - set(candidate.rows))
        print_path_section("Only in candidate", set(candidate.rows) - set(baseline.rows))
        return 0 if args.allow_empty_overlap else 2

    if not comparable_paths:
        print("Error: all shared benchmark rows were suppressed as noisy.")
        print("Re-run with --show-noisy to compare them explicitly.")
        return 0 if args.allow_empty_overlap else 2

    print_change_section("Regression", regressions)
    print()
    print_change_section("Improvement", improvements)

    suppressed_noisy = sorted(noisy_rows) if not args.show_noisy else []
    if suppressed_noisy:
        print()
        print_path_section("Suppressed noisy rows", suppressed_noisy)

    if args.show_missing and (baseline_only or candidate_only):
        print()
        print_path_section("Only in baseline", baseline_only)
        print()
        print_path_section("Only in candidate", candidate_only)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
