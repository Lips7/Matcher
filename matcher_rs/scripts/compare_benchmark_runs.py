#!/usr/bin/env python3

from __future__ import annotations

import argparse
import pathlib

from bench_utils import (
    compare_result_maps,
    load_aggregate_input,
    print_change_section,
    print_path_section,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare two aggregated benchmark run sets. Inputs may be a run directory, "
            "an aggregate.json file, or a single raw benchmark output."
        )
    )
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("candidate", type=pathlib.Path)
    parser.add_argument(
        "--metric",
        choices=["median", "mean", "fastest", "slowest"],
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
