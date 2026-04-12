#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "opencc",
#   "pypinyin",
#   "requests",
# ]
# ///

from __future__ import annotations

import argparse
import importlib.metadata
import io
import json
import math
import sys
import unicodedata
import zipfile
from fractions import Fraction
from pathlib import Path

import opencc
import pypinyin.pinyin_dict
import requests
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
VARIANT_NORM_CONFIGS = ("t2s", "tw2s", "hk2s")

UNIHAN_ZIP_URL = "https://unicode.org/Public/UCD/latest/ucd/Unihan.zip"

# ---------------------------------------------------------------------------
# Kana → Romaji tables (Modified Hepburn)
# ---------------------------------------------------------------------------

HIRAGANA_ROMAJI: dict[int, str] = {
    0x3041: "a", 0x3042: "a", 0x3043: "i", 0x3044: "i", 0x3045: "u",
    0x3046: "u", 0x3047: "e", 0x3048: "e", 0x3049: "o", 0x304A: "o",
    0x304B: "ka", 0x304C: "ga", 0x304D: "ki", 0x304E: "gi", 0x304F: "ku",
    0x3050: "gu", 0x3051: "ke", 0x3052: "ge", 0x3053: "ko", 0x3054: "go",
    0x3055: "sa", 0x3056: "za", 0x3057: "shi", 0x3058: "ji", 0x3059: "su",
    0x305A: "zu", 0x305B: "se", 0x305C: "ze", 0x305D: "so", 0x305E: "zo",
    0x305F: "ta", 0x3060: "da", 0x3061: "chi", 0x3062: "di", 0x3063: "tsu",
    0x3064: "tsu", 0x3065: "du", 0x3066: "te", 0x3067: "de", 0x3068: "to",
    0x3069: "do", 0x306A: "na", 0x306B: "ni", 0x306C: "nu", 0x306D: "ne",
    0x306E: "no", 0x306F: "ha", 0x3070: "ba", 0x3071: "pa", 0x3072: "hi",
    0x3073: "bi", 0x3074: "pi", 0x3075: "fu", 0x3076: "bu", 0x3077: "pu",
    0x3078: "he", 0x3079: "be", 0x307A: "pe", 0x307B: "ho", 0x307C: "bo",
    0x307D: "po", 0x307E: "ma", 0x307F: "mi", 0x3080: "mu", 0x3081: "me",
    0x3082: "mo", 0x3083: "ya", 0x3084: "ya", 0x3085: "yu", 0x3086: "yu",
    0x3087: "yo", 0x3088: "yo", 0x3089: "ra", 0x308A: "ri", 0x308B: "ru",
    0x308C: "re", 0x308D: "ro", 0x308E: "wa", 0x308F: "wa", 0x3090: "wi",
    0x3091: "we", 0x3092: "wo", 0x3093: "n", 0x3094: "vu",
    0x3095: "ka", 0x3096: "ke",
}

KATAKANA_ROMAJI: dict[int, str] = {
    0x30A1: "a", 0x30A2: "a", 0x30A3: "i", 0x30A4: "i", 0x30A5: "u",
    0x30A6: "u", 0x30A7: "e", 0x30A8: "e", 0x30A9: "o", 0x30AA: "o",
    0x30AB: "ka", 0x30AC: "ga", 0x30AD: "ki", 0x30AE: "gi", 0x30AF: "ku",
    0x30B0: "gu", 0x30B1: "ke", 0x30B2: "ge", 0x30B3: "ko", 0x30B4: "go",
    0x30B5: "sa", 0x30B6: "za", 0x30B7: "shi", 0x30B8: "ji", 0x30B9: "su",
    0x30BA: "zu", 0x30BB: "se", 0x30BC: "ze", 0x30BD: "so", 0x30BE: "zo",
    0x30BF: "ta", 0x30C0: "da", 0x30C1: "chi", 0x30C2: "di", 0x30C3: "tsu",
    0x30C4: "tsu", 0x30C5: "du", 0x30C6: "te", 0x30C7: "de", 0x30C8: "to",
    0x30C9: "do", 0x30CA: "na", 0x30CB: "ni", 0x30CC: "nu", 0x30CD: "ne",
    0x30CE: "no", 0x30CF: "ha", 0x30D0: "ba", 0x30D1: "pa", 0x30D2: "hi",
    0x30D3: "bi", 0x30D4: "pi", 0x30D5: "fu", 0x30D6: "bu", 0x30D7: "pu",
    0x30D8: "he", 0x30D9: "be", 0x30DA: "pe", 0x30DB: "ho", 0x30DC: "bo",
    0x30DD: "po", 0x30DE: "ma", 0x30DF: "mi", 0x30E0: "mu", 0x30E1: "me",
    0x30E2: "mo", 0x30E3: "ya", 0x30E4: "ya", 0x30E5: "yu", 0x30E6: "yu",
    0x30E7: "yo", 0x30E8: "yo", 0x30E9: "ra", 0x30EA: "ri", 0x30EB: "ru",
    0x30EC: "re", 0x30ED: "ro", 0x30EE: "wa", 0x30EF: "wa", 0x30F0: "wi",
    0x30F1: "we", 0x30F2: "wo", 0x30F3: "n", 0x30F4: "vu",
    0x30F5: "ka", 0x30F6: "ke",
    0x30F7: "va", 0x30F8: "vi", 0x30F9: "ve", 0x30FA: "vo",
}

