# Design

This document explains how `matcher_rs` works by walking through a concrete example end to end — from rule construction to query evaluation. Deep-dive sections at the end cover SIMD engines, state management, and memory efficiency.

## Table of Contents

- [Running Example](#running-example)
- [1. Construction](#1-construction)
  - [1.1 Parse Rules](#11-parse-rules)
  - [1.2 Build Transform Trie](#12-build-transform-trie)
  - [1.3 Compile Scan Engines](#13-compile-scan-engines)
  - [1.4 Assemble](#14-assemble)
- [2. Query](#2-query-processhello-你好世界-china-is-cool)
  - [2.1 Prepare State](#21-prepare-state)
  - [2.2 Walk the Trie](#22-walk-the-trie)
  - [2.3 Evaluate](#23-evaluate-pass-2)
- [3. Fast Path: AllSimple](#3-fast-path-allsimple)
- [Deep Dives](#deep-dives)
  - [Text Transformation Engines](#text-transformation-engines)
  - [Density-Based Engine Dispatch](#density-based-engine-dispatch)
  - [State Management](#state-management)
  - [Memory Efficiency](#memory-efficiency)
  - [Feature Flags](#feature-flags)
  - [Compiled Transformation Tables](#compiled-transformation-tables)

---

## Running Example

Three rules, each using a different text transformation:

| Rule | ProcessType | word_id | Pattern |
|------|-------------|---------|---------|
| R1 | `None` | 1 | `"hello&world"` |
| R2 | `VariantNorm \| Delete` | 2 | `"你好"` |
| R3 | `Romanize` | 3 | `"zhongguo"` |

Query text: `"Hello! 你好世界 china is cool"`

We will trace both construction and query evaluation using these rules.

---

## 1. Construction

`SimpleMatcher::new` (in `build.rs`) runs four stages.

### 1.1 Parse Rules

`parse_rules` processes each rule string:

**R1: `"hello&world"` under `ProcessType::None`**

Split on `&`/`~` → two AND segments: `["hello", "world"]`. Each segment is then split on `|` for OR alternatives (neither has any here). No NOT segments. `and_count = 2`, shape = `Bitmask` (both counts are 1, total ≤ 64), `has_not = false`.

Each sub-pattern is emitted via `reduce_text_process_emit(process_type - Delete, pattern)`. Since `None - Delete = None`, both `"hello"` and `"world"` emit themselves unchanged.

**R2: `"你好"` under `VariantNorm | Delete`**

Single AND segment: `["你好"]`. `and_count = 1`, simple rule. Emitted under `VariantNorm | Delete - Delete = VariantNorm`. The VariantNorm transform normalizes CJK variant forms (Chinese T→S, Japanese Kyūjitai→Shinjitai, half-width katakana→full-width); `"你好"` is already normalized, so it emits unchanged as `"你好"`.

**R3: `"zhongguo"` under `Romanize`**

Single AND segment: `["zhongguo"]`. Simple rule. Emitted under `Romanize - Delete = Romanize`. Since `"zhongguo"` is pure ASCII and Romanize only transforms CJK, it emits unchanged.

**Why subtract Delete?** Input text is Delete-transformed before scanning, so patterns are stored verbatim and matched against already-deleted text. Double-deleting would break matches.

**OR alternatives (`|`):** Each segment (between `&`/`~` operators) may contain `|`-separated alternatives. For example, `"color|colour&bright"` produces two AND segments: segment 0 with alternatives `["color", "colour"]` and segment 1 with `["bright"]`. Each alternative becomes a separate AC pattern sharing the same `offset` — any single alternative matching satisfies that segment. `|` binds tighter than `&`/`~`, so `"a|b&c|d~e|f"` means (a OR b) AND (c OR d) AND NOT (e OR f). OR alternatives preserve their parent's `PatternKind` (Simple, And, or Not), so single-rule OR patterns like `"color|colour"` remain eligible for the AllSimple fast path.

**Word boundaries (`\b`):** Each sub-pattern (after `&`/`~`/`|` splitting) may have `\b` at its start and/or end. `"\bcat\b"` matches "cat" only when surrounded by non-word characters (or text edges). Boundary checking happens inside the AC scan loop using hit positions — a byte-level check of `is_word_byte(text[start-1])` and `is_word_byte(text[end])`. Patterns with boundaries cannot use `DIRECT_RULE_BIT` and disable `AllSimple` mode, falling back to the General path.

After deduplication, we have a flat pattern table:

```
dedup_patterns: ["hello", "world", "你好", "zhongguo"]
dedup_entries:
  [0] → PatternEntry { rule_idx: 0, offset: 0, pt_index: 0, kind: And }   # "hello" → R1
  [1] → PatternEntry { rule_idx: 0, offset: 1, pt_index: 0, kind: And }   # "world" → R1
  [2] → PatternEntry { rule_idx: 1, offset: 0, pt_index: 1, kind: Simple }# "你好"  → R2
  [3] → PatternEntry { rule_idx: 2, offset: 0, pt_index: 2, kind: Simple }# "zhongguo" → R3
```

### 1.2 Build Transform Trie

The three `ProcessType` values — `{None, VariantNorm|Delete, Romanize}` — are decomposed into single-bit steps and merged into a shared-prefix trie:

```
[0] Root (None) ← terminates: pt_index_mask has bit 0 (None)
 ├─[1] VariantNorm
 │  └─[2] Delete ← terminates: pt_index_mask has bit 1 (VariantNorm|Delete)
 └─[3] Romanize  ← terminates: pt_index_mask has bit 2 (Romanize)
```

Each node caches a `&'static TransformStep` reference from the global step registry. The root's step is `None` (no transformation). `pt_index_mask` is a `u64` bitmask of which compact indices terminate at or pass through each node.

**Sequential index table** (`pt_index_table`): maps raw `ProcessType::bits()` → compact 0..N. `None` always gets index 0. This compact index lets `PatternEntry.pt_index` fit in a `u8`.

### 1.3 Compile Scan Engines

`ScanPlan::compile` receives the deduplicated patterns and builds:

**PatternIndex**: maps each pattern's dedup index to its `PatternEntry` slice. Also builds the value map — for simple single-entry patterns, the value is `rule_idx | DIRECT_RULE_BIT` (bit 31 set), encoding the rule index directly in the automaton hit value so the scan hot path skips the entry table lookup.

**Bytewise engine** (`BytewiseMatcher`): compiled from **all** patterns. With the `dfa` feature, uses `aho-corasick` DFA for maximum throughput. Otherwise falls back to `daachorse` bytewise DAAC.

**Charwise engine** (`CharwiseMatcher`): compiled from **all** patterns. Always built. CJK characters are 3 UTF-8 bytes, so charwise does 1 state transition vs 3 for bytewise — ~1.6–1.9× faster on non-ASCII text.

**Engine selection** is density-based at runtime: a SIMD scan counts non-ASCII bytes in the text. Below the crossover threshold (~40% CJK characters ≈ 0.67 non-ASCII byte fraction), bytewise/DFA is faster; above it, charwise wins.

### 1.4 Assemble

```rust
SimpleMatcher {
    tree: Vec<ProcessTypeBitNode>,  // the 4-node trie above
    mode: SearchMode::General,      // R1 has &-operator → not AllSimple
    scan: ScanPlan { bytewise, charwise, pattern_index },
    rules: RuleSet { hot: [RuleHot; 3], cold: [RuleCold; 3] },  // RuleHot only stores segment_counts
}
```

`SearchMode::General` because R1 uses `&` (not a simple literal). If all rules were pure literals under a single `ProcessType`, mode would be `AllSimple` — enabling a fast path that bypasses the trie and state machinery entirely.

---

## 2. Query: `process("Hello! 你好世界 china is cool")`

### 2.1 Prepare State

The thread-local `SimpleMatchState` bumps its `generation` counter (say, to `gen=5`). No arrays are zeroed — stale entries from previous calls have `generation < 5` and are invisible. A `ScanState` split-borrow view is created, caching `&mut [WordState]` and `&mut Vec<usize>` as individual stack references for register-friendly access.

### 2.2 Walk the Trie

`walk_and_scan` visits each trie node in flat-array order (parents before children), transforming text and scanning immediately.

---

**Node 0 — Root (None)**: no transformation.

```
text = "Hello! 你好世界 china is cool"
density = 0.36 (≤ 0.67 threshold) → bytewise engine selected
```

The bytewise AC automaton scans the full text. It finds no overlapping matches (our patterns are `"hello"` lowercase, `"你好"`, etc. — the raw text has `"Hello"` with capital H). No state updates.

---

**Node 1 — VariantNorm**: apply `VariantNormMatcher` to the root text.

`VariantNormMatcher::replace` scans for CJK variant codepoints via the page table. `你好世界` is already in normalized form → returns `None` (no change). The child node (Delete) receives the same text.

---

**Node 2 — Delete** (child of VariantNorm): apply `DeleteMatcher`.

`DeleteMatcher::delete` strips punctuation, symbols, and whitespace:

```
input:  "Hello! 你好世界 china is cool"
output: "Hello你好世界chinaisscool"    (density = 0.41 → bytewise)
```

This node terminates (`pt_index_mask` has bit 1 for `VariantNorm|Delete`). Scan with `pt_index_mask = 0b010`:

The bytewise AC finds `"你好"` at byte offset 5. The raw value has `DIRECT_RULE_BIT` set (R2 is a simple single-entry pattern). `process_match` extracts `pt_index=1` from the bit-packed value, checks `pt_index_mask & (1 << 1) != 0` → match. Sets `positive_generation = 5` for R2. `resolved_count` increments to 1.

---

**Node 3 — Romanize**: apply `RomanizeMatcher` to the root text.

`RomanizeMatcher::replace` converts CJK codepoints to romanized form (Chinese Pinyin, Japanese Romaji, Korean Revised Romanization):

```
input:  "Hello! 你好世界 china is cool"
output: "Hello!  ni  hao  shi  jie  china is cool"    (density = 0.0 → bytewise)
```

This node terminates (`pt_index_mask` has bit 2 for `Romanize`). Since `density = 0.0` (≤ 0.67 threshold), the bytewise engine is selected.

The bytewise AC finds no match for `"zhongguo"` (the text contains `"ni hao shi jie"`, not `"zhongguo"`). No state update for R3.

---

### 2.3 Evaluate (Pass 2)

`RuleSet::collect_matches` iterates `touched_indices` (rules that received at least one hit):

| Rule | positive_generation == 5? | not_generation != 5? | Result |
|------|--------------------------|---------------------|--------|
| R1 | No (only 0 of 2 AND segments matched) | — | Not satisfied |
| R2 | Yes | Yes (no NOT segments) | **Satisfied** → emit `SimpleResult { word_id: 2, word: "你好" }` |

R3 was never touched (no hit). Final output: `[SimpleResult { word_id: 2, word: "你好" }]`.

---

## 3. Fast Path: AllSimple

When every rule is a pure literal (no `&`/`~` operators) under a single `ProcessType` (typically `None`), `SearchMode::AllSimple` activates:

- **`is_match`** → delegates directly to `ScanPlan::is_match`, which uses a SIMD density scan to dispatch to bytewise (DFA or DAAC) or charwise. No TLS state, no generation counters, no trie walk.
- **`process`** → uses `process_simple`, which scans via `for_each_rule_idx_simple`. Each hit maps directly to a rule result via `DIRECT_RULE_BIT`. Deduplication uses only `positive_generation` — no `touched_indices` bookkeeping.

This path handles the common case of "check if any of these N keywords appear" with minimal overhead.

---

## Deep Dives

### Text Transformation Engines

#### ProcessType Bitflags

`ProcessType` is a `u8` bitflags type where each bit selects one transformation step:

| Flag | Bit | Effect | Data Source |
|------|-----|--------|-------------|
| `None` | 0 | No transformation; match raw input | — |
| `VariantNorm` | 1 | CJK variant normalization (Chinese T→S, Japanese Kyūjitai→Shinjitai, half-width katakana→full-width) | OpenCC `t2s` + `tw2s`/`hk2s` + JIS mappings |
| `Delete` | 2 | Remove punctuation/symbols/whitespace | `unicodedata.category()` |
| `Normalize` | 3 | NFKC casefold + numeric normalization | `unicodedata.normalize().casefold()` |
| `Romanize` | 4 | CJK → space-separated romanization (Chinese Pinyin, Japanese Romaji, Korean Revised Romanization) | `pypinyin` + kana/hangul tables |
| `RomanizeChar` | 5 | CJK → romanization (no inter-syllable spaces) | same as Romanize |
| `EmojiNorm` | 6 | Emoji → English words, strips modifiers (ZWJ, VS16, skin tones) | CLDR `annotations/en.xml` short names |

Flags compose with `|`. Named aliases: `DeleteNormalize`, `VariantNormDeleteNormalize`. Note: `EmojiNorm` does not compose usefully with `Delete` — Delete removes emoji before EmojiNorm can see them.

#### Page-Table Lookup

VariantNorm, Romanize, and Normalize share a two-stage page table (in `replace/mod.rs`):

```
page = l1[cp >> 8]                    // which 256-codepoint block?
if page == 0 → no mapping
value = l2[page * 256 + (cp & 0xFF)] // lookup within the block
if value == 0 → no mapping
```

- **VariantNorm**: L2 value is the normalized codepoint directly (`replace/variant_norm.rs`)
- **Romanize/Normalize**: L2 value packs `(offset << 8) | length` into a shared string buffer (`replace/romanize.rs`, `replace/normalize.rs`)

Both L1 and L2 are accessed via `get_unchecked` for branchless hot-path performance.

#### SIMD Skip Functions

The transform iterators use SIMD to skip irrelevant ASCII byte runs (in `transform/simd.rs`):

| Engine | Skip Function | What It Skips |
|--------|--------------|---------------|
| VariantNorm, Romanize | `skip_ascii_simd` | All ASCII bytes (only CJK keys exist) |
| Delete | `skip_ascii_non_delete_simd` | ASCII bytes not in the delete bitset |

Dispatch: AVX2 intrinsics on x86-64 (runtime detection via `OnceLock`), NEON on AArch64 (compile-time), portable `std::simd` fallback. Chunk sizes: 32 bytes (AVX2/portable), 16 bytes (NEON).

The delete-mask algorithm probes a 16-byte `ascii_lut` inside the SIMD loop using shuffle-based lookup: `byte_idx = byte >> 3`, `lut_byte = shuffle(ascii_lut, byte_idx)`, `bit_mask = shuffle(SHIFT_TABLE, byte & 7)`, `deleted = lut_byte & bit_mask`.

#### Fused Transform-Scan

For leaf Delete or Normalize nodes, `walk_and_scan` can bypass string materialization by streaming transformed bytes directly into the AC automaton via `daachorse`'s `find_overlapping_iter_from_iter`:

- **Delete**: `DeleteFilterIterator` yields only non-deleted bytes
- **Normalize**: `NormalizeFilterIterator` yields normalized bytes (unmapped pass through, mapped emit replacement bytes)
- **VariantNorm**: `VariantNormFilterIterator` yields normalized bytes (unmapped CJK pass through, mapped variant→normalized emit replacement char's UTF-8 bytes)

This eliminates the intermediate `String` allocation and the second text traversal.

The fused path uses a 3-way dispatch based on DFA availability and text density:

| Condition | Strategy | Rationale |
|---|---|---|
| `dfa` feature ON + density ≤ 0.67 | Materialize via `step.apply()` → DFA scan | DFA is 2–5× faster than DAAC bytewise streaming on ASCII-heavy text; allocation cost is negligible |
| `dfa` feature OFF + density ≤ 0.67 | Stream via `filter_bytes()` → DAAC bytewise | Best available option without DFA |
| density > 0.67 | Stream via `filter_bytes()` → DAAC charwise | Charwise does 1 transition per CJK char vs 3 bytewise; streaming avoids allocation |

---

### Density-Based Engine Dispatch

Engine selection uses non-ASCII byte density rather than a binary `is_ascii()` check. A SIMD scan (`simple_matcher/simd.rs`) counts non-ASCII bytes (≥ 0x80) across the full text in one pass (~2 µs for 200 KB). The density determines which engine is faster:

| Text density | Engine | Reason |
|---|---|---|
| ≤ 0.67 non-ASCII bytes (~≤40% CJK chars) | Bytewise (DFA or DAAC) | DFA is 2–5× faster than DAAC bytewise streaming on ASCII-heavy text |
| > 0.67 non-ASCII bytes (~>40% CJK chars) | Charwise DAAC | 1 transition per char vs 3 bytewise on CJK |

Both engines are built from the **full** pattern set (not split by ASCII/CJK), so either engine is correct for any text. The dispatch is a pure speed optimization.

The threshold (0.67) was calibrated from an 8,932-point characterization sweep across 12 pattern sizes × 11 pattern CJK compositions × 11 text CJK densities. The crossover is consistent regardless of pattern composition.

In `walk_and_scan`, density propagates through the transform tree via the `(String, f32)` tuple returned by `TransformStep::apply()`. The returned density is conservative (typically `parent_density`, or `0.0` for Romanize which converts CJK→ASCII). `density == 0.0` replaces the old `is_ascii` boolean for transform no-op detection.

#### No-op Scan Folding

When the parent text is pure ASCII (`density == 0.0`), transforms like VariantNorm, Romanize, RomanizeChar, and EmojiNorm produce identical text (they only operate on non-ASCII codepoints). Scanning the same text again with a different `pt_index_mask` wastes an entire DFA traversal.

`fold_noop_children_masks` recursively merges no-op children's `pt_index_mask` into the parent's scan mask. The parent scans once with the OR'd mask; no-op children are skipped entirely during the walk. This is correct because:

- Each `PatternEntry` has a fixed `pt_index` — hits pass exactly one mask branch.
- `mark_positive` and `satisfied_mask |= bit` are idempotent.
- Matrix path uses the same `text_index` (`parent_vi`) — same column, same counters.
- The AC engine reports each position exactly once per scan.

For a matcher with PTs {None, VariantNorm, Romanize, Delete} on ASCII text, this reduces 4 scans to 2 (root+VN+Romanize merged, Delete separate), yielding ~7-8% throughput improvement on the scan-dominated path.

---

### State Management

#### Per-Rule State

Rules are split into hot and cold structs for cache efficiency:

- **`RuleHot`** (accessed only for matrix-mode rules on first touch): `segment_counts: Vec<i32>`. The `and_count` field lives in `PatternEntry` for cache locality (avoids loading `RuleHot` on the hot path). `use_matrix` is derived from `RuleShape`.
- **`RuleCold`** (accessed only when producing output): `word_id: u32`, `word: String`.
- **`WordState`** (per-rule mutable state): three generation stamps (`matrix_generation`, `positive_generation`, `not_generation`), a `satisfied_mask: u64`, and `remaining_and: u16`.

#### Generation-Based Reuse

Instead of zeroing `WordState` arrays between calls, a monotonic `generation: u32` counter is bumped. A field is "live" only when its stamp matches the current generation. Cost: O(1) amortized reset. Wraps at `u32::MAX` (once per ~4 billion calls).

#### ScanState Split-Borrow

`ScanState<'a>` borrows `SimpleMatchState` fields as individual mutable slices rather than passing `&mut SimpleMatchState`. This enables register-cached base pointers (the compiler keeps `&mut [WordState]` data pointer in a register across the scan loop) and eliminates double word_state loads via disjoint-field borrowing. Profiled: −9.9% pointer-chase overhead, 3–6% throughput improvement.

#### PatternKind Dispatch

Each `PatternEntry` carries a pre-computed `PatternKind`, `RuleShape`, and `and_count`:

| Kind | Condition | Behavior |
|------|-----------|----------|
| `Simple` | 1 AND segment, no NOT, no matrix | First hit sets `positive_generation`. Done. |
| `And` | `offset < and_count` | Decrements counter or sets bitmask bit. |
| `Not` | `offset >= and_count` | Sets `not_generation` to veto the rule. |

`and_count` is duplicated from build-time rule metadata into `PatternEntry` so the init block in `process_entry` can initialize `WordState` without loading `RuleHot` (which is only needed for the cold matrix-init path). This fits in the existing struct padding (9→10 bytes, still padded to 12).

#### DIRECT_RULE_BIT

For single-entry simple patterns, the automaton value encodes `rule_idx | (1 << 31)` directly. The scan hot path checks bit 31 first — if set, extracts the rule index without touching the entry table. Eliminates two indirections for the common case.

#### Bitmask vs Matrix

- **Bitmask** (≤64 segments, no repeated counts): each AND hit sets bit `offset` in `satisfied_mask` and decrements `remaining_and`. Reaching 0 → satisfied. NOT hits set `not_generation` immediately.
- **Matrix** (>64 segments or repeated counts): a `TinyVec<[i32; 16]>` counter grid sized `[segments × variants]`. AND cells decrement; NOT cells increment. Threshold crossings tracked per-segment via `matrix_status`.

```
Rule parsed from pattern string
        │
        ▼
  shape == SingleAnd, no NOT? ──► Simple (no counters)
        │ NO
        ▼
  ≤64 segs, no repeats?  ──► Bitmask (u64 + remaining_and)
        │ NO
        ▼
                              Matrix (TinyVec counter grid)
```

---

### Memory Efficiency

#### Thread-Local Storage

| Slot | Type | Purpose |
|------|------|---------|
| `SIMPLE_MATCH_STATE` | `UnsafeCell<SimpleMatchState>` | Per-rule word states, counter matrices, touched-index list. Reused across calls. |
| `STRING_POOL` | `UnsafeCell<Vec<String>>` | Recycled `String` buffers (bounded at 128). |

Both use `#[thread_local]` + `UnsafeCell` for zero-overhead TLS access (eliminates `thread_local!` macro's `.with()` closure). Sound because single-threaded access is guaranteed and no function is re-entrant.

#### String Pool

`get_string_from_pool(capacity)` pops and clears a buffer (or allocates new). `return_string_to_pool(s)` pushes back, bounded at 128. Used throughout the transform pipeline and in `walk_and_scan` for arena management.

#### Static Step Cache

`TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8]` — one slot per `ProcessType` bit. Lazily initialized from build-time artifacts (`include_bytes!`/`include_str!`). Shared as `&'static` across all matchers and threads.

#### Global Allocator

`mimalloc` v3 replaces the system allocator for improved multi-threaded allocation throughput.

---

### Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `perf` | on | Meta-feature enabling `dfa` + `simd_runtime_dispatch` |
| `dfa` | via `perf` | `aho-corasick` DFA for bytewise engine. ~17× more memory, ~1.7–3.3× faster (Teddy prefilter). |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD kernel selection for transforms (AVX2/NEON/portable) and density counting |

---

### Compiled Transformation Tables

`build.rs` pre-compiles transformation data from source files in `matcher_rs/process_map/` (`VARIANT_NORM.txt`, `ROMANIZE.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) into binary artifacts embedded via `include_bytes!`/`include_str!` (in `transform/constants.rs`). At runtime, page tables are decoded lazily on first access by the step registry.
