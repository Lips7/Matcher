#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///

from __future__ import annotations

import argparse
import pathlib

from bench_utils import (
    compare_result_maps,
    parse_bench_file,
    print_change_section,
    print_path_section,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare two matcher_rs benchmark record files. "
            "The first file is treated as the baseline and the second as the candidate."
        )
    )
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("candidate", type=pathlib.Path)
    parser.add_argument(
        "--metric",
        choices=["median", "mean", "fastest", "slowest"],
        default="median",
        help="Latency metric to compare. Default: median.",
    )
    parser.add_argument(
        "--min-change-pct",
        type=float,
        default=5.0,
        help="Ignore rows whose absolute percentage change is smaller than this threshold.",
    )
    parser.add_argument(
        "--show-missing",
        action="store_true",
        help="Also print benchmarks that exist in only one file.",
    )
    return parser


def main() -> int:
    args = build_parser().parse_args()

    baseline = parse_bench_file(args.baseline)
    candidate = parse_bench_file(args.candidate)
    regressions, improvements, baseline_only, candidate_only = compare_result_maps(
        {path: result.metric(args.metric) for path, result in baseline.results.items()},
        {path: result.metric(args.metric) for path, result in candidate.results.items()},
        min_change_pct=args.min_change_pct,
    )

    print(
        f"Baseline: {baseline.path.name} | Candidate: {candidate.path.name} | Metric: {args.metric}"
    )
    if baseline.metadata:
        print(
            "Baseline metadata: "
            + ", ".join(f"{key}={value}" for key, value in baseline.metadata.items())
        )
    if candidate.metadata:
        print(
            "Candidate metadata: "
            + ", ".join(f"{key}={value}" for key, value in candidate.metadata.items())
        )
    print()

    print_change_section("Regression", regressions)
    print()
    print_change_section("Improvement", improvements)

    if args.show_missing and (baseline_only or candidate_only):
        print()
        print_path_section("Only in baseline", baseline_only)
        print()
        print_path_section("Only in candidate", candidate_only)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
