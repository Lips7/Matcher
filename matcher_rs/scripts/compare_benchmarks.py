#!/usr/bin/env python3

from __future__ import annotations

import argparse
import dataclasses
import pathlib
import re
import sys
from typing import Iterable


TREE_LINE_RE = re.compile(r"^(?P<prefix>(?:│  |   )*)(?P<branch>[├╰])─ (?P<rest>.*)$")
BENCH_ROW_RE = re.compile(
    r"""
    ^(?P<name>.+?)\s{2,}
    (?P<fastest>[0-9.]+\s+(?:ns|µs|us|ms|s))\s+│\s+
    (?P<slowest>[0-9.]+\s+(?:ns|µs|us|ms|s))\s+│\s+
    (?P<median>[0-9.]+\s+(?:ns|µs|us|ms|s))\s+│\s+
    (?P<mean>[0-9.]+\s+(?:ns|µs|us|ms|s))\s+│\s+
    (?P<samples>\d+)\s+│\s+(?P<iters>\d+)
    $
    """,
    re.VERBOSE,
)
META_RE = re.compile(r"^(Hardware|Feature|Date):\s*(.+)$")

UNIT_TO_SECONDS = {
    "ns": 1e-9,
    "us": 1e-6,
    "µs": 1e-6,
    "ms": 1e-3,
    "s": 1.0,
}


@dataclasses.dataclass(frozen=True)
class BenchResult:
    path: str
    fastest_s: float
    slowest_s: float
    median_s: float
    mean_s: float
    samples: int
    iters: int

    def metric(self, name: str) -> float:
        return {
            "fastest": self.fastest_s,
            "slowest": self.slowest_s,
            "median": self.median_s,
            "mean": self.mean_s,
        }[name]


@dataclasses.dataclass(frozen=True)
class BenchFile:
    path: pathlib.Path
    metadata: dict[str, str]
    results: dict[str, BenchResult]


def parse_duration(value: str) -> float:
    amount_str, unit = value.split()
    return float(amount_str) * UNIT_TO_SECONDS[unit]


def format_duration(seconds: float) -> str:
    if seconds < 1e-6:
        return f"{seconds * 1e9:.3f} ns"
    if seconds < 1e-3:
        return f"{seconds * 1e6:.3f} µs"
    if seconds < 1:
        return f"{seconds * 1e3:.3f} ms"
    return f"{seconds:.3f} s"


def parse_bench_file(path: pathlib.Path) -> BenchFile:
    metadata: dict[str, str] = {}
    results: dict[str, BenchResult] = {}
    stack: list[str] = []

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        meta_match = META_RE.match(raw_line)
        if meta_match:
            metadata[meta_match.group(1)] = meta_match.group(2)
            continue

        tree_match = TREE_LINE_RE.match(raw_line)
        if not tree_match:
            continue

        depth = len(tree_match.group("prefix")) // 3
        rest = tree_match.group("rest").rstrip()
        row_match = BENCH_ROW_RE.match(rest)

        if row_match:
            name = row_match.group("name").strip().strip('"')
            stack = stack[:depth]
            full_path = " / ".join(stack + [name])
            results[full_path] = BenchResult(
                path=full_path,
                fastest_s=parse_duration(row_match.group("fastest")),
                slowest_s=parse_duration(row_match.group("slowest")),
                median_s=parse_duration(row_match.group("median")),
                mean_s=parse_duration(row_match.group("mean")),
                samples=int(row_match.group("samples")),
                iters=int(row_match.group("iters")),
            )
            continue

        name = rest.split("│", 1)[0].rstrip().strip('"')
        stack = stack[:depth]
        if len(stack) == depth:
            stack.append(name)
        else:
            stack[depth] = name

    return BenchFile(path=path, metadata=metadata, results=results)


def compare_results(
    baseline: BenchFile,
    candidate: BenchFile,
    metric: str,
    min_change_pct: float,
) -> tuple[list[dict[str, object]], list[dict[str, object]], set[str], set[str]]:
    regressions: list[dict[str, object]] = []
    improvements: list[dict[str, object]] = []

    shared_paths = sorted(set(baseline.results) & set(candidate.results))
    baseline_only = set(baseline.results) - set(candidate.results)
    candidate_only = set(candidate.results) - set(baseline.results)

    for path in shared_paths:
        base_value = baseline.results[path].metric(metric)
        cand_value = candidate.results[path].metric(metric)
        if base_value == 0:
            continue
        delta_pct = ((cand_value - base_value) / base_value) * 100.0
        if abs(delta_pct) < min_change_pct:
            continue

        row = {
            "path": path,
            "baseline": base_value,
            "candidate": cand_value,
            "delta_pct": delta_pct,
        }
        if delta_pct > 0:
            regressions.append(row)
        else:
            improvements.append(row)

    regressions.sort(key=lambda item: item["delta_pct"], reverse=True)
    improvements.sort(key=lambda item: item["delta_pct"])
    return regressions, improvements, baseline_only, candidate_only


def print_section(title: str, rows: Iterable[dict[str, object]]) -> None:
    rows = list(rows)
    print(title)
    if not rows:
        print("  none")
        return

    for row in rows:
        delta_pct = float(row["delta_pct"])
        print(
            "  - {path}: {baseline} -> {candidate} ({delta:+.2f}%)".format(
                path=row["path"],
                baseline=format_duration(float(row["baseline"])),
                candidate=format_duration(float(row["candidate"])),
                delta=delta_pct,
            )
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
    regressions, improvements, baseline_only, candidate_only = compare_results(
        baseline,
        candidate,
        metric=args.metric,
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

    print_section("Regression", regressions)
    print()
    print_section("Improvement", improvements)

    if args.show_missing and (baseline_only or candidate_only):
        print()
        if baseline_only:
            print("Missing in candidate")
            for path in sorted(baseline_only):
                print(f"  - {path}")
        if candidate_only:
            print("Missing in baseline")
            for path in sorted(candidate_only):
                print(f"  - {path}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
