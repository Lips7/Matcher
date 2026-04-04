#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///
"""Analyze samply profiles with source-line attribution through inlined code.

Parses a samply JSON profile, resolves leaf instruction addresses to source
lines via `atos -i` (macOS), and produces a categorized breakdown of where
time is spent — even inside heavily inlined functions.

Usage:
    # Step 1: Build the profiling binary
    cargo build --profile profiling --example profile_search -p matcher_rs

    # Step 2: Record a profile (10s, 4kHz sampling)
    uv run matcher_rs/scripts/analyze_profile.py record \\
        --mode process --shape literal --dict en --rules 10000

    # Step 3: Analyze an existing profile
    uv run matcher_rs/scripts/analyze_profile.py analyze /tmp/prof_*.json.gz

    # Or do both in one shot:
    uv run matcher_rs/scripts/analyze_profile.py record --analyze \\
        --mode process --shape literal --dict en

Requires: samply, atos (macOS built-in), cargo (for building)
"""

from __future__ import annotations

import argparse
import gzip
import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
MATCHER_RS = REPO_ROOT / "matcher_rs"
PROFILE_BINARY = "profile_search"


# ---------------------------------------------------------------------------
# Record
# ---------------------------------------------------------------------------

def find_profiling_binary() -> Path:
    """Locate the profile_search binary built with --profile profiling."""
    target = REPO_ROOT / "target" / "profiling" / "examples" / PROFILE_BINARY
    if target.exists():
        return target
    # Try to find any matching binary
    deps = REPO_ROOT / "target" / "profiling" / "deps"
    if deps.exists():
        for f in deps.iterdir():
            if f.name.startswith("profile_search") and f.is_file() and os.access(f, os.X_OK):
                return f
    return target  # will fail later with a clear message


def build_profiling_binary() -> Path:
    """Build profile_search with the profiling cargo profile."""
    print("Building profile_search with --profile profiling...")
    subprocess.run(
        ["cargo", "build", "--profile", "profiling", "--example", PROFILE_BINARY, "-p", "matcher_rs"],
        cwd=REPO_ROOT,
        check=True,
    )
    binary = find_profiling_binary()
    if not binary.exists():
        sys.exit(f"Error: binary not found at {binary} after build")
    return binary


def record_profile(
    *,
    mode: str,
    shape: str,
    dict_lang: str,
    rules: int,
    pt: str,
    seconds: int,
    rate: int,
    output: Path | None,
    build: bool,
) -> Path:
    """Run samply record on the profile_search example."""
    binary = find_profiling_binary()
    if not binary.exists() or build:
        binary = build_profiling_binary()

    if output is None:
        output = Path(f"/tmp/prof_{mode}_{shape}_{dict_lang}_{rules}.json.gz")

    env = {
        **os.environ,
        "RULES": str(rules),
        "DICT": dict_lang,
        "PT": pt,
        "MODE": mode,
        "SHAPE": shape,
        "SECONDS": str(seconds),
    }

    cmd = [
        "samply", "record",
        "-s", "-o", str(output),
        "--rate", str(rate),
        "--", str(binary),
    ]

    print(f"Recording profile: mode={mode} shape={shape} dict={dict_lang} rules={rules} pt={pt} seconds={seconds}")
    print(f"Output: {output}")
    subprocess.run(cmd, env=env, check=True)

    size = output.stat().st_size
    if size < 1024:
        sys.exit(f"Error: profile too small ({size} bytes) — samply may have failed to sample")

    print(f"Profile saved: {output} ({size / 1024:.1f} KB)")
    return output


# ---------------------------------------------------------------------------
# Analyze
# ---------------------------------------------------------------------------

def load_profile(path: Path) -> dict:
    """Load a samply JSON profile (gzipped or plain)."""
    if path.suffix == ".gz" or path.name.endswith(".json.gz"):
        with gzip.open(path, "rt") as f:
            return json.load(f)
    else:
        with open(path) as f:
            return json.load(f)


def extract_leaf_addresses(profile: dict) -> Counter:
    """Extract leaf (innermost) frame addresses with sample counts."""
    thread = profile["threads"][0]
    samples = thread["samples"]
    stack_table = thread["stackTable"]
    frame_table = thread["frameTable"]

    leaf_addrs: Counter = Counter()
    for stack_idx in samples["stack"]:
        if stack_idx is None:
            continue
        frame_idx = stack_table["frame"][stack_idx]
        addr = frame_table["address"][frame_idx]
        if addr is not None:
            leaf_addrs[addr] += 1

    return leaf_addrs


