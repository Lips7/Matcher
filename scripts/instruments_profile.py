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
    uv run scripts/instruments_profile.py record --analyze \
        --mode process --shape literal --dict en --rules 10000

    # Just record (opens in Instruments.app afterwards):
    uv run scripts/instruments_profile.py record \
        --mode is_match --dict cn --rules 500 --open

    # Analyze an existing .trace bundle:
    uv run scripts/instruments_profile.py analyze scripts/profile_records/prof_*.trace

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

REPO_ROOT = Path(__file__).resolve().parent.parent
PROFILE_RECORDS_DIR = REPO_ROOT / "scripts" / "profile_records"
PROFILE_BINARIES = ("profile_search", "profile_build")


# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------


def find_profiling_binary(name: str = "profile_search") -> Path:
    target = REPO_ROOT / "target" / "profiling" / "examples" / name
    if target.exists():
        return target
    deps = REPO_ROOT / "target" / "profiling" / "deps"
    if deps.exists():
        for f in deps.iterdir():
            if f.name.startswith(name) and f.is_file() and os.access(f, os.X_OK):
                return f
    return target


def build_profiling_binary(
    name: str = "profile_search", boundaries: bool = True
) -> Path:
    feature_flags = (
        ["--features", "matcher_rs/_profile_boundaries"] if boundaries else []
    )
    label = " +_profile_boundaries" if boundaries else ""
    print(f"Building {name} with --profile profiling{label}...")
    subprocess.run(
        [
            "cargo",
            "build",
            "--profile",
            "profiling",
            "--example",
            name,
            "-p",
            "matcher_rs",
        ]
        + feature_flags,
        cwd=REPO_ROOT,
        check=True,
    )
    binary = find_profiling_binary(name)
    if not binary.exists():
        sys.exit(f"Error: binary not found at {binary} after build")
    return binary


# ---------------------------------------------------------------------------
# Record
# ---------------------------------------------------------------------------


def record_profile(
    *,
    target: str = "search",
    scene: str | None = None,
    mode: str = "process",
    shape: str = "literal",
    dict_lang: str = "en",
    rules: int = 10_000,
    pt: str = "none",
    seconds: int = 10,
    output: Path | None = None,
    build: bool = True,
    boundaries: bool = True,
) -> Path:
    binary_name = f"profile_{target}"
    binary = find_profiling_binary(binary_name)
    if build:
        binary = build_profiling_binary(binary_name, boundaries=boundaries)
    elif not binary.exists():
        binary = build_profiling_binary(binary_name, boundaries=boundaries)

    binary_args: list[str] = []
    if target == "build":
        # profile_build: --dict, --rules, --pt, --seconds
        if output is None:
            PROFILE_RECORDS_DIR.mkdir(parents=True, exist_ok=True)
            output = PROFILE_RECORDS_DIR / f"prof_build_{dict_lang}_{rules}.trace"
        binary_args += [
            "--dict",
            dict_lang,
            "--rules",
            str(rules),
            "--pt",
            pt,
            "--seconds",
            str(seconds),
        ]
        print(
            f"Recording build: dict={dict_lang} rules={rules} pt={pt} seconds={seconds}s"
        )
    elif scene:
        if output is None:
            PROFILE_RECORDS_DIR.mkdir(parents=True, exist_ok=True)
            output = PROFILE_RECORDS_DIR / f"prof_{scene}.trace"
        binary_args += ["--scene", scene, "--seconds", str(seconds)]
        print(f"Recording: scene={scene} seconds={seconds}s")
    else:
        if output is None:
            PROFILE_RECORDS_DIR.mkdir(parents=True, exist_ok=True)
            output = PROFILE_RECORDS_DIR / f"prof_{mode}_{shape}_{dict_lang}_{rules}.trace"
        binary_args += [
            "--dict",
            dict_lang,
            "--rules",
            str(rules),
            "--mode",
            mode,
            "--shape",
            shape,
            "--pt",
            pt,
            "--seconds",
            str(seconds),
        ]
        print(
            f"Recording: mode={mode} shape={shape} dict={dict_lang} rules={rules} pt={pt} seconds={seconds}s"
        )

    if output.exists():
        subprocess.run(["rm", "-rf", str(output)], check=True)

    cmd = [
        "xctrace",
        "record",
        "--template",
        "Time Profiler",
        "--time-limit",
        f"{seconds + 5}s",
        "--output",
        str(output),
        "--launch",
        "--",
        str(binary),
        *binary_args,
    ]

    print(f"Output: {output}")

    result = subprocess.run(cmd)
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
# Inline Resolution via atos
# ---------------------------------------------------------------------------


