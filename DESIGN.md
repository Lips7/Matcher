# Design

This document explains how `matcher_rs` works by walking through a concrete example end to end — from rule construction to query evaluation. Deep-dive sections at the end cover SIMD engines, state management, and memory efficiency.

---

## Running Example

Three rules, each using a different text transformation:

| Rule | ProcessType | word_id | Pattern |
|------|-------------|---------|---------|
| R1 | `None` | 1 | `"hello&world"` |
| R2 | `Fanjian \| Delete` | 2 | `"你好"` |
| R3 | `PinYin` | 3 | `"zhongguo"` |

Query text: `"Hello! 你好世界 china is cool"`

We will trace both construction and query evaluation using these rules.

---

## 1. Construction

`SimpleMatcher::new` (in `build.rs`) runs four stages.

### 1.1 Parse Rules

`parse_rules` processes each rule string:

**R1: `"hello&world"` under `ProcessType::None`**

Split on `&`/`~` → two AND segments: `["hello", "world"]`. No NOT segments. `and_count = 2`, `use_matrix = false` (both counts are 1, total ≤ 64), `has_not = false`.

Each sub-pattern is emitted via `reduce_text_process_emit(process_type - Delete, pattern)`. Since `None - Delete = None`, both `"hello"` and `"world"` emit themselves unchanged.

**R2: `"你好"` under `Fanjian | Delete`**

Single AND segment: `["你好"]`. `and_count = 1`, simple rule. Emitted under `Fanjian | Delete - Delete = Fanjian`. The Fanjian transform maps Traditional → Simplified; `"你好"` is already Simplified, so it emits unchanged as `"你好"`.

**R3: `"zhongguo"` under `PinYin`**

Single AND segment: `["zhongguo"]`. Simple rule. Emitted under `PinYin - Delete = PinYin`. Since `"zhongguo"` is pure ASCII and PinYin only transforms CJK, it emits unchanged.

**Why subtract Delete?** Input text is Delete-transformed before scanning, so patterns are stored verbatim and matched against already-deleted text. Double-deleting would break matches.

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

The three `ProcessType` values — `{None, Fanjian|Delete, PinYin}` — are decomposed into single-bit steps and merged into a shared-prefix trie:

```
[0] Root (None) ← terminates: pt_index_mask has bit 0 (None)
 ├─[1] Fanjian
 │  └─[2] Delete ← terminates: pt_index_mask has bit 1 (Fanjian|Delete)
 └─[3] PinYin   ← terminates: pt_index_mask has bit 2 (PinYin)
```

Each node caches a `&'static TransformStep` reference from the global step registry. The root's step is `None` (no transformation). `pt_index_mask` is a `u64` bitmask of which compact indices terminate at or pass through each node.

**Sequential index table** (`pt_index_table`): maps raw `ProcessType::bits()` → compact 0..N. `None` always gets index 0. This compact index lets `PatternEntry.pt_index` fit in a `u8`.

### 1.3 Compile Scan Engines

`ScanPlan::compile` receives the deduplicated patterns and builds:

**PatternIndex**: maps each pattern's dedup index to its `PatternEntry` slice. Also builds the value map — for simple single-entry patterns, the value is `rule_idx | DIRECT_RULE_BIT` (bit 31 set), encoding the rule index directly in the automaton hit value so the scan hot path skips the entry table lookup.

**Bytewise engine** (`BytewiseMatcher`): compiled from ASCII-only patterns (`"hello"`, `"world"`, `"zhongguo"`). With the `dfa` feature and ≤ 25,000 patterns, uses `aho-corasick` DFA for maximum throughput. Otherwise falls back to `daachorse` bytewise DAAC.

**Charwise engine** (`CharwiseMatcher`): compiled from **all** patterns (ASCII + CJK). Always built. CJK characters are 3 UTF-8 bytes, so charwise does 1 state transition vs 3 for bytewise — ~1.6–1.9× faster on non-ASCII text.

**Harry engine** (with `harry` feature): built when all patterns are ASCII and ≥ 64 exist. In our example with only 4 patterns, Harry is not built.

### 1.4 Assemble

```rust
SimpleMatcher {
    tree: Vec<ProcessTypeBitNode>,  // the 4-node trie above
    mode: SearchMode::General,      // R1 has &-operator → not AllSimple
    scan: ScanPlan { bytewise, charwise, harry, pattern_index },
    rules: RuleSet { hot: [RuleHot; 3], cold: [RuleCold; 3] },
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
is_ascii = false → charwise engine selected
```

The charwise AC automaton scans the full text. It finds no overlapping matches (our patterns are `"hello"` lowercase, `"你好"`, etc. — the raw text has `"Hello"` with capital H). No state updates.

---

**Node 1 — Fanjian**: apply `FanjianMatcher` to the root text.

`FanjianMatcher::replace` scans for Traditional Chinese codepoints via the page table. `你好世界` is already Simplified → returns `None` (no change). The child node (Delete) receives the same text.

---

**Node 2 — Delete** (child of Fanjian): apply `DeleteMatcher`.

`DeleteMatcher::delete` strips punctuation, symbols, and whitespace:

