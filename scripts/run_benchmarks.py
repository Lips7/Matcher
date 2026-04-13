#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///
"""Run matcher_rs benchmarks and aggregate results into timestamped run sets.

Usage:
    uv run scripts/run_benchmarks.py                          # search preset (default)
    uv run scripts/run_benchmarks.py --preset build           # construction benchmarks
    uv run scripts/run_benchmarks.py --preset all             # all presets
    uv run scripts/run_benchmarks.py --quick                  # fast iteration (5 samples, 1 repeat)
    uv run scripts/run_benchmarks.py --filter scaling         # narrow to a divan module
    uv run scripts/run_benchmarks.py --filter "scaling::process_cn"
    uv run scripts/run_benchmarks.py --repeats 5 --profile bench-dev
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import platform
import subprocess
import sys
from collections import OrderedDict

from bench_utils import (
    METRIC_CHOICES,
    AggregateFile,
    aggregate_bench_files,
    parse_bench_file,
    render_aggregate_summary,
    write_aggregate_json,
)

REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
BENCH_RECORDS_DIR = REPO_ROOT / "scripts" / "bench_records"


def run_command(command: list[str], capture_output: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=capture_output,
    )


def detect_hardware() -> str:
    if sys.platform == "darwin":
        try:
            result = run_command(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True)
            return result.stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            pass

    machine = platform.machine()
    processor = platform.processor()
    if processor and processor != machine:
        return f"{processor} ({machine})"
    return machine or platform.platform()


def detect_branch() -> str:
    try:
        result = run_command(["git", "branch", "--show-current"], capture_output=True)
        return result.stdout.strip() or "detached"
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run matcher_rs benchmarks serially and aggregate repeated runs."
    )
    parser.add_argument(
        "--preset",
        choices=["search", "build", "all"],
        default="search",
        help="Benchmark preset to run. Default: search.",
    )
    parser.add_argument(
        "--repeats",
        type=int,
        default=3,
        help="Number of recorded repeats. Default: 3.",
    )
    parser.add_argument(
        "--metric",
        choices=METRIC_CHOICES,
        default="median",
        help="Metric used in the aggregate summary. Default: median.",
    )
    parser.add_argument(
        "--sample-count",
        type=int,
        default=None,
        help="Override divan sample count for all benchmark commands.",
    )
    parser.add_argument(
        "--min-time",
        type=float,
        default=None,
        help="Override divan minimum time in seconds for all benchmark commands.",
    )
    parser.add_argument(
        "--no-warmup",
        action="store_true",
        help="Skip the unrecorded warm-up pass.",
    )
    parser.add_argument(
        "--profile",
        default="bench",
        help="Cargo profile to build/run benchmarks with. Default: bench.",
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Quick iteration mode: sample-count=5, min-time=0.5, repeats=1, no warmup.",
    )
    parser.add_argument(
        "--output-dir",
        type=pathlib.Path,
        default=BENCH_RECORDS_DIR,
        help="Directory that will receive the benchmark run set. Default: scripts/bench_records.",
    )
    parser.add_argument(
        "--filter",
        default=None,
        help=(
            "Divan filter pattern to narrow which benchmarks run. "
            "Replaces the preset's default module filters. "
            "Examples: 'text_transform', 'text_transform::cn', 'scaling::process_cn'."
        ),
    )
    return parser


def preset_commands(
    sample_count_override: int | None,
    min_time_override: float | None,
    profile: str = "bench",
    filter_pattern: str | None = None,
) -> OrderedDict[str, list[str]]:
    def divan_args(kind: str) -> list[str]:
        defaults = {
            "search": {"sample_count": 40, "min_time": 2.0},
            "build": {"sample_count": 15, "min_time": 0.5},
        }[kind]
        sample_count = sample_count_override or defaults["sample_count"]
        min_time = min_time_override or defaults["min_time"]
        return [
            "--timer",
            "os",
            "--color",
            "never",
            "--sample-count",
            str(sample_count),
            "--min-time",
            str(min_time),
            "--skip-ext-time",
            "true",
        ]

    def cargo_bench(bench_name: str) -> list[str]:
        cmd = ["cargo", "bench", "-p", "matcher_rs"]
        if profile != "bench":
            cmd += ["--profile", profile]
        cmd += ["--bench", bench_name, "--"]
        return cmd

    # Each preset maps to a list of (bench_target, divan_kind) pairs.
    # When --filter is provided, it's applied across all targets in the preset.
    presets: dict[str, list[tuple[str, str]]] = {
        "search": [
            ("bench_search", "search"),
            ("bench_transform", "search"),
        ],
        "build": [("bench_build", "build")],
    }

    result = OrderedDict()
    for preset_name, targets in presets.items():
        for bench_target, divan_kind in targets:
            filters = [filter_pattern] if filter_pattern else []
            result[f"{preset_name}:{bench_target}"] = [
                *cargo_bench(bench_target),
                *filters,
                *divan_args(divan_kind),
            ]
    return result


def command_sets_for_preset(
    preset: str,
    sample_count_override: int | None,
    min_time_override: float | None,
    profile: str = "bench",
    filter_pattern: str | None = None,
) -> OrderedDict[str, list[str]]:
    commands = preset_commands(
        sample_count_override, min_time_override, profile=profile, filter_pattern=filter_pattern,
    )
    if preset == "all":
        return commands
    return OrderedDict(
        (key, cmd) for key, cmd in commands.items() if key.startswith(f"{preset}:")
    )


def prebuild(command_sets: OrderedDict[str, list[str]], profile: str = "bench") -> None:
    benches = {
        command[command.index("--bench") + 1]
        for command in command_sets.values()
        if "--bench" in command
    }
    for bench_name in sorted(benches):
        cmd = ["cargo", "bench", "-p", "matcher_rs"]
        if profile != "bench":
            cmd += ["--profile", profile]
        cmd += ["--bench", bench_name, "--no-run"]
        run_command(cmd)


def timestamp_slug() -> str:
    return dt.datetime.now().strftime("%Y-%m-%d_%H-%M-%S")


def bench_header(metadata: dict[str, str]) -> str:
    lines = [f"{key}: {value}" for key, value in metadata.items()]
    return "\n".join(lines) + "\n\n"


def aggregate_run_set(run_dir: pathlib.Path, metric: str, metadata: dict[str, str]) -> AggregateFile:
    raw_dir = run_dir / "raw"
    files = [parse_bench_file(path) for path in sorted(raw_dir.glob("*.txt"))]
    rows = aggregate_bench_files(files, metric=metric)
    return AggregateFile(path=run_dir, metadata=metadata, metric=metric, rows=rows)


def main() -> int:
    args = build_parser().parse_args()

    if args.quick:
        args.sample_count = args.sample_count or 5
        args.min_time = args.min_time or 0.5
        args.repeats = 1
        args.no_warmup = True

    command_sets = command_sets_for_preset(
        args.preset, args.sample_count, args.min_time,
        profile=args.profile, filter_pattern=args.filter,
    )

    if args.repeats <= 0:
        raise SystemExit("--repeats must be greater than zero")

    filter_suffix = f"_{args.filter}" if args.filter else ""
    run_dir = args.output_dir / f"{timestamp_slug()}_{args.preset}{filter_suffix}"
    raw_dir = run_dir / "raw"
    raw_dir.mkdir(parents=True, exist_ok=False)

    metadata = OrderedDict(
        [
            ("Date", dt.datetime.now().isoformat(timespec="seconds")),
            ("Preset", args.preset),
            ("Repeat Count", str(args.repeats)),
            ("Metric", args.metric),
            ("Profile", args.profile),
            ("Branch", detect_branch()),
            ("Hardware", detect_hardware()),
            ("Platform", platform.platform()),
            ("Python", platform.python_version()),
        ]
    )

    manifest = {
        "date": metadata["Date"],
        "preset": args.preset,
        "repeat_count": args.repeats,
        "metric": args.metric,
        "profile": args.profile,
        "branch": metadata["Branch"],
        "hardware": metadata["Hardware"],
        "platform": metadata["Platform"],
        "python": metadata["Python"],
        "commands": [
            {"label": label, "argv": command}
            for label, command in command_sets.items()
        ],
    }
    (run_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True),
        encoding="utf-8",
    )

    prebuild(command_sets, profile=args.profile)

    if not args.no_warmup:
        for label, command in command_sets.items():
            print(f"[warmup] {label}", flush=True)
            run_command(command)

    for repeat_index in range(1, args.repeats + 1):
        for label, command in command_sets.items():
            print(f"[run {repeat_index}/{args.repeats}] {label}", flush=True)
            result = run_command(command, capture_output=True)
            output_path = raw_dir / f"{label}-run{repeat_index:02d}.txt"
            output_path.write_text(
                bench_header(
                    OrderedDict(
                        [
                            *metadata.items(),
                            ("Command", " ".join(command)),
                            ("Command Label", label),
                            ("Repeat", str(repeat_index)),
                        ]
                    )
                )
                + result.stdout,
                encoding="utf-8",
            )

    aggregate = aggregate_run_set(run_dir, args.metric, dict(metadata))
    write_aggregate_json(run_dir / "aggregate.json", aggregate)
    (run_dir / "summary.txt").write_text(
        render_aggregate_summary(aggregate),
        encoding="utf-8",
    )

    print(f"Run set: {run_dir}")
    print(f"Summary: {run_dir / 'summary.txt'}")
    print(f"Aggregate: {run_dir / 'aggregate.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