def resolve_addresses_atos(
    binary_path: str,
    load_addr: str,
    addresses: list[str],
) -> dict[str, list[dict]]:
    """Resolve instruction addresses to inline call chains via `atos -i`.

    Returns dict mapping address → list of {name, file, line} dicts,
    ordered inner (leaf) → outer (caller).
    """
    if not addresses:
        return {}

    unique_addrs = sorted(set(addresses))

    try:
        cmd = ["atos", "-i", "-o", binary_path, "-l", load_addr] + unique_addrs
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        raw_output = proc.stdout.strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return {}

    if not raw_output:
        return {}

    # Pipe through rustfilt for demangling
    try:
        proc2 = subprocess.run(
            ["rustfilt"],
            input=raw_output,
            capture_output=True,
            text=True,
            timeout=10,
        )
        demangled_output = proc2.stdout.strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        demangled_output = raw_output

    # Split into per-address groups (separated by blank lines).
    groups = re.split(r"\n\s*\n", demangled_output)

    # atos -i with N addresses produces N groups.
    result: dict[str, list[dict]] = {}
    for addr, group in zip(unique_addrs, groups):
        chain = []
        for line in group.strip().splitlines():
            line = line.strip()
            if not line:
                continue
            # Format: "symbol (in binary_name) (file:line)" or just "symbol (in binary_name)"
            m = re.match(r"^(.+?)\s+\(in .+?\)\s+\((.+?):(\d+)\)$", line)
            if m:
                chain.append(
                    {
                        "name": shorten_symbol(m.group(1)),
                        "file": Path(m.group(2)).name,
                        "line": m.group(3),
                    }
                )
            else:
                m2 = re.match(r"^(.+?)\s+\(in .+?\)", line)
                name = shorten_symbol(m2.group(1)) if m2 else line
                chain.append({"name": name, "file": "", "line": ""})
        if chain:
            result[addr] = chain

    return result


def _build_source_attribution(
    samples: list[dict],
    atos_cache: dict[str, list[dict]],
    total_weight_ns: int,
) -> list[dict]:
    """Attribute each sample to the deepest frame in our codebase using atos inline chains.

    Returns list of {source, weight_ms, pct, inline_chain} sorted by weight.
    """
    OUR_FILES = {p.name for p in (REPO_ROOT / "matcher_rs" / "src").rglob("*.rs")}

    attribution: Counter[str] = Counter()
    attribution_chain: dict[str, str] = {}

    for s in samples:
        frames = s["frames"]
        w = s["weight_ns"]

        # Use the leaf frame's address to get the atos-resolved inline chain
        leaf_addr = frames[0].get("addr", "") if frames else ""
        chain = atos_cache.get(leaf_addr, [])

        # Find deepest frame in our code
        attributed = None
        chain_str = ""
        for i, frame in enumerate(chain):
            fname = frame.get("file", "")
            if fname in OUR_FILES:
                loc = f"{fname}:{frame['line']}" if frame.get("line") else fname
                name = frame["name"]
                # Strip generic parameters (e.g., "::for_each_match_value::<...closure#0>")
                name = re.sub(r"::<.*$", "", name)  # strip trailing generic
                name = re.sub(r"<[^>]*>", "", name)  # remove remaining generics
                if "::" in name:
                    parts = [p for p in name.split("::") if p]
                    name = "::".join(parts[-2:]) if len(parts) > 2 else name
                attributed = f"{name}  ({loc})"
                # Build abbreviated chain from this point outward
                chain_parts = []
                for j in range(i + 1, min(i + 4, len(chain))):
                    cname = chain[j]["name"]
                    cname = re.sub(r"::<.*$", "", cname)
                    cname = re.sub(r"<[^>]*>", "", cname)
                    if "::" in cname:
                        cparts = [p for p in cname.split("::") if p]
                        cname = cparts[-1] if len(cparts) > 1 else cname
                    chain_parts.append(cname)
                if chain_parts:
                    chain_str = " → ".join(chain_parts)
                break

        if attributed is None:
            # Fall back to leaf symbol name
            if chain:
                attributed = f"{chain[0]['name']}  ({chain[0].get('file', '?')}:{chain[0].get('line', '?')})"
            else:
                attributed = "unknown"

        attribution[attributed] += w
        if attributed not in attribution_chain and chain_str:
            attribution_chain[attributed] = chain_str

    result = []
    for source, w in attribution.most_common(25):
        entry = {
            "source": source,
            "weight_ms": w / 1_000_000,
            "pct": w / total_weight_ns * 100 if total_weight_ns else 0,
        }
        if source in attribution_chain:
            entry["chain"] = attribution_chain[source]
        result.append(entry)

    return result