```
input:  "Hello! 你好世界 china is cool"
output: "Hello你好世界chinaisscool"    (is_ascii = false)
```

This node terminates (`pt_index_mask` has bit 1 for `Fanjian|Delete`). Scan with `pt_index_mask = 0b010`:

The charwise AC finds `"你好"` at byte offset 5. The raw value has `DIRECT_RULE_BIT` set (R2 is a simple single-entry pattern). `process_match` extracts `pt_index=1` from the bit-packed value, checks `pt_index_mask & (1 << 1) != 0` → match. Sets `positive_generation = 5` for R2. `resolved_count` increments to 1.

---

**Node 3 — PinYin**: apply `PinyinMatcher` to the root text.

`PinyinMatcher::replace` converts CJK codepoints to Pinyin romanization:

```
input:  "Hello! 你好世界 china is cool"
output: "Hello!  ni  hao  shi  jie  china is cool"    (is_ascii = true)
```

This node terminates (`pt_index_mask` has bit 2 for `PinYin`). Since `is_ascii = true`, the bytewise engine is selected.

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

- **`is_match`** → delegates directly to `ScanPlan::is_match`, which dispatches to Harry (≥64 patterns), AC DFA, or DAAC bytewise. No TLS state, no generation counters, no trie walk.
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
| `Fanjian` | 1 | Traditional → Simplified Chinese | OpenCC `t2s` + `tw2s`/`hk2s` |
| `Delete` | 2 | Remove punctuation/symbols/whitespace | `unicodedata.category()` |
| `Normalize` | 3 | NFKC casefold + numeric normalization | `unicodedata.normalize().casefold()` |
| `PinYin` | 4 | CJK → space-separated Pinyin | `pypinyin` no-tone |
| `PinYinChar` | 5 | CJK → Pinyin (no inter-syllable spaces) | `pypinyin` no-tone |

Flags compose with `|`. Named aliases: `DeleteNormalize`, `FanjianDeleteNormalize`.

#### Page-Table Lookup

Fanjian, Pinyin, and Normalize share a two-stage page table (in `replace/mod.rs`):

```
page = l1[cp >> 8]                    // which 256-codepoint block?
if page == 0 → no mapping
value = l2[page * 256 + (cp & 0xFF)] // lookup within the block
if value == 0 → no mapping
```

- **Fanjian**: L2 value is the Simplified codepoint directly (`replace/fanjian.rs`)
- **Pinyin/Normalize**: L2 value packs `(offset << 8) | length` into a shared string buffer (`replace/pinyin.rs`, `replace/normalize.rs`)

Both L1 and L2 are accessed via `get_unchecked` for branchless hot-path performance.

#### SIMD Skip Functions

The transform iterators use SIMD to skip irrelevant ASCII byte runs (in `transform/simd.rs`):

| Engine | Skip Function | What It Skips |
|--------|--------------|---------------|
| Fanjian, Pinyin | `skip_ascii_simd` | All ASCII bytes (only CJK keys exist) |
| Delete | `skip_ascii_non_delete_simd` | ASCII bytes not in the delete bitset |

Dispatch: AVX2 intrinsics on x86-64 (runtime detection via `OnceLock`), NEON on AArch64 (compile-time), portable `std::simd` fallback. Chunk sizes: 32 bytes (AVX2/portable), 16 bytes (NEON).

The delete-mask algorithm probes a 16-byte `ascii_lut` inside the SIMD loop using shuffle-based lookup: `byte_idx = byte >> 3`, `lut_byte = shuffle(ascii_lut, byte_idx)`, `bit_mask = shuffle(SHIFT_TABLE, byte & 7)`, `deleted = lut_byte & bit_mask`.

#### Fused Transform-Scan

For leaf Delete or Normalize nodes, `walk_and_scan` can bypass string materialization by streaming transformed bytes directly into the AC automaton via `daachorse`'s `find_overlapping_iter_from_iter`:

- **Delete**: `DeleteFilterIterator` yields only non-deleted bytes
- **Normalize**: `NormalizeFilterIterator` yields normalized bytes (unmapped pass through, mapped emit replacement bytes)

This eliminates the intermediate `String` allocation and the second text traversal. Not used when the `aho-corasick` DFA engine is selected (DFA has no streaming API, and DFA+materialization is faster anyway). Measured: +12.6% throughput on CJK delete+scan workloads.

---

### Harry Column-Vector SIMD Backend

`HarryMatcher` (in `simple_matcher/harry/`) is a column-vector SIMD scan engine for `is_match` when the pattern set is large (≥ 64 patterns) and `AllSimple`.

#### Dual-Index Encoding

Patterns are grouped into 8 buckets by `byte[0] & 0x07`. A single unified matcher covers prefix lengths 2–8. Two mask tables per column — `low_mask[byte & 0x3F]` (bits [0:5]) and `high_mask[(byte >> 1) & 0x3F]` (bits [1:6]) — are ORed per lane:

```
byte = 0x68 ('h') = 0b_0110_1000

low_mask  index:  byte & 0x3F        = 40
high_mask index:  (byte >> 1) & 0x3F = 52

   low_mask[col][40]  ──┐
                         OR ──► state (8 bits, one per bucket)
   high_mask[col][52] ──┘

After all columns:  hit_mask = !state
Bit k set ═► bucket k has a candidate match
```

Coverage: 7 of 8 bits per byte. ASCII patterns → zero false positives. Non-ASCII bytes may collide at bit 7 — caught by exact-match verification.

#### Column-0 Early Exit

After applying column 0, check if every lane's state byte is 0xFF. If true, skip the entire chunk. Fires ~95% of the time on CJK haystacks with ASCII patterns, yielding 3–6× speedup over AC.

#### Wildcarding

Columns beyond a pattern's actual prefix length are wildcarded (bucket bit cleared for all 64 rows). Patterns of different lengths coexist in the same unified tables.

#### Verification

Bucket hits pass through `BucketVerify`: a `length_mask: u8` indicates which prefix lengths have entries, and a `PrefixMap` per length stores sorted `keys: Box<[u64]>` for binary search. Matches dispatch to `PrefixGroup`: `exact_values` (prefix == full pattern) or `long_literals` (need suffix comparison).

#### SIMD Kernels

- **AArch64 NEON** (`harry/neon.rs`): 16-byte chunks, `M = 16 - max_prefix_len + 1` positions. Compile-time intrinsics.
- **x86-64 AVX512-VBMI** (`harry/avx512.rs`): 64-byte chunks, M=56 positions. Runtime detection.
- **Scalar** (`harry/mod.rs`): byte-at-a-time fallback.

Single-byte patterns bypass the column pipeline via a dedicated `single_byte_values: Box<[Vec<u32>; 128]>` lookup table with SIMD-accelerated scanning (≤ 4 distinct keys).

---

### State Management

#### Per-Rule State

Rules are split into hot and cold structs for cache efficiency:

- **`RuleHot`** (accessed on every pattern hit): `segment_counts: Vec<i32>`, `and_count`, `use_matrix`, `has_not`.
- **`RuleCold`** (accessed only when producing output): `word_id: u32`, `word: String`.
- **`WordState`** (per-rule mutable state): three generation stamps (`matrix_generation`, `positive_generation`, `not_generation`), a `satisfied_mask: u64`, and `remaining_and: u16`.

#### Generation-Based Reuse

Instead of zeroing `WordState` arrays between calls, a monotonic `generation: u32` counter is bumped. A field is "live" only when its stamp matches the current generation. Cost: O(1) amortized reset. Wraps at `u32::MAX` (once per ~4 billion calls).

#### ScanState Split-Borrow

`ScanState<'a>` borrows `SimpleMatchState` fields as individual mutable slices rather than passing `&mut SimpleMatchState`. This enables register-cached base pointers (the compiler keeps `&mut [WordState]` data pointer in a register across the scan loop) and eliminates double word_state loads via disjoint-field borrowing. Profiled: −9.9% pointer-chase overhead, 3–6% throughput improvement.

#### PatternKind Dispatch

Each `PatternEntry` carries a pre-computed `PatternKind`:

| Kind | Condition | Behavior |
|------|-----------|----------|
| `Simple` | 1 AND segment, no NOT, no matrix | First hit sets `positive_generation`. Done. |
| `And` | `offset < and_count` | Decrements counter or sets bitmask bit. |
| `Not` | `offset >= and_count` | Sets `not_generation` to veto the rule. |

#### DIRECT_RULE_BIT

For single-entry simple patterns, the automaton value encodes `rule_idx | (1 << 31)` directly. The scan hot path checks bit 31 first — if set, extracts the rule index without touching the entry table. Eliminates two indirections for the common case.

#### Bitmask vs Matrix

- **Bitmask** (≤64 segments, no repeated counts): each AND hit sets bit `offset` in `satisfied_mask` and decrements `remaining_and`. Reaching 0 → satisfied. NOT hits set `not_generation` immediately.
- **Matrix** (>64 segments or repeated counts): a `TinyVec<[i32; 16]>` counter grid sized `[segments × variants]`. AND cells decrement; NOT cells increment. Threshold crossings tracked per-segment via `matrix_status`.

```
Rule parsed from pattern string
        │
        ▼
  and_count == 1, no NOT? ──► Simple (no counters)
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
| `perf` | on | Meta-feature enabling `dfa` + `simd_runtime_dispatch` + `harry` |
| `dfa` | via `perf` | `aho-corasick` DFA for bytewise engine (≤25,000 ASCII patterns). ~17× more memory, ~1.7–1.9× faster. |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD kernel selection for transforms (AVX2/NEON) and Harry (AVX512-VBMI/NEON) |
| `harry` | via `perf` | Column-vector SIMD scan backend for `is_match` when ≥64 patterns, `AllSimple` mode |

---

### Compiled Transformation Tables

`build.rs` pre-compiles transformation data from source files in `matcher_rs/process_map/` (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) into binary artifacts embedded via `include_bytes!`/`include_str!` (in `transform/constants.rs`). At runtime, page tables are decoded lazily on first access by the step registry.
