#!/usr/bin/env python3

from __future__ import annotations

import dataclasses
import json
import pathlib
import re
from statistics import median
from typing import Iterable

METRIC_CHOICES = ["median", "mean", "fastest", "slowest"]

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
META_RE = re.compile(r"^(?P<key>[A-Za-z][A-Za-z ]+):\s*(?P<value>.+)$")

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


@dataclasses.dataclass(frozen=True)
class AggregateRow:
    path: str
    value_s: float
    runs: int
    q1_s: float
    q3_s: float
    spread_pct: float
    noisy: bool


@dataclasses.dataclass(frozen=True)
class AggregateFile:
    path: pathlib.Path
    metadata: dict[str, str]
    metric: str
    rows: dict[str, AggregateRow]


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
            metadata[meta_match.group("key")] = meta_match.group("value")
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


def compare_result_maps(
    baseline: dict[str, float],
    candidate: dict[str, float],
    min_change_pct: float,
) -> tuple[list[dict[str, str | int | float]], list[dict[str, str | int | float]], set[str], set[str]]:
    regressions: list[dict[str, str | int | float]] = []
    improvements: list[dict[str, str | int | float]] = []

    shared_paths = sorted(set(baseline) & set(candidate))
    baseline_only = set(baseline) - set(candidate)
    candidate_only = set(candidate) - set(baseline)

    for path in shared_paths:
        base_value = baseline[path]
        cand_value = candidate[path]
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


def print_change_section(title: str, rows: Iterable[dict[str, str | int | float]]) -> None:
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


def print_path_section(title: str, paths: Iterable[str]) -> None:
    paths = sorted(paths)
    print(title)
    if not paths:
        print("  none")
        return

    for path in paths:
        print(f"  - {path}")


def median_of_metric(files: Iterable[BenchFile], metric: str) -> dict[str, list[float]]:
    metric_by_path: dict[str, list[float]] = {}
    for bench_file in files:
        for path, result in bench_file.results.items():
            metric_by_path.setdefault(path, []).append(result.metric(metric))
    return metric_by_path


def quartiles(values: list[float]) -> tuple[float, float]:
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0], ordered[0]

    mid = len(ordered) // 2
    if len(ordered) % 2 == 0:
        lower = ordered[:mid]
        upper = ordered[mid:]
    else:
        lower = ordered[:mid]
        upper = ordered[mid + 1 :]
        if not lower:
            lower = ordered
        if not upper:
            upper = ordered

    return median(lower), median(upper)


def aggregate_bench_files(
    files: Iterable[BenchFile],
    metric: str = "median",
    noisy_threshold_pct: float = 5.0,
) -> dict[str, AggregateRow]:
    rows: dict[str, AggregateRow] = {}
    for path, values in median_of_metric(files, metric).items():
        value_s = median(values)
        q1_s, q3_s = quartiles(values)
        spread_pct = 0.0 if value_s == 0 else ((q3_s - q1_s) / value_s) * 100.0
        rows[path] = AggregateRow(
            path=path,
            value_s=value_s,
            runs=len(values),
            q1_s=q1_s,
            q3_s=q3_s,
            spread_pct=spread_pct,
            noisy=spread_pct > noisy_threshold_pct,
        )
    return rows


def write_aggregate_json(path: pathlib.Path, aggregate: AggregateFile) -> None:
    payload = {
        "metadata": aggregate.metadata,
        "metric": aggregate.metric,
        "rows": {
            row_path: {
                "value_s": row.value_s,
                "runs": row.runs,
                "q1_s": row.q1_s,
                "q3_s": row.q3_s,
                "spread_pct": row.spread_pct,
                "noisy": row.noisy,
            }
            for row_path, row in sorted(aggregate.rows.items())
        },
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")


def read_aggregate_json(path: pathlib.Path) -> AggregateFile:
    payload = json.loads(path.read_text(encoding="utf-8"))
    rows = {
        row_path: AggregateRow(
            path=row_path,
            value_s=row_payload["value_s"],
            runs=row_payload["runs"],
            q1_s=row_payload["q1_s"],
            q3_s=row_payload["q3_s"],
            spread_pct=row_payload["spread_pct"],
            noisy=row_payload["noisy"],
        )
        for row_path, row_payload in payload["rows"].items()
    }
    return AggregateFile(
        path=path,
        metadata=payload.get("metadata", {}),
        metric=payload["metric"],
        rows=rows,
    )


def render_aggregate_summary(aggregate: AggregateFile) -> str:
    stable = sum(not row.noisy for row in aggregate.rows.values())
    noisy = sum(row.noisy for row in aggregate.rows.values())

    lines = [
        f"Metric: {aggregate.metric}",
        f"Benchmarks: {len(aggregate.rows)}",
        f"Stable rows: {stable}",
        f"Noisy rows (>5% spread): {noisy}",
    ]
    for key, value in aggregate.metadata.items():
        if key == "Metric":
            continue
        lines.append(f"{key}: {value}")

    lines.append("")
    lines.append("Rows")
    for row in sorted(aggregate.rows.values(), key=lambda item: item.path):
        flag = " noisy" if row.noisy else ""
        lines.append(
            f"  - {row.path}: {format_duration(row.value_s)} | spread={row.spread_pct:.2f}% | runs={row.runs}{flag}"
        )
    return "\n".join(lines) + "\n"


def load_aggregate_input(
    path: pathlib.Path,
    metric: str = "median",
    noisy_threshold_pct: float = 5.0,
) -> AggregateFile:
    if path.is_file():
        if path.suffix == ".json":
            return read_aggregate_json(path)
        bench_file = parse_bench_file(path)
        rows = aggregate_bench_files([bench_file], metric=metric, noisy_threshold_pct=noisy_threshold_pct)
        return AggregateFile(path=path, metadata=bench_file.metadata, metric=metric, rows=rows)

    aggregate_json = path / "aggregate.json"
    if aggregate_json.exists():
        return read_aggregate_json(aggregate_json)

    raw_dir = path / "raw"
    candidates = sorted(raw_dir.glob("*.txt")) if raw_dir.exists() else sorted(path.glob("*.txt"))
    files = [
        parse_bench_file(candidate)
        for candidate in candidates
        if candidate.name not in {"summary.txt"}
    ]
    metadata = {"Source": str(path)}
    manifest = path / "manifest.json"
    if manifest.exists():
        manifest_payload = json.loads(manifest.read_text(encoding="utf-8"))
        metadata.update({key.replace("_", " ").title(): str(value) for key, value in manifest_payload.items() if not isinstance(value, list)})
    rows = aggregate_bench_files(files, metric=metric, noisy_threshold_pct=noisy_threshold_pct)
    return AggregateFile(path=path, metadata=metadata, metric=metric, rows=rows)