# ---------------------------------------------------------------------------
# Parse XML
# ---------------------------------------------------------------------------


def parse_time_profile(xml_text: str) -> tuple[list[dict], dict]:
    """Parse xctrace time-profile XML into structured samples.

    Returns (samples, binary_info) where each sample has:
      - weight_ns: sample weight in nanoseconds
      - frames: list of {name, addr, source_file, source_line} dicts (leaf first)
      - thread: thread description string

    binary_info maps binary name → {path, load_addr}.
    """
    root = ET.fromstring(xml_text)

    # Build id→element lookup for ref resolution
    id_map: dict[str, ET.Element] = {}
    for elem in root.iter():
        eid = elem.get("id")
        if eid:
            id_map[eid] = elem

    # Collect binary metadata (path + load address)
    binary_info: dict[str, dict] = {}
    for binary_el in root.iter("binary"):
        bref = binary_el.get("ref")
        if bref and bref in id_map:
            binary_el = id_map[bref]
        bname = binary_el.get("name", "")
        bpath = binary_el.get("path", "")
        load_addr = binary_el.get("load-addr", "")
        if bname and bpath and load_addr:
            binary_info[bname] = {"path": bpath, "load_addr": load_addr}

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

        # Find backtrace — may be nested inside <tagged-backtrace>
        tbt_el = row.find("tagged-backtrace")
        if tbt_el is not None:
            ref = tbt_el.get("ref")
            if ref and ref in id_map:
                tbt_el = id_map[ref]
            bt_el = tbt_el.find("backtrace")
        else:
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
            addr = frame_el.get("addr", "")
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

            frames.append(
                {
                    "name": name,
                    "addr": addr,
                    "source_file": source_file,
                    "source_line": source_line,
                }
            )

        if frames:
            samples.append(
                {
                    "weight_ns": weight_ns,
                    "frames": frames,
                    "thread": thread_name,
                }
            )

    return samples, binary_info


# ---------------------------------------------------------------------------
# Categorization
# ---------------------------------------------------------------------------