# ---------------------------------------------------------------------------
# Korean Revised Romanization tables
# ---------------------------------------------------------------------------

HANGUL_SYLLABLE_BASE = 0xAC00
HANGUL_SYLLABLE_END = 0xD7A3

RR_INITIAL = [
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp",
    "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h",
]
RR_MEDIAL = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o",
    "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu",
    "eu", "ui", "i",
]
RR_FINAL = [
    "", "g", "kk", "gs", "n", "nj", "nh", "d", "l",
    "lg", "lm", "lb", "ls", "lt", "lp", "lh", "m", "b",
    "bs", "s", "ss", "ng", "j", "ch", "k", "t", "p", "h",
]

# Half-width katakana → full-width katakana (1:1 codepoint mappings only)
HALFWIDTH_TO_FULLWIDTH_KATAKANA: dict[int, int] = {
    0xFF66: 0x30F2,  # ｦ → ヲ
    0xFF67: 0x30A1,  # ｧ → ァ
    0xFF68: 0x30A3,  # ｨ → ィ
    0xFF69: 0x30A5,  # ｩ → ゥ
    0xFF6A: 0x30A7,  # ｪ → ェ
    0xFF6B: 0x30A9,  # ｫ → ォ
    0xFF6C: 0x30E3,  # ｬ → ャ
    0xFF6D: 0x30E5,  # ｭ → ュ
    0xFF6E: 0x30E7,  # ｮ → ョ
    0xFF6F: 0x30C3,  # ｯ → ッ
    0xFF70: 0x30FC,  # ｰ → ー
    0xFF71: 0x30A2,  # ｱ → ア
    0xFF72: 0x30A4,  # ｲ → イ
    0xFF73: 0x30A6,  # ｳ → ウ
    0xFF74: 0x30A8,  # ｴ → エ
    0xFF75: 0x30AA,  # ｵ → オ
    0xFF76: 0x30AB,  # ｶ → カ
    0xFF77: 0x30AD,  # ｷ → キ
    0xFF78: 0x30AF,  # ｸ → ク
    0xFF79: 0x30B1,  # ｹ → ケ
    0xFF7A: 0x30B3,  # ｺ → コ
    0xFF7B: 0x30B5,  # ｻ → サ
    0xFF7C: 0x30B7,  # ｼ → シ
    0xFF7D: 0x30B9,  # ｽ → ス
    0xFF7E: 0x30BB,  # ｾ → セ
    0xFF7F: 0x30BD,  # ｿ → ソ
    0xFF80: 0x30BF,  # ﾀ → タ
    0xFF81: 0x30C1,  # ﾁ → チ
    0xFF82: 0x30C4,  # ﾂ → ツ
    0xFF83: 0x30C6,  # ﾃ → テ
    0xFF84: 0x30C8,  # ﾄ → ト
    0xFF85: 0x30CA,  # ﾅ → ナ
    0xFF86: 0x30CB,  # ﾆ → ニ
    0xFF87: 0x30CC,  # ﾇ → ヌ
    0xFF88: 0x30CD,  # ﾈ → ネ
    0xFF89: 0x30CE,  # ﾉ → ノ
    0xFF8A: 0x30CF,  # ﾊ → ハ
    0xFF8B: 0x30D2,  # ﾋ → ヒ
    0xFF8C: 0x30D5,  # ﾌ → フ
    0xFF8D: 0x30D8,  # ﾍ → ヘ
    0xFF8E: 0x30DB,  # ﾎ → ホ
    0xFF8F: 0x30DE,  # ﾏ → マ
    0xFF90: 0x30DF,  # ﾐ → ミ
    0xFF91: 0x30E0,  # ﾑ → ム
    0xFF92: 0x30E1,  # ﾒ → メ
    0xFF93: 0x30E2,  # ﾓ → モ
    0xFF94: 0x30E4,  # ﾔ → ヤ
    0xFF95: 0x30E6,  # ﾕ → ユ
    0xFF96: 0x30E8,  # ﾖ → ヨ
    0xFF97: 0x30E9,  # ﾗ → ラ
    0xFF98: 0x30EA,  # ﾘ → リ
    0xFF99: 0x30EB,  # ﾙ → ル
    0xFF9A: 0x30EC,  # ﾚ → レ
    0xFF9B: 0x30ED,  # ﾛ → ロ
    0xFF9C: 0x30EF,  # ﾜ → ワ
    0xFF9D: 0x30F3,  # ﾝ → ン
    # U+FF9E (ﾞ dakuten) and U+FF9F (ﾟ handakuten) are combining marks,
    # skipped since the page-table is 1:1 codepoint only.
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generate matcher_rs/process_map/*.txt from OpenCC, Unihan, "
            "unicodedata, and pypinyin."
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


def format_numeric_value(value: int | float) -> str:
    if not math.isfinite(value):
        raise ValueError(f"unsupported numeric value: {value!r}")
    if isinstance(value, int) or value.is_integer():
        return str(int(value))

    fraction = Fraction(value).limit_denominator(MAX_FRACTION_DENOMINATOR)
    if abs(float(fraction) - value) <= NUMERIC_FRACTION_EPSILON:
        return f"{fraction.numerator}/{fraction.denominator}"
    return format(value, ".15g")


# ---------------------------------------------------------------------------
# Unihan data download and parsing
# ---------------------------------------------------------------------------

_unihan_cache: dict[str, dict[int, str]] | None = None


def _download_unihan() -> dict[str, dict[int, str]]:
    """Download and parse Unihan.zip, returning {field_name: {codepoint: value}}."""
    global _unihan_cache
    if _unihan_cache is not None:
        return _unihan_cache

    print("Downloading Unihan.zip from unicode.org...", file=sys.stderr)
    resp = requests.get(UNIHAN_ZIP_URL, timeout=60)
    resp.raise_for_status()

    fields: dict[str, dict[int, str]] = {}
    with zipfile.ZipFile(io.BytesIO(resp.content)) as zf:
        for name in zf.namelist():
            if not name.endswith(".txt"):
                continue
            with zf.open(name) as f:
                for raw_line in f:
                    line = raw_line.decode("utf-8").strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t", 2)
                    if len(parts) < 3:
                        continue
                    cp_str, field, value = parts
                    cp = int(cp_str.removeprefix("U+"), 16)
                    fields.setdefault(field, {})[cp] = value

    _unihan_cache = fields
    return fields


def _parse_unihan_simplified_variants() -> dict[int, int]:
    """Parse kSimplifiedVariant: Kyūjitai → Shinjitai for JP-only kanji."""
    unihan = _download_unihan()
    raw = unihan.get("kSimplifiedVariant", {})
    result: dict[int, int] = {}
    for cp, value in raw.items():
        # kSimplifiedVariant can list multiple codepoints; take first only
        first = value.split()[0] if value else ""
        if first.startswith("U+"):
            target = int(first.removeprefix("U+"), 16)
            if target != cp:
                result[cp] = target
    return result


def _parse_unihan_hangul() -> dict[int, int]:
    """Parse kHangul: Hanja → Hangul syllable."""
    unihan = _download_unihan()
    raw = unihan.get("kHangul", {})
    result: dict[int, int] = {}
    for cp, value in raw.items():
        # kHangul format: "한:0E" or just "한", possibly multiple separated by space
        first = value.split()[0]
        hangul_char = first.split(":")[0]
        if len(hangul_char) == 1:
            target = ord(hangul_char)
            if target != cp:
                result[cp] = target
    return result


# ---------------------------------------------------------------------------
# VariantNorm: merged Chinese + Japanese + Korean variant normalization
# ---------------------------------------------------------------------------

def build_variant_norm_map(chars: list[str]) -> dict[str, str]:
    # 1. Chinese Traditional → Simplified (existing, highest priority)
    converters = {config: opencc.OpenCC(config) for config in VARIANT_NORM_CONFIGS}
    mapping: dict[str, str] = {}
    for config in VARIANT_NORM_CONFIGS:
        converter = converters[config]
        for char in chars:
            if char in mapping:
                continue
            converted = converter.convert(char)
            if converted == char or len(converted) != 1:
                continue
            mapping[char] = converted

    # 2. Japanese Kyūjitai → Shinjitai (skip codepoints already in Chinese map)
    kyujitai = _parse_unihan_simplified_variants()
    for cp, target_cp in sorted(kyujitai.items()):
        char = chr(cp)
        if char not in mapping:
            target = chr(target_cp)
            if target != char:
                mapping[char] = target

    # 3. Half-width katakana → full-width katakana
    for hw_cp, fw_cp in sorted(HALFWIDTH_TO_FULLWIDTH_KATAKANA.items()):
        char = chr(hw_cp)
        if char not in mapping:
            mapping[char] = chr(fw_cp)

    # Note: Korean Hanja → Hangul is NOT included in VariantNorm because Hanja
    # codepoints overlap with common CJK ideographs (e.g. 西安 Xi'an → 서안).
    # Hanja is a pronunciation/script conversion, not variant normalization.

    return mapping


# ---------------------------------------------------------------------------
# Delete / Normalize / NumNorm (unchanged)
# ---------------------------------------------------------------------------

def build_text_delete_codepoints(chars: list[str]) -> list[int]:
    return [ord(char) for char in chars if unicodedata.category(char) in DELETE_CATEGORIES]


def build_norm_map(chars: list[str], num_norm: dict[str, str]) -> dict[str, str]:
    combining_categories = frozenset({"Mn", "Mc", "Me"})
    mapping: dict[str, str] = {}
    for char in chars:
        # Skip codepoints already handled by NUM-NORM (which takes priority
        # at build time because it is loaded second into the same HashMap).
        if char in num_norm:
            continue
        nfkd = unicodedata.normalize("NFKD", char)
        stripped = "".join(
            c for c in nfkd if unicodedata.category(c) not in combining_categories
        )
        normalized = unicodedata.normalize("NFKC", stripped).casefold()
        if normalized and normalized != char:
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


# ---------------------------------------------------------------------------
# Romanize: merged Chinese Pinyin + Japanese kana Romaji + Korean RR
# ---------------------------------------------------------------------------

def _build_hangul_rr() -> dict[str, str]:
    """Pre-generate Revised Romanization for all 11,172 Hangul syllable blocks."""
    mapping: dict[str, str] = {}
    for cp in range(HANGUL_SYLLABLE_BASE, HANGUL_SYLLABLE_END + 1):
        idx = cp - HANGUL_SYLLABLE_BASE
        initial = idx // (21 * 28)
        medial = (idx % (21 * 28)) // 28
        final = idx % 28
        roman = RR_INITIAL[initial] + RR_MEDIAL[medial] + RR_FINAL[final]
        if roman:
            mapping[chr(cp)] = f" {roman}"
    return mapping


def _build_kana_romaji() -> dict[str, str]:
    """Build kana → romaji mapping from static tables."""
    mapping: dict[str, str] = {}
    for cp, romaji in HIRAGANA_ROMAJI.items():
        mapping[chr(cp)] = f" {romaji}"
    for cp, romaji in KATAKANA_ROMAJI.items():
        mapping[chr(cp)] = f" {romaji}"
    return mapping


def build_romanize_map() -> dict[str, str]:
    # 1. Chinese Pinyin (highest priority for shared CJK ideographs)
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

    # 2. Japanese kana → Romaji (disjoint Unicode blocks, no conflicts)
    kana_romaji = _build_kana_romaji()
    for char, romaji in kana_romaji.items():
        if char not in mapping:
            mapping[char] = romaji

    # 3. Korean Hangul → Revised Romanization (disjoint Unicode block)
    hangul_rr = _build_hangul_rr()
    for char, rr in hangul_rr.items():
        if char not in mapping:
            mapping[char] = rr

    return mapping


# ---------------------------------------------------------------------------
# Emoji Normalization: CLDR short names → snake_case English words
# ---------------------------------------------------------------------------

CLDR_ANNOTATIONS_URL = (
    "https://raw.githubusercontent.com/unicode-org/cldr/main"
    "/common/annotations/en.xml"
)

# Emoji modifier codepoints to strip (map to empty string).
EMOJI_MODIFIER_CODEPOINTS = {
    0x200D,   # Zero Width Joiner
    0xFE0F,   # Variation Selector-16
    0xFE0E,   # Variation Selector-15
    *range(0x1F3FB, 0x1F400),  # Skin tone modifiers (Fitzpatrick types 1-2 through 6)
}

# Regional indicator symbols (U+1F1E6–U+1F1FF) → lowercase letter.
REGIONAL_INDICATOR_BASE = 0x1F1E6


def _download_cldr_annotations() -> str:
    resp = requests.get(CLDR_ANNOTATIONS_URL, timeout=30)
    resp.raise_for_status()
    return resp.text


def _parse_cldr_tts(xml_text: str) -> dict[str, str]:
    """Extract type='tts' annotations from CLDR annotationsDerived XML."""
    import xml.etree.ElementTree as ET

    root = ET.fromstring(xml_text)
    tts: dict[str, str] = {}
    for ann in root.iter("annotation"):
        if ann.get("type") != "tts":
            continue
        cp_str = ann.get("cp", "")
        text = (ann.text or "").strip()
        if len(cp_str) == 1 and text:
            tts[cp_str] = text
    return tts


def _to_snake_case(name: str) -> str:
    """Convert CLDR short name to snake_case: 'thumbs up' → 'thumbs_up'."""
    return "_".join(name.lower().split())


def build_emoji_norm_map() -> dict[str, str]:
    xml_text = _download_cldr_annotations()
    cldr_tts = _parse_cldr_tts(xml_text)

    mapping: dict[str, str] = {}

    # 1. CLDR short names for So (Other Symbol) codepoints >= U+0200
    for char, name in cldr_tts.items():
        code = ord(char)
        cat = unicodedata.category(char)
        if cat != "So" or code < 0x200:
            continue
        snake = _to_snake_case(name)
        if snake:
            mapping[char] = f" {snake}"

    # 2. Modifier codepoints → empty string (strip them)
    for cp in sorted(EMOJI_MODIFIER_CODEPOINTS):
        char = chr(cp)
        if char not in mapping:
            mapping[char] = ""

    # 3. Regional indicator symbols → lowercase letter
    for offset in range(26):
        char = chr(REGIONAL_INDICATOR_BASE + offset)
        if char not in mapping:
            mapping[char] = chr(ord("a") + offset)

    return mapping


# ---------------------------------------------------------------------------
# Output rendering and main
# ---------------------------------------------------------------------------

def render_mapping(mapping: dict[str, str]) -> str:
    lines = [f"{key}\t{mapping[key]}" for key in sorted(mapping)]
    return "\n".join(lines) + "\n"


def render_codepoints(codepoints: list[int]) -> str:
    return "\n".join(f"U+{codepoint:04X}" for codepoint in sorted(codepoints)) + "\n"


def collect_outputs(root: Path) -> tuple[dict[Path, str], dict[str, str | dict[str, str] | list[str] | dict[str, int]]]:
    chars = iter_scalar_chars()
    process_map_dir = root / "matcher_rs" / "process_map"

    variant_norm = build_variant_norm_map(chars)
    text_delete = build_text_delete_codepoints(chars)
    num_norm = build_num_norm_map(chars)
    norm = build_norm_map(chars, num_norm)
    romanize = build_romanize_map()
    emoji_norm = build_emoji_norm_map()

    outputs = {
        process_map_dir / "VARIANT_NORM.txt": render_mapping(variant_norm),
        process_map_dir / "TEXT-DELETE.txt": render_codepoints(text_delete),
        process_map_dir / "NORM.txt": render_mapping(norm),
        process_map_dir / "NUM-NORM.txt": render_mapping(num_norm),
        process_map_dir / "ROMANIZE.txt": render_mapping(romanize),
        process_map_dir / "EMOJI_NORM.txt": render_mapping(emoji_norm),
    }

    manifest = {
        "python_version": sys.version.split()[0],
        "unicodedata_version": unicodedata.unidata_version,
        "dependencies": {
            "opencc": importlib.metadata.version("opencc"),
            "pypinyin": importlib.metadata.version("pypinyin"),
        },
        "variant_norm_configs": list(VARIANT_NORM_CONFIGS),
        "variant_norm_sources": [
            "opencc",
            "unihan_kyujitai",
            "halfwidth_katakana",
        ],
        "romanize_sources": [
            "pypinyin",
            "kana_romaji",
            "korean_rr",
        ],
        "emoji_norm_sources": [
            "cldr_annotations_derived_en",
        ],
        "delete_categories": sorted(DELETE_CATEGORIES),
        "counts": {
            "variant_norm": len(variant_norm),
            "text_delete": len(text_delete),
            "norm": len(norm),
            "num_norm": len(num_norm),
            "romanize": len(romanize),
            "emoji_norm": len(emoji_norm),
        },
    }
    outputs[process_map_dir / "manifest.json"] = (
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
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
