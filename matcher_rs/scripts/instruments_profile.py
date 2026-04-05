#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///
"""Profile matcher_rs using Xcode Instruments (Time Profiler).

Records via `xctrace`, exports the call tree to XML, demangles Rust symbols,
and produces a categorized breakdown — including closures and inlined frames
that samply misses.

Usage:
    # Record + analyze in one shot (default 10s):
    uv run matcher_rs/scripts/instruments_profile.py record --analyze \
        --mode process --shape literal --dict en --rules 10000

    # Just record (opens in Instruments.app afterwards):
    uv run matcher_rs/scripts/instruments_profile.py record \
        --mode is_match --dict cn --rules 500 --open

    # Analyze an existing .trace bundle:
    uv run matcher_rs/scripts/instruments_profile.py analyze /tmp/prof_*.trace

Requires: Xcode (xctrace), cargo, rustfilt (`cargo install rustfilt`)
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path
from xml.etree import ElementTree as ET

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
PROFILE_BINARY = "profile_search"


# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

def find_profiling_binary() -> Path:
    target = REPO_ROOT / "target" / "profiling" / "examples" / PROFILE_BINARY
    if target.exists():
        return target
    deps = REPO_ROOT / "target" / "profiling" / "deps"
    if deps.exists():
        for f in deps.iterdir():
            if f.name.startswith("profile_search") and f.is_file() and os.access(f, os.X_OK):
                return f
    return target


def build_profiling_binary() -> Path:
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


# ---------------------------------------------------------------------------
# Record
# ---------------------------------------------------------------------------

def record_profile(
    *,
    mode: str,
    shape: str,
    dict_lang: str,
    rules: int,
    pt: str,
    seconds: int,
    output: Path | None,
    build: bool,
) -> Path:
    binary = find_profiling_binary()
    if not binary.exists() or build:
        binary = build_profiling_binary()

    if output is None:
        output = Path(f"/tmp/prof_{mode}_{shape}_{dict_lang}_{rules}.trace")

    if output.exists():
        subprocess.run(["rm", "-rf", str(output)], check=True)

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
        "xctrace", "record",
        "--template", "Time Profiler",
        "--time-limit", f"{seconds + 5}s",
        "--output", str(output),
        "--launch", "--", str(binary),
    ]

    print(f"Recording: mode={mode} shape={shape} dict={dict_lang} rules={rules} pt={pt} seconds={seconds}s")
    print(f"Output: {output}")

    result = subprocess.run(cmd, env=env)
    if result.returncode != 0:
        sys.exit(f"xctrace record failed (exit {result.returncode})")

    if not output.exists():
        sys.exit(f"Error: trace not created at {output}")

    print(f"Trace saved: {output}")
    return output


# ---------------------------------------------------------------------------
# Demangle
# ---------------------------------------------------------------------------

def batch_demangle(mangled: set[str]) -> dict[str, str]:
    """Demangle a set of Rust symbols via rustfilt."""
    if not mangled:
        return {}

    names = sorted(mangled)
    try:
        proc = subprocess.run(
            ["rustfilt"],
            input="\n".join(names),
            capture_output=True,
            text=True,
            timeout=10,
        )
        demangled = proc.stdout.strip().split("\n")
        if len(demangled) == len(names):
            return dict(zip(names, demangled))
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    # Fallback: strip hash suffix only
    result = {}
    for n in names:
        clean = re.sub(r"::h[0-9a-f]{16}$", "", n)
        result[n] = clean
    return result


def shorten_symbol(demangled: str) -> str:
    """Clean up a demangled Rust symbol for display (no abbreviation)."""
    s = demangled
    # Remove leading < and trailing > for impl blocks
    if s.startswith("<") and ">" in s:
        s = re.sub(r"^<(.+?)>::", r"\1::", s)
    return s


# ---------------------------------------------------------------------------
# Parse XML
# ---------------------------------------------------------------------------

def parse_time_profile(xml_text: str) -> list[dict]:
    """Parse xctrace time-profile XML into structured samples.

    Each sample has:
      - weight_ns: sample weight in nanoseconds
      - frames: list of {name, source_file, source_line} dicts (leaf first)
      - thread: thread description string
    """
    root = ET.fromstring(xml_text)

    # Build id→element lookup for ref resolution
    id_map: dict[str, ET.Element] = {}
    for elem in root.iter():
        eid = elem.get("id")
        if eid:
            id_map[eid] = elem

    samples: list[dict] = []

    for row in root.iter("row"):
        weight_ns = 1_000_000  # default 1ms

        # Find weight element (may be inline or ref)
        weight_el = row.find("weight")
        if weight_el is not None:
            ref = weight_el.get("ref")
            if ref and ref in id_map:
                weight_el = id_map[ref]
            try:
                weight_ns = int(weight_el.text or "1000000")
            except ValueError:
                pass

        # Find thread
        thread_el = row.find("thread")
        thread_name = ""
        if thread_el is not None:
            ref = thread_el.get("ref")
            if ref and ref in id_map:
                thread_el = id_map[ref]
            thread_name = thread_el.get("fmt", "")

        # Find backtrace (may be ref)
        bt_el = row.find("backtrace")
        if bt_el is None:
            continue
        ref = bt_el.get("ref")
        if ref and ref in id_map:
            bt_el = id_map[ref]

        frames = []
        for frame_el in bt_el.iter("frame"):
            # Resolve ref
            fref = frame_el.get("ref")
            if fref and fref in id_map:
                frame_el = id_map[fref]

            name = frame_el.get("name", "")
            source_file = None
            source_line = None

            src_el = frame_el.find("source")
            if src_el is not None:
                source_line = src_el.get("line")
                path_el = src_el.find("path")
                if path_el is not None:
                    pref = path_el.get("ref")
                    if pref and pref in id_map:
                        path_el = id_map[pref]
                    source_file = (path_el.text or "").strip()

            frames.append({
                "name": name,
                "source_file": source_file,
                "source_line": source_line,
            })

        if frames:
            samples.append({
                "weight_ns": weight_ns,
                "frames": frames,
                "thread": thread_name,
            })

    return samples


# ---------------------------------------------------------------------------
# Categorization
# ---------------------------------------------------------------------------

CATEGORY_RULES: list[tuple[str, list[str]]] = [
    ("Harry scan",        ["harry", "HarryMatcher"]),
    ("DFA scan",          ["dfa::", "DFA::", "aho_corasick::dfa"]),
    ("Daachorse scan",    ["bytewise", "charwise", "daachorse"]),
    ("AC normalize scan", ["automaton::", "NormalizeMatcher"]),
    ("Engine dispatch",   ["engine::", "ScanPlan", "BytewiseMatcher", "CharwiseMatcher"]),
    ("Search hot path",   ["search::", "walk_and_scan", "scan_variant", "process_match",
                           "is_match_simple", "process_simple"]),
    ("State machine",     ["state::", "WordState", "SimpleMatchState", "ScanContext",
                           "generation"]),
    ("Rule evaluation",   ["rule::", "process_entry", "RuleSet", "RuleHot"]),
    ("Text transform",    ["process::", "transform::", "DeleteMatcher", "FanjianMatcher",
                           "PinyinMatcher", "string_pool", "ProcessType"]),
    ("ASCII check",       ["is_ascii", "ascii::"]),
    ("Allocator",         ["alloc::", "mi_", "malloc", "free", "realloc", "Allocator"]),
    ("Vec / collections", ["vec::", "Vec::", "HashMap", "hashbrown", "AHash"]),
    ("Std / overhead",    ["black_box", "hint::", "option.rs", "cmp::", "PartialOrd",
                           "PartialEq", "slice::cmp", "ptr::read", "ptr::copy",
                           "memcmp", "memcpy", "memmove"]),
    ("Sort (init)",       ["sort::", "quicksort", "smallsort", "median"]),
    ("Main loop",         ["profile_search", "main"]),
    ("Thread / spawn",    ["thread::", "pthread", "thread_start", "spawn"]),
    ("dyld / system",     ["dyld", "libsystem", "boot_boot", "ignite", "_open"]),
]


def categorize(demangled_leaf: str, demangled_chain: list[str]) -> str:
    combined = demangled_leaf + " " + " ".join(demangled_chain[:5])
    for category, keywords in CATEGORY_RULES:
        for kw in keywords:
            if kw in combined:
                return category
    return "Other"


# ---------------------------------------------------------------------------
# Analyze
# ---------------------------------------------------------------------------

BOILERPLATE_FRAGMENTS = {
    "FnOnce", "call_once", "lang_start", "std::rt", "std::sys",
    "std::panicking", "std::thread::lifecycle", "boxed::Box",
    "core::ops::function",
}


def _find_meaningful_caller(chain_names: list[str]) -> str | None:
    """Walk the call chain to find the first non-boilerplate caller."""
    for name in chain_names:
        if any(frag in name for frag in BOILERPLATE_FRAGMENTS):
            continue
        if name in ("main", "start", "thread_start", "_pthread_start"):
            continue
        return name
    return None


def analyze_trace(trace_path: Path) -> dict:
    print(f"Exporting trace: {trace_path.name}...")

    # Get TOC to find the right schema
    toc_result = subprocess.run(
        ["xctrace", "export", "--input", str(trace_path), "--toc"],
        capture_output=True, text=True,
    )
    if toc_result.returncode != 0:
        sys.exit(f"xctrace export --toc failed: {toc_result.stderr}")

    # Export time-profile data
    xpath = '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]'
    export_result = subprocess.run(
        ["xctrace", "export", "--input", str(trace_path), "--xpath", xpath],
        capture_output=True, text=True,
    )
    if export_result.returncode != 0:
        sys.exit(f"xctrace export failed: {export_result.stderr}")

    xml_text = export_result.stdout
    if not xml_text.strip():
        sys.exit("Empty export — trace may not contain time-profile data")

    print("Parsing samples...")
    samples = parse_time_profile(xml_text)
    if not samples:
        print("No samples parsed. Try opening in Instruments.app for interactive analysis.")
        return {"total_weight_ms": 0, "categories": {}, "top_symbols": [], "samples": 0}

    # Filter to main thread only (where our code runs)
    main_samples = [s for s in samples if "Main Thread" in s["thread"] or "profile_search" in s["thread"]]
    if not main_samples:
        main_samples = samples  # fallback

    # Collect all mangled symbols for batch demangling
    all_mangled: set[str] = set()
    for s in main_samples:
        for f in s["frames"]:
            if f["name"]:
                all_mangled.add(f["name"])

    print(f"Demangling {len(all_mangled)} symbols...")
    demangle_map = batch_demangle(all_mangled)

    # Aggregate
    total_weight_ns = sum(s["weight_ns"] for s in main_samples)
    categories: Counter[str] = Counter()
    leaf_symbols: Counter[str] = Counter()
    # Also track leaf + first caller for richer view
    leaf_with_caller: Counter[str] = Counter()

    for s in main_samples:
        frames = s["frames"]
        w = s["weight_ns"]

        # Leaf = first frame (innermost)
        leaf_name = demangle_map.get(frames[0]["name"], frames[0]["name"])
        chain_names = [demangle_map.get(f["name"], f["name"]) for f in frames[1:]]

        cat = categorize(leaf_name, chain_names)
        categories[cat] += w

        # Build display name for leaf
        leaf_short = shorten_symbol(leaf_name)
        src = frames[0].get("source_file") or ""
        line = frames[0].get("source_line") or ""
        if src:
            fname = Path(src).name
            loc = f"{fname}:{line}" if line and line != "0" else fname
            leaf_display = f"{leaf_short}  ({loc})"
        else:
            leaf_display = leaf_short

        leaf_symbols[leaf_display] += w

        # Leaf + meaningful caller (skip std boilerplate)
        meaningful_caller = _find_meaningful_caller(chain_names)
        if meaningful_caller:
            caller_short = shorten_symbol(meaningful_caller)
            leaf_with_caller[f"{leaf_display}  <- {caller_short}"] += w

    top_symbols = [
        {"symbol": sym, "weight_ms": w / 1_000_000, "pct": w / total_weight_ns * 100}
        for sym, w in leaf_symbols.most_common(30)
    ]

    # Build call tree (top-down, from root toward leaf)
    call_tree = _build_call_tree(main_samples, demangle_map, total_weight_ns)

    # Build heavy backtraces (bottom-up, full stacks for top leaf symbols)
    heavy_backtraces = _build_heavy_backtraces(main_samples, demangle_map, total_weight_ns)

    return {
        "total_weight_ms": total_weight_ns / 1_000_000,
        "samples": len(main_samples),
        "categories": {
            cat: {"weight_ms": w / 1_000_000, "pct": w / total_weight_ns * 100}
            for cat, w in sorted(categories.items(), key=lambda x: -x[1])
        },
        "top_symbols": top_symbols,
        "top_with_caller": [
            {"display": d, "weight_ms": w / 1_000_000, "pct": w / total_weight_ns * 100}
            for d, w in leaf_with_caller.most_common(30)
        ],
        "call_tree": call_tree,
        "heavy_backtraces": heavy_backtraces,
    }


# ---------------------------------------------------------------------------
# Call Tree (top-down)
# ---------------------------------------------------------------------------

class CallTreeNode:
    __slots__ = ("name", "total_ns", "self_ns", "children")

    def __init__(self, name: str):
        self.name = name
        self.total_ns: int = 0
        self.self_ns: int = 0
        self.children: dict[str, CallTreeNode] = {}

    def child(self, name: str) -> CallTreeNode:
        if name not in self.children:
            self.children[name] = CallTreeNode(name)
        return self.children[name]


def _frame_display(frame: dict, demangle_map: dict[str, str]) -> str:
    name = demangle_map.get(frame["name"], frame["name"])
    short = shorten_symbol(name)
    src = frame.get("source_file") or ""
    line = frame.get("source_line") or ""
    if src:
        fname = Path(src).name
        loc = f"{fname}:{line}" if line and line != "0" else fname
        return f"{short}  ({loc})"
    return short


def _build_call_tree(
    samples: list[dict],
    demangle_map: dict[str, str],
    total_weight_ns: int,
) -> CallTreeNode:
    root = CallTreeNode("[root]")
    root.total_ns = total_weight_ns

    for s in samples:
        frames = s["frames"]
        w = s["weight_ns"]

        # Build stack from root → leaf (frames are leaf-first, reverse for top-down)
        stack = []
        for f in reversed(frames):
            name = demangle_map.get(f["name"], f["name"])
            short = shorten_symbol(name)
            src = f.get("source_file") or ""
            line = f.get("source_line") or ""
            if src:
                fname = Path(src).name
                loc = f"{fname}:{line}" if line and line != "0" else fname
                display = f"{short}  ({loc})"
            else:
                display = short
            stack.append(display)

        # Walk down the tree
        node = root
        for display in stack:
            node = node.child(display)
            node.total_ns += w
        # Self-time goes to the leaf
        node.self_ns += w

    return root


def _print_call_tree(
    node: CallTreeNode,
    total_ns: int,
    depth: int = 0,
    min_pct: float = 1.0,
    max_depth: int = 15,
):
    """Recursively print hot-path call tree, pruning branches below min_pct.

    Auto-collapses single-child chains (A→B→C with no branching shown as A → B → C)
    to reduce noise from wrapper/boilerplate frames.
    """
    if depth > max_depth:
        return

    children = sorted(node.children.values(), key=lambda c: -c.total_ns)
    hot_children = [c for c in children if (c.total_ns / total_ns * 100 if total_ns else 0) >= min_pct]

    for child in hot_children:
        # Collapse single-child chains with negligible self-time
        collapsed: list[str] = []
        walk = child
        while True:
            walk_children = sorted(walk.children.values(), key=lambda c: -c.total_ns)
            walk_hot = [c for c in walk_children if (c.total_ns / total_ns * 100 if total_ns else 0) >= min_pct]
            walk_self_pct = walk.self_ns / total_ns * 100 if total_ns else 0
            if len(walk_hot) == 1 and walk_self_pct < 0.5:
                collapsed.append(walk.name)
                walk = walk_hot[0]
            else:
                break

        # `walk` is now the first interesting node (branching or has self-time)
        pct = walk.total_ns / total_ns * 100 if total_ns else 0
        self_pct = walk.self_ns / total_ns * 100 if total_ns else 0
        self_tag = f" [self: {self_pct:.1f}%]" if self_pct >= 0.5 else ""

        indent = "  │ " * depth
        if collapsed:
            print(f"  {pct:5.1f}%{self_tag}  {indent}  ├─ ... → {walk.name}")
        else:
            print(f"  {pct:5.1f}%{self_tag}  {indent}  ├─ {walk.name}")

        _print_call_tree(walk, total_ns, depth + 1, min_pct, max_depth)


# ---------------------------------------------------------------------------
# Heavy Backtraces (bottom-up)
# ---------------------------------------------------------------------------

HEAVY_BOILERPLATE_EXACT = {
    "start", "main", "thread_start", "_pthread_start",
}
HEAVY_BOILERPLATE_FRAGMENTS = {
    "FnOnce", "call_once", "lang_start", "std::rt", "std::sys",
    "std::panicking", "std::thread::lifecycle",
}


def _build_heavy_backtraces(
    samples: list[dict],
    demangle_map: dict[str, str],
    total_weight_ns: int,
    top_n: int = 8,
) -> list[dict]:
    """For the top N leaf symbols, collect their most common full call stacks.

    Shows raw caller chains (only skipping OS entry points) so that inlined
    frames that are the actual callers aren't lost.
    """
    leaf_stacks: dict[str, list[tuple[int, list[str]]]] = {}

    for s in samples:
        frames = s["frames"]
        w = s["weight_ns"]
        if not frames:
            continue

        leaf_name = demangle_map.get(frames[0]["name"], frames[0]["name"])
        leaf_short = shorten_symbol(leaf_name)
        src = frames[0].get("source_file") or ""
        line = frames[0].get("source_line") or ""
        if src:
            fname = Path(src).name
            loc = f"{fname}:{line}" if line and line != "0" else fname
            leaf_display = f"{leaf_short}  ({loc})"
        else:
            leaf_display = leaf_short

        # Build caller chain — skip runtime entry-point boilerplate
        callers = []
        for f in frames[1:]:
            name = demangle_map.get(f["name"], f["name"])
            short = shorten_symbol(name)
            if short in HEAVY_BOILERPLATE_EXACT:
                continue
            if any(frag in name for frag in HEAVY_BOILERPLATE_FRAGMENTS):
                continue
            src2 = f.get("source_file") or ""
            line2 = f.get("source_line") or ""
            if src2:
                fname2 = Path(src2).name
                loc2 = f"{fname2}:{line2}" if line2 and line2 != "0" else fname2
                callers.append(f"{short}  ({loc2})")
            else:
                callers.append(short)

        if leaf_display not in leaf_stacks:
            leaf_stacks[leaf_display] = []
        leaf_stacks[leaf_display].append((w, callers))

    leaf_totals = {leaf: sum(w for w, _ in stacks) for leaf, stacks in leaf_stacks.items()}
    top_leaves = sorted(leaf_totals.items(), key=lambda x: -x[1])[:top_n]

    result = []
    for leaf_display, total_ns in top_leaves:
        pct = total_ns / total_weight_ns * 100

        stack_weights: Counter[tuple[str, ...]] = Counter()
        for w, callers in leaf_stacks[leaf_display]:
            key = tuple(callers[:12])
            stack_weights[key] += w

        top_stacks = []
        for stack_tuple, sw in stack_weights.most_common(3):
            top_stacks.append({
                "callers": list(stack_tuple),
                "weight_ms": sw / 1_000_000,
                "pct": sw / total_weight_ns * 100,
            })

        result.append({
            "leaf": leaf_display,
            "total_ms": total_ns / 1_000_000,
            "pct": pct,
            "stacks": top_stacks,
        })

    return result


def print_report(result: dict, path: Path) -> None:
    total_ms = result["total_weight_ms"]
    if total_ms == 0:
        print("No samples to report.")
        return

    print(f"\n{'=' * 78}")
    print(f"  Trace: {path.name}")
    print(f"  Samples: {result['samples']:,}   Total: {total_ms:.0f} ms")
    print(f"{'=' * 78}")

    print("\n  Category Breakdown:")
    print(f"  {'-' * 58}")
    for cat, info in result["categories"].items():
        pct = info["pct"]
        bar = "█" * int(pct / 2)
        print(f"  {pct:5.1f}%  {info['weight_ms']:7.0f}ms  {cat:<25s} {bar}")

    print("\n  Top Leaf Symbols:")
    print("  " + "-" * 74)
    print(f"  {'%':>6s}  {'ms':>7s}  Symbol")
    print(f"  {'-' * 74}")
    for entry in result["top_symbols"][:20]:
        print(f"  {entry['pct']:5.1f}%  {entry['weight_ms']:7.0f}ms  {entry['symbol']}")

    if result.get("top_with_caller"):
        print("\n  Top Leaf + Caller:")
        print("  " + "-" * 74)
        for entry in result["top_with_caller"][:15]:
            print(f"  {entry['pct']:5.1f}%  {entry['weight_ms']:7.0f}ms  {entry['display']}")

    # Call tree (top-down hot path)
    if result.get("call_tree"):
        print("\n  Call Tree (top-down, ≥1% of total):")
        print("  " + "-" * 74)
        _print_call_tree(result["call_tree"], result["call_tree"].total_ns)

    # Heavy backtraces (bottom-up)
    if result.get("heavy_backtraces"):
        print("\n  Heavy Backtraces (bottom-up, top leaf → callers):")
        print("  " + "=" * 74)
        for entry in result["heavy_backtraces"]:
            print(f"\n  {entry['pct']:5.1f}%  {entry['total_ms']:7.0f}ms  ▶ {entry['leaf']}")
            has_any_callers = any(stack["callers"] for stack in entry["stacks"])
            if not has_any_callers:
                print("         (callers inlined away — open in Instruments.app for full context)")
                continue
            for i, stack in enumerate(entry["stacks"]):
                if not stack["callers"]:
                    continue
                print(f"         stack #{i+1} ({stack['pct']:.1f}%):")
                for depth, caller in enumerate(stack["callers"]):
                    prefix = "           " + "  " * depth + "← "
                    print(f"{prefix}{caller}")

    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Profile matcher_rs using Xcode Instruments.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # --- record ---
    rec = sub.add_parser("record", help="Record a Time Profiler trace")
    rec.add_argument("--mode", default="process", choices=["is_match", "process"])
    rec.add_argument("--shape", default="literal", choices=["literal", "and", "not"])
    rec.add_argument("--dict", default="en", choices=["en", "cn"], dest="dict_lang")
    rec.add_argument("--rules", type=int, default=10_000)
    rec.add_argument("--pt", default="none",
                     choices=["none", "fanjian", "delete", "norm", "dn", "fdn", "pinyin", "pychar"])
    rec.add_argument("--seconds", type=int, default=10)
    rec.add_argument("--output", "-o", type=Path, default=None)
    rec.add_argument("--build", action="store_true", help="Force rebuild before recording")
    rec.add_argument("--analyze", action="store_true", help="Analyze immediately after recording")
    rec.add_argument("--open", action="store_true", help="Open trace in Instruments.app after recording")

    # --- analyze ---
    ana = sub.add_parser("analyze", help="Analyze an existing .trace bundle")
    ana.add_argument("traces", nargs="+", type=Path, help="One or more .trace files")
    ana.add_argument("--open", action="store_true", help="Also open in Instruments.app")

    # --- open ---
    opn = sub.add_parser("open", help="Open a .trace in Instruments.app")
    opn.add_argument("trace", type=Path)

    args = parser.parse_args()

    if args.command == "record":
        trace = record_profile(
            mode=args.mode,
            shape=args.shape,
            dict_lang=args.dict_lang,
            rules=args.rules,
            pt=args.pt,
            seconds=args.seconds,
            output=args.output,
            build=args.build,
        )
        if args.analyze:
            result = analyze_trace(trace)
            print_report(result, trace)
        if args.open:
            subprocess.run(["open", str(trace)])
        if not args.analyze and not args.open:
            print(f"Open in Instruments:  open {trace}")

    elif args.command == "analyze":
        for path in args.traces:
            if not path.exists():
                print(f"Warning: {path} not found, skipping", file=sys.stderr)
                continue
            result = analyze_trace(path)
            print_report(result, path)
            if args.open:
                subprocess.run(["open", str(path)])

    elif args.command == "open":
        if not args.trace.exists():
            sys.exit(f"Error: {args.trace} not found")
        subprocess.run(["open", str(args.trace)])


if __name__ == "__main__":
    main()