# Matched against the atos-resolved innermost frame (leaf only, not caller chain).
# Order matters — first match wins.
CATEGORY_RULES: list[tuple[str, list[str]]] = [
    # ── Scan engines (AC automata) ────────────────────────────────────────
    (
        "DFA scan",
        [
            "aho_corasick::dfa",
            "aho_corasick::automaton",
            "aho_corasick::ahocorasick",
            "AcAutomaton",
            "BytewiseDFAEngine",
        ],
    ),
    ("Charwise scan", ["daachorse", "charwise"]),
    # ── Hit processing & rule evaluation ──────────────────────────────────
    (
        "Hit evaluation",
        [
            "eval_hit",
            "process_match",
            "check_word_boundary",
            "check_satisfaction",
            "SatisfactionMethod",
            "fold_noop_children_masks",
        ],
    ),
    (
        "Rule state",
        [
            "SimpleMatchState",
            "WordState",
            "ScanState",
            "ScanContext",
            "generation",
            "mark_positive",
            "state::",
        ],
    ),
    # ── Text transform pipeline ───────────────────────────────────────────
    (
        "Text transform",
        [
            "DeleteMatcher",
            "VariantNormMatcher",
            "NormalizeMatcher",
            "RomanizeMatcher",
            "EmojiNorm",
            "TransformFilter",
            "page_table",
            "variant_norm",
            "romanize",
            "transform::",
            "process_type",
        ],
    ),
    # ── Construction (automaton build, warm-up) ───────────────────────────
    (
        "Construction",
        [
            "build_trie",
            "fill_failure",
            "shuffle",
            "build_from_noncontiguous",
            "compile_automata",
            "build_current_bytewise",
            "build_current_charwise",
            "parse_rules",
            "build_process_type_tree",
            "SimpleMatcher::new",
        ],
    ),
    # ── Engine plumbing (dispatch, density, pattern tables) ───────────────
    (
        "Engine dispatch",
        [
            "ScanPlan",
            "BytewiseMatcher",
            "CharwiseMatcher",
            "ScanEngine",
            "text_char_density",
            "bytecount",
            "PatternIndex",
            "PatternDispatch",
            "DIRECT_RULE",
        ],
    ),
    # ── Tree walk / scan loop (catch-all for search.rs self-time) ─────────
    ("Tree walk", ["walk_and_scan", "scan_variant", "ProcessTypeBitNode"]),
    # ── Allocator ─────────────────────────────────────────────────────────
    (
        "Allocator",
        [
            "mi_free",
            "mi_malloc",
            "mi_zalloc",
            "mi_realloc",
            "mi_page",
            "_mi_",
            "malloc",
            "free",
            "realloc",
            "raw_vec",
            "finish_grow",
        ],
    ),
    # ── Std / system ──────────────────────────────────────────────────────
    (
        "Std / system",
        [
            "dyld",
            "libsystem",
            "pthread",
            "thread_start",
            "_platform_mem",
            "mach_absolute_time",
            "__bzero",
            "DYLD-STUB",
            "_tlv_get_addr",
            "clock_gettime",
            "memcmp",
            "memcpy",
            "memmove",
            "sort::",
            "quicksort",
            "smallsort",
            "black_box",
            "hint::",
            "drop_in_place",
        ],
    ),
    # ── Harness ───────────────────────────────────────────────────────────
    ("Harness", ["profile_search", "profile_build", "run_scene"]),
]


def categorize(
    demangled_leaf: str,
    atos_chain: list[dict] | None = None,
) -> str:
    """Categorize a sample using the atos-resolved inline chain.

    Walks the atos chain from innermost to outermost, returning the first
    match. This handles std functions (e.g. u32::partial_cmp) inlined into
    domain code (e.g. DFA scan) — the inner frame won't match, but the
    outer DFA frame will.  Falls back to the demangled leaf symbol.
    """
    if atos_chain:
        for frame in atos_chain:
            target = frame["name"]
            for category, keywords in CATEGORY_RULES:
                for kw in keywords:
                    if kw in target:
                        return category
    # Fallback: match on the raw demangled leaf symbol
    for category, keywords in CATEGORY_RULES:
        for kw in keywords:
            if kw in demangled_leaf:
                return category
    return "Other"


# ---------------------------------------------------------------------------
# Analyze
# ---------------------------------------------------------------------------