def resolve_addresses(binary: Path, addresses: list[int]) -> list[list[str]]:
    """Resolve addresses to source lines via atos with inline frame expansion.

    Returns a list of blocks, one per address. Each block is a list of
    frame strings (innermost first) from atos -i output.
    """
    addrs_hex = [f"0x{0x100000000 + a:x}" for a in addresses]

    proc = subprocess.run(
        ["atos", "-o", str(binary), "-i", "-l", "0x100000000"],
        input="\n".join(addrs_hex),
        capture_output=True,
        text=True,
    )

    blocks: list[list[str]] = []
    current: list[str] = []
    for line in proc.stdout.split("\n"):
        stripped = line.strip()
        if not stripped:
            if current:
                blocks.append(current)
                current = []
        else:
            current.append(stripped)
    if current:
        blocks.append(current)

    return blocks


def parse_source_location(frame: str) -> str | None:
    """Extract 'file.rs:123' from an atos frame string."""
    m = re.search(r"\(([^)]+\.rs:\d+)\)", frame)
    return m.group(1) if m else None


def categorize_source(src: str, via_chain: list[str]) -> str:
    """Assign a source location to a functional category."""
    via = " ".join(via_chain)

    # DFA scan
    if "dfa.rs" in src or ("cmp.rs" in src and "dfa.rs" in via):
        return "DFA scan"
    if "alphabet.rs" in src and "dfa.rs" in via:
        return "DFA scan"
    if "automaton.rs" in src:
        return "AC normalize scan"

    # Daachorse (bytewise/charwise)
    if "bytewise.rs" in src or "charwise.rs" in src:
        return "Daachorse scan"

    # Iterator / byte loop
    if ("macros.rs" in src or "iter.rs" in src) and "engine.rs" in via:
        return "Scan iterator"

    # Engine dispatch
    if "engine.rs" in src:
        return "Engine dispatch"

    # Search callback / closure
    if "search.rs" in src:
        return "Search callback"

    # State machine
    if "state.rs" in src:
        return "State machine"
    if "rule.rs" in src:
        return "Rule evaluation"

    # Vec / allocation
    if "mod.rs" in src and ("3006" in src or "810" in src or "619" in src or "1004" in src or "1040" in src):
        return "Vec operations"

    # ASCII check
    if "ascii.rs" in src:
        return "ASCII check"

    # Profile search main
    if "profile_search.rs" in src:
        return "Main loop"

    # Allocator
    if "alloc" in src.lower() or "mi_" in src:
        return "Allocator"

    # Misc std
    if "option.rs" in src or "hint.rs" in src or "nonzero.rs" in src or "num.rs" in src:
        return "Std overhead"

    # Index operations
    if "index.rs" in src:
        if "daachorse" in via or "bytewise" in via or "charwise" in via:
            return "Daachorse scan"
        return "Index ops"

    # Vec internals (remaining mod.rs)
    if "mod.rs" in src:
        if "dfa.rs" in via or "engine.rs" in via:
            return "DFA scan"
        if "rule.rs" in via or "state.rs" in via:
            return "Rule evaluation"
        return "Std overhead"

    return "Other"


def analyze_profile(path: Path, binary: Path, top_n: int = 30) -> dict:
    """Full analysis: load profile, resolve addresses, categorize."""
    profile = load_profile(path)
    leaf_addrs = extract_leaf_addresses(profile)
    total_samples = sum(leaf_addrs.values())

    # Take top addresses (covering ~95%+ of samples)
    top_addrs = sorted(leaf_addrs.items(), key=lambda x: -x[1])[:top_n]
    addresses = [a for a, _ in top_addrs]
    counts = [c for _, c in top_addrs]

    blocks = resolve_addresses(binary, addresses)

    # Build per-address detail and category aggregation
    details: list[dict] = []
    categories: Counter = Counter()

    for i, ((_addr, count), block) in enumerate(zip(top_addrs, blocks)):
        chain = []
        for frame in block:
            loc = parse_source_location(frame)
            if loc:
                chain.append(loc)

        leaf = chain[0] if chain else "(unknown)"
        via = list(dict.fromkeys(chain[1:])) if len(chain) > 1 else []
        category = categorize_source(leaf, via)

        categories[category] += count
        details.append({
            "rank": i + 1,
            "pct": count / total_samples * 100,
            "count": count,
            "leaf": leaf,
            "via": via[:4],
            "category": category,
        })

    accounted = sum(counts)
    return {
        "total_samples": total_samples,
        "accounted_samples": accounted,
        "accounted_pct": accounted / total_samples * 100,
        "categories": dict(sorted(categories.items(), key=lambda x: -x[1])),
        "details": details,
    }


