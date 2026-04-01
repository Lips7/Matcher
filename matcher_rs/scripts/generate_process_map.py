#!/usr/bin/env python3
# /// script
# requires-python = ">=3.8"
# dependencies = [
#   "opencc",
#   "pypinyin",
# ]
# ///

from __future__ import annotations

import argparse
import importlib.metadata
import json
import math
import sys
import unicodedata
from fractions import Fraction
from pathlib import Path

import opencc
import pypinyin.pinyin_dict
from pypinyin import Style, lazy_pinyin

DELETE_CATEGORIES = frozenset(
    {
        "Cc",
        "Cf",
        "Mc",
        "Me",
        "Mn",
        "Pc",
        "Pd",
        "Pe",
        "Pf",
        "Pi",
        "Po",
        "Ps",
        "Sc",
        "Sk",
        "Sm",
        "So",
        "Zl",
        "Zp",
        "Zs",
    }
)
MAX_FRACTION_DENOMINATOR = 1000
NUMERIC_FRACTION_EPSILON = 1e-12
FANJIAN_CONFIGS = ("t2s", "tw2s", "hk2s")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generate matcher_rs/process_map/*.txt from OpenCC, unicodedata, and pypinyin."
        )
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="Repository root. Defaults to the current script's repo root.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify generated output matches the checked-in files without rewriting them.",
    )
    return parser.parse_args()


def iter_scalar_chars() -> list[str]:
    return [
        chr(codepoint)
        for codepoint in range(sys.maxunicode + 1)
        if not 0xD800 <= codepoint <= 0xDFFF
    ]


def format_numeric_value(value: float) -> str:
    if not math.isfinite(value):
        raise ValueError(f"unsupported numeric value: {value!r}")
    if value.is_integer():
        return str(int(value))

    fraction = Fraction(value).limit_denominator(MAX_FRACTION_DENOMINATOR)
    if abs(float(fraction) - value) <= NUMERIC_FRACTION_EPSILON:
        return f"{fraction.numerator}/{fraction.denominator}"
    return format(value, ".15g")


def build_fanjian_map(chars: list[str]) -> dict[str, str]:
    converters = {config: opencc.OpenCC(config) for config in FANJIAN_CONFIGS}
    mapping: dict[str, str] = {}
    for config in FANJIAN_CONFIGS:
        converter = converters[config]
        for char in chars:
            if char in mapping:
                continue
            converted = converter.convert(char)
            if converted == char or len(converted) != 1:
                continue
            mapping[char] = converted
    return mapping


def build_text_delete_codepoints(chars: list[str]) -> list[int]:
    return [ord(char) for char in chars if unicodedata.category(char) in DELETE_CATEGORIES]


def build_norm_map(chars: list[str]) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for char in chars:
        normalized = unicodedata.normalize("NFKC", char).casefold()
        if normalized != char:
            mapping[char] = normalized
    return mapping


def build_num_norm_map(chars: list[str]) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for char in chars:
        try:
            numeric = unicodedata.numeric(char)
        except ValueError:
            continue
        rendered = format_numeric_value(numeric)
        if rendered != char:
            mapping[char] = rendered
    return mapping


def build_pinyin_map() -> dict[str, str]:
    mapping: dict[str, str] = {}
    for codepoint in sorted(pypinyin.pinyin_dict.pinyin_dict):
        if 0xD800 <= codepoint <= 0xDFFF:
            continue
        char = chr(codepoint)
        syllables = lazy_pinyin(char, style=Style.NORMAL, strict=False)
        if len(syllables) != 1:
            continue
        syllable = syllables[0].strip().lower()
        if not syllable or syllable == char.casefold():
            continue
        mapping[char] = f" {syllable}"
    return mapping


def render_mapping(mapping: dict[str, str]) -> str:
    lines = [f"{key}\t{mapping[key]}" for key in sorted(mapping, key=ord)]
    return "\n".join(lines) + "\n"


def render_codepoints(codepoints: list[int]) -> str:
    return "\n".join(f"U+{codepoint:04X}" for codepoint in sorted(codepoints)) + "\n"


def collect_outputs(root: Path) -> tuple[dict[Path, str], dict[str, object]]:
    chars = iter_scalar_chars()
    process_map_dir = root / "matcher_rs" / "process_map"

    fanjian = build_fanjian_map(chars)
    text_delete = build_text_delete_codepoints(chars)
    norm = build_norm_map(chars)
    num_norm = build_num_norm_map(chars)
    pinyin = build_pinyin_map()

    outputs = {
        process_map_dir / "FANJIAN.txt": render_mapping(fanjian),
        process_map_dir / "TEXT-DELETE.txt": render_codepoints(text_delete),
        process_map_dir / "NORM.txt": render_mapping(norm),
        process_map_dir / "NUM-NORM.txt": render_mapping(num_norm),
        process_map_dir / "PINYIN.txt": render_mapping(pinyin),
    }

    manifest = {
        "python_version": sys.version.split()[0],
        "unicodedata_version": unicodedata.unidata_version,
        "dependencies": {
            "opencc": importlib.metadata.version("opencc"),
            "pypinyin": importlib.metadata.version("pypinyin"),
        },
        "fanjian_configs": list(FANJIAN_CONFIGS),
        "delete_categories": sorted(DELETE_CATEGORIES),
        "counts": {
            "fanjian": len(fanjian),
            "text_delete": len(text_delete),
            "norm": len(norm),
            "num_norm": len(num_norm),
            "pinyin": len(pinyin),
        },
    }
    outputs[process_map_dir / "manifest.json"] = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    return outputs, manifest


def main() -> int:
    args = parse_args()
    outputs, _manifest = collect_outputs(args.root.resolve())

    mismatches: list[Path] = []
    for path, content in outputs.items():
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current != content:
            mismatches.append(path)
            if not args.check:
                path.write_text(content, encoding="utf-8")

    if args.check:
        if mismatches:
            for path in mismatches:
                print(path.relative_to(args.root))
            return 1
        return 0

    for path in mismatches:
        print(path.relative_to(args.root))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