BOILERPLATE_FRAGMENTS = {
    "FnOnce",
    "call_once",
    "lang_start",
    "std::rt",
    "std::sys",
    "std::panicking",
    "std::thread::lifecycle",
    "boxed::Box",
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
        capture_output=True,
        text=True,
    )
    if toc_result.returncode != 0:
        sys.exit(f"xctrace export --toc failed: {toc_result.stderr}")

    # Export time-profile data
    xpath = '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]'
    export_result = subprocess.run(
        ["xctrace", "export", "--input", str(trace_path), "--xpath", xpath],
        capture_output=True,
        text=True,
    )
    if export_result.returncode != 0:
        sys.exit(f"xctrace export failed: {export_result.stderr}")

    xml_text = export_result.stdout
    if not xml_text.strip():
        sys.exit("Empty export — trace may not contain time-profile data")

    print("Parsing samples...")
    samples, binary_info = parse_time_profile(xml_text)
    if not samples:
        print(
            "No samples parsed. Try opening in Instruments.app for interactive analysis."
        )
        return {"total_weight_ms": 0, "categories": {}, "top_symbols": [], "samples": 0}

    # Filter to main thread only (where our code runs)
    main_samples = [
        s for s in samples if "Main Thread" in s["thread"] or "profile_" in s["thread"]
    ]
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

    # --- atos inline resolution ---
    # Collect all unique leaf addresses for atos resolution
    atos_cache: dict[str, list[dict]] = {}
    profile_binary = None
    for bname in PROFILE_BINARIES:
        profile_binary = binary_info.get(bname)
        if profile_binary:
            break
    if profile_binary:
        all_leaf_addrs: set[str] = set()
        for s in main_samples:
            if s["frames"]:
                addr = s["frames"][0].get("addr", "")
                if addr:
                    all_leaf_addrs.add(addr)
        if all_leaf_addrs:
            print(f"Resolving {len(all_leaf_addrs)} addresses via atos -i...")
            atos_cache = resolve_addresses_atos(
                profile_binary["path"],
                profile_binary["load_addr"],
                sorted(all_leaf_addrs),
            )
            if atos_cache:
                print(f"  Resolved {len(atos_cache)} inline chains")
            else:
                print("  (atos resolution failed — inline chains unavailable)")
    else:
        print("  (binary info not found in trace — skipping atos resolution)")

    # Aggregate
    total_weight_ns = sum(s["weight_ns"] for s in main_samples)
    categories: Counter[str] = Counter()
    leaf_symbols: Counter[str] = Counter()
    # Also track leaf + first caller for richer view
    leaf_with_caller: Counter[str] = Counter()
    # Track atos-resolved inline breakdown per leaf symbol
    # leaf_display → {innermost_name → weight_ns}
    inline_breakdown: dict[str, Counter[str]] = {}

    for s in main_samples:
        frames = s["frames"]
        w = s["weight_ns"]

        # Leaf = first frame (innermost)
        leaf_name = demangle_map.get(frames[0]["name"], frames[0]["name"])
        chain_names = [demangle_map.get(f["name"], f["name"]) for f in frames[1:]]

        # Use atos inline chain for categorization
        leaf_addr = frames[0].get("addr", "")
        atos_chain = atos_cache.get(leaf_addr)
        cat = categorize(leaf_name, atos_chain)
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

        # Record atos-resolved innermost frame for inline sub-breakdown
        if atos_chain and len(atos_chain) >= 2:
            inner = atos_chain[0]
            inner_name = inner["name"]
            if inner.get("file") and inner.get("line"):
                inner_label = f"{inner_name}  ({inner['file']}:{inner['line']})"
            else:
                inner_label = inner_name
            if leaf_display not in inline_breakdown:
                inline_breakdown[leaf_display] = Counter()
            inline_breakdown[leaf_display][inner_label] += w

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
    heavy_backtraces = _build_heavy_backtraces(
        main_samples, demangle_map, total_weight_ns, atos_cache
    )

    # Build source attribution (inline-resolved grouping by our code)
    source_attribution = (
        _build_source_attribution(main_samples, atos_cache, total_weight_ns)
        if atos_cache
        else []
    )

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
        "source_attribution": source_attribution,
        "inline_breakdown": {
            sym: [
                {
                    "inner": name,
                    "weight_ms": iw / 1_000_000,
                    "pct": iw / total_weight_ns * 100,
                }
                for name, iw in counter.most_common(8)
            ]
            for sym, counter in inline_breakdown.items()
            if counter.total() / total_weight_ns >= 0.02  # only for ≥2% symbols
        },
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
    hot_children = [
        c
        for c in children
        if (c.total_ns / total_ns * 100 if total_ns else 0) >= min_pct
    ]

    for child in hot_children:
        # Collapse single-child chains with negligible self-time
        collapsed: list[str] = []
        walk = child
        while True:
            walk_children = sorted(walk.children.values(), key=lambda c: -c.total_ns)
            walk_hot = [
                c
                for c in walk_children
                if (c.total_ns / total_ns * 100 if total_ns else 0) >= min_pct
            ]
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
    "start",
    "main",
    "thread_start",
    "_pthread_start",
}
HEAVY_BOILERPLATE_FRAGMENTS = {
    "FnOnce",
    "call_once",
    "lang_start",
    "std::rt",
    "std::sys",
    "std::panicking",
    "std::thread::lifecycle",
}