def print_report(result: dict, path: Path) -> None:
    """Print a formatted report."""
    total = result["total_samples"]

    print(f"\n{'=' * 70}")
    print(f"  Profile: {path.name}")
    print(f"  Samples: {total:,}  (accounted: {result['accounted_pct']:.1f}%)")
    print(f"{'=' * 70}")

    # Category summary
    print("\n  Category Breakdown:")
    print(f"  {'-' * 50}")
    for cat, count in result["categories"].items():
        pct = count / total * 100
        bar = "█" * int(pct / 2)
        print(f"  {pct:5.1f}%  {cat:<25s} {bar}")

    # Detailed top addresses
    print("\n  Top Hot Addresses:")
    print(f"  {'-' * 66}")
    print(f"  {'%':>6s}  {'Leaf Source':<30s}  {'Category':<20s}  Via")
    print(f"  {'-' * 66}")
    for d in result["details"][:20]:
        via_str = " <- ".join(d["via"][:3]) if d["via"] else ""
        print(f"  {d['pct']:5.1f}%  {d['leaf']:<30s}  {d['category']:<20s}  {via_str}")

    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Analyze samply profiles with source-line attribution.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # --- record ---
    rec = sub.add_parser("record", help="Record a profile with samply")
    rec.add_argument("--mode", default="process", choices=["is_match", "process"])
    rec.add_argument("--shape", default="literal", choices=["literal", "and", "not"])
    rec.add_argument("--dict", default="en", choices=["en", "cn"], dest="dict_lang")
    rec.add_argument("--rules", type=int, default=10_000)
    rec.add_argument("--pt", default="none",
                     choices=["none", "fanjian", "delete", "norm", "dn", "fdn", "pinyin", "pychar"])
    rec.add_argument("--seconds", type=int, default=10)
    rec.add_argument("--rate", type=int, default=4000)
    rec.add_argument("--output", "-o", type=Path, default=None)
    rec.add_argument("--build", action="store_true", help="Force rebuild before recording")
    rec.add_argument("--analyze", action="store_true", help="Analyze immediately after recording")
    rec.add_argument("--top", type=int, default=30, help="Number of top addresses to resolve")

    # --- analyze ---
    ana = sub.add_parser("analyze", help="Analyze an existing samply profile")
    ana.add_argument("profiles", nargs="+", type=Path, help="One or more .json.gz profile files")
    ana.add_argument("--top", type=int, default=30, help="Number of top addresses to resolve")
    ana.add_argument("--binary", type=Path, default=None,
                     help="Path to the profiled binary (auto-detected if omitted)")

    args = parser.parse_args()

    if args.command == "record":
        output = record_profile(
            mode=args.mode,
            shape=args.shape,
            dict_lang=args.dict_lang,
            rules=args.rules,
            pt=args.pt,
            seconds=args.seconds,
            rate=args.rate,
            output=args.output,
            build=args.build,
        )
        if args.analyze:
            binary = find_profiling_binary()
            result = analyze_profile(output, binary, top_n=args.top)
            print_report(result, output)

    elif args.command == "analyze":
        binary = args.binary or find_profiling_binary()
        if not binary.exists():
            sys.exit(
                f"Error: binary not found at {binary}\n"
                f"Run: cargo build --profile profiling --example profile_search -p matcher_rs"
            )
        for path in args.profiles:
            if not path.exists():
                print(f"Warning: {path} not found, skipping", file=sys.stderr)
                continue
            result = analyze_profile(path, binary, top_n=args.top)
            print_report(result, path)


if __name__ == "__main__":
    main()