def _build_heavy_backtraces(
    samples: list[dict],
    demangle_map: dict[str, str],
    total_weight_ns: int,
    atos_cache: dict[str, list[dict]] | None = None,
    top_n: int = 8,
) -> list[dict]:
    """For the top N leaf symbols, collect their most common full call stacks.

    When atos_cache is available, uses inline-resolved chains from `atos -i`
    instead of the (often empty) XML backtraces.
    """
    if atos_cache is None:
        atos_cache = {}

    leaf_stacks: dict[str, list[tuple[int, list[str]]]] = {}
    # Also collect atos-resolved chains per leaf for inline display
    leaf_atos: dict[str, Counter[tuple[str, ...]]] = {}

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

        # Build caller chain from XML — skip runtime entry-point boilerplate
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

        # Collect atos-resolved inline chain for this leaf address
        leaf_addr = frames[0].get("addr", "")
        if leaf_addr and leaf_addr in atos_cache:
            chain = atos_cache[leaf_addr]
            # Skip the leaf itself (chain[0]), show callers (chain[1:])
            atos_callers = tuple(
                f"{f['name']}  ({f['file']}:{f['line']})"
                if f.get("file")
                else f["name"]
                for f in chain[1:]
            )
            if leaf_display not in leaf_atos:
                leaf_atos[leaf_display] = Counter()
            leaf_atos[leaf_display][atos_callers] += w

    leaf_totals = {
        leaf: sum(w for w, _ in stacks) for leaf, stacks in leaf_stacks.items()
    }
    top_leaves = sorted(leaf_totals.items(), key=lambda x: -x[1])[:top_n]

    result = []
    for leaf_display, total_ns in top_leaves:
        pct = total_ns / total_weight_ns * 100

        # Prefer atos-resolved chains over XML backtraces, but only when
        # atos gives us real inline callers (>0 entries after the leaf).
        atos_has_callers = (
            leaf_display in leaf_atos
            and leaf_atos[leaf_display]
            and any(callers for callers in leaf_atos[leaf_display] if callers)
        )
        if atos_has_callers:
            top_stacks = []
            for stack_tuple, sw in leaf_atos[leaf_display].most_common(3):
                top_stacks.append(
                    {
                        "callers": list(stack_tuple),
                        "weight_ms": sw / 1_000_000,
                        "pct": sw / total_weight_ns * 100,
                        "source": "atos",
                    }
                )
        else:
            stack_weights: Counter[tuple[str, ...]] = Counter()
            for w, callers in leaf_stacks[leaf_display]:
                key = tuple(callers[:12])
                stack_weights[key] += w

            top_stacks = []
            for stack_tuple, sw in stack_weights.most_common(3):
                top_stacks.append(
                    {
                        "callers": list(stack_tuple),
                        "weight_ms": sw / 1_000_000,
                        "pct": sw / total_weight_ns * 100,
                    }
                )

        result.append(
            {
                "leaf": leaf_display,
                "total_ms": total_ns / 1_000_000,
                "pct": pct,
                "stacks": top_stacks,
            }
        )

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

    inline_bd = result.get("inline_breakdown", {})
    print("\n  Top Leaf Symbols:")
    print("  " + "-" * 74)
    print(f"  {'%':>6s}  {'ms':>7s}  Symbol")
    print(f"  {'-' * 74}")
    for entry in result["top_symbols"][:20]:
        print(f"  {entry['pct']:5.1f}%  {entry['weight_ms']:7.0f}ms  {entry['symbol']}")
        # Show atos-resolved inline sub-breakdown if available
        inlines = inline_bd.get(entry["symbol"])
        if inlines:
            for j, item in enumerate(inlines):
                if item["pct"] < 0.5:
                    break
                connector = (
                    "└─"
                    if j == len(inlines) - 1 or inlines[j + 1]["pct"] < 0.5
                    else "├─"
                )
                print(
                    f"           {connector} {item['pct']:4.1f}%  {item['weight_ms']:6.0f}ms  {item['inner']}"
                )

    if result.get("top_with_caller"):
        print("\n  Top Leaf + Caller:")
        print("  " + "-" * 74)
        for entry in result["top_with_caller"][:15]:
            print(
                f"  {entry['pct']:5.1f}%  {entry['weight_ms']:7.0f}ms  {entry['display']}"
            )

    # Call tree (top-down hot path)
    if result.get("call_tree"):
        print("\n  Call Tree (top-down, ≥1% of total):")
        print("  " + "-" * 74)
        _print_call_tree(result["call_tree"], result["call_tree"].total_ns)

    # Heavy backtraces (bottom-up, with inline resolution)
    if result.get("heavy_backtraces"):
        print("\n  Heavy Backtraces (bottom-up, inline-resolved via atos):")
        print("  " + "=" * 74)
        for entry in result["heavy_backtraces"]:
            print(
                f"\n  {entry['pct']:5.1f}%  {entry['total_ms']:7.0f}ms  ▶ {entry['leaf']}"
            )
            has_any_callers = any(stack["callers"] for stack in entry["stacks"])
            if not has_any_callers:
                print(
                    "         (callers inlined away — open in Instruments.app for full context)"
                )
                continue
            for i, stack in enumerate(entry["stacks"]):
                if not stack["callers"]:
                    continue
                source = stack.get("source", "")
                label = "inline chain" if source == "atos" else f"stack #{i + 1}"
                print(f"         {label} ({stack['pct']:.1f}%):")
                for depth, caller in enumerate(stack["callers"]):
                    prefix = "           " + "  " * depth + "← "
                    print(f"{prefix}{caller}")

    # Source attribution (inline-resolved, grouped by our code)
    if result.get("source_attribution"):
        print("\n  Source Attribution (inline-resolved, by our code location):")
        print("  " + "=" * 74)
        for entry in result["source_attribution"]:
            if entry["pct"] < 0.5:
                break
            chain_suffix = f"  in: {entry['chain']}" if entry.get("chain") else ""
            print(
                f"  {entry['pct']:5.1f}%  {entry['weight_ms']:7.0f}ms  {entry['source']}{chain_suffix}"
            )

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
    rec.add_argument(
        "--target",
        default="search",
        choices=["search", "build"],
        help="Profiling target: 'search' (default) or 'build' (construction)",
    )
    rec.add_argument(
        "--scene",
        default=None,
        help="Named scene (e.g. en-search, cn-transform, all). Overrides --mode/--dict/etc.",
    )
    rec.add_argument("--mode", default="process", choices=["is_match", "process"])
    rec.add_argument(
        "--shape",
        default="literal",
        choices=["literal", "and", "not", "or", "word_boundary"],
    )
    rec.add_argument(
        "--dict", default="en", choices=["en", "cn", "mixed"], dest="dict_lang"
    )
    rec.add_argument("--rules", type=int, default=10_000)
    rec.add_argument(
        "--pt",
        default="none",
        choices=[
            "none",
            "variant_norm",
            "delete",
            "norm",
            "dn",
            "fdn",
            "romanize",
            "pychar",
        ],
    )
    rec.add_argument("--seconds", type=int, default=10)
    rec.add_argument("--output", "-o", type=Path, default=None)
    rec.add_argument(
        "--no-build",
        action="store_true",
        help="Skip rebuild (use existing binary as-is)",
    )
    rec.add_argument(
        "--no-boundaries",
        action="store_true",
        help="Omit _profile_boundaries feature (baseline: everything inlined)",
    )
    rec.add_argument(
        "--analyze", action="store_true", help="Analyze immediately after recording"
    )
    rec.add_argument(
        "--open",
        action="store_true",
        help="Open trace in Instruments.app after recording",
    )

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
            target=args.target,
            scene=args.scene,
            mode=args.mode,
            shape=args.shape,
            dict_lang=args.dict_lang,
            rules=args.rules,
            pt=args.pt,
            seconds=args.seconds,
            output=args.output,
            build=not args.no_build,
            boundaries=not args.no_boundaries,
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
