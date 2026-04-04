# Design

This document describes the internal architecture of `matcher_rs` as it exists in the codebase today. It is intended for contributors and anyone integrating the library at a low level. Where the code has a non-obvious reason for doing something a particular way, this document explains the reasoning.

## Table of Contents

- [Text Transformation Pipeline](#text-transformation-pipeline)
  - [ProcessType Bitflags](#processtype-bitflags)
  - [Transformation Backends](#transformation-backends)
  - [TransformStep and StepOutput](#transformstep-and-stepoutput)
  - [Step Registry](#step-registry)
  - [Transformation DAG (ProcessTypeBitNode Tree)](#transformation-dag-processtypebitnode-tree)
  - [Trie Traversal](#trie-traversal)
- [SimpleMatcher](#simplematcher)
  - [Input Format](#input-format)
  - [Pattern Syntax](#pattern-syntax)
  - [Construction](#construction)
  - [Three-Component Architecture](#three-component-architecture)
  - [SearchMode](#searchmode)
  - [Two-Pass Matching](#two-pass-matching)
  - [Scan Engine Selection](#scan-engine-selection)
  - [Harry Column-Vector SIMD Backend](#harry-column-vector-simd-backend)
  - [Pass 1: Pattern Scanning](#pass-1-pattern-scanning)
  - [Pass 2: Logical Evaluation](#pass-2-logical-evaluation)
- [State Management](#state-management)
  - [Per-Rule State: RuleHot, RuleCold, WordState](#per-rule-state-rulehot-rulecold-wordstate)
  - [Generation-Based State Reuse](#generation-based-state-reuse)
  - [Sparse Set: touched_indices](#sparse-set-touched_indices)
  - [PatternKind Dispatch](#patternkind-dispatch)
  - [PatternIndex and PatternDispatch](#patternindex-and-patterndispatch)
  - [DIRECT_RULE_BIT Fast Path](#direct_rule_bit-fast-path)
  - [Bitmask Fast Path](#bitmask-fast-path)
  - [Matrix Fallback](#matrix-fallback)
  - [AllSimple Fast Path](#allsimple-fast-path)
- [Memory and Resource Efficiency](#memory-and-resource-efficiency)
  - [Thread-Local Storage](#thread-local-storage)
  - [String Pool](#string-pool)
  - [Static Transform Step Cache](#static-transform-step-cache)
  - [Global Allocator](#global-allocator)
- [Feature Flags](#feature-flags)
- [Compiled vs. Runtime Transformation Tables](#compiled-vs-runtime-transformation-tables)

---

## Architecture Overview

```
                        ┌─────────────────────────────────────────────────┐
  CONSTRUCTION          │  SimpleMatcher::new(rules)                      │
  (one-time)            │                                                 │
                        │  1. parse_rules ──► ParsedRules                 │
                        │     split &/~ operators, dedup patterns          │
                        │                                                 │
                        │  2. build_process_type_tree ──► ProcessPlan     │
                        │     shared-prefix trie + SearchMode             │
                        │                                                 │
                        │  3. ScanPlan::compile ──► ScanPlan              │
                        │     AC automata (bytewise/charwise/Harry)       │
                        │     + PatternIndex (value → rule mapping)       │
                        │                                                 │
                        │  4. Assemble RuleSet (hot/cold rule metadata)   │
                        └─────────────────────────────────────────────────┘
                                            │
                     ┌──────────────────────┼──────────────────────┐
                     ▼                      ▼                      ▼
              ┌─────────────┐      ┌──────────────┐      ┌──────────────┐
              │ ProcessPlan │      │   ScanPlan   │      │   RuleSet    │
              │             │      │              │      │              │
              │ • trie nodes│      │ • bytewise AC│      │ • RuleHot[]  │
              │ • SearchMode│      │ • charwise AC│      │ • RuleCold[] │
              │             │      │ • Harry      │      │              │
              │             │      │ • PatternIdx │      │              │
              └──────┬──────┘      └──────┬───────┘      └──────┬───────┘
                     │                    │                      │
  QUERY              └──────────┬─────────┘──────────────────────┘
  (per call)                    ▼
                ┌───────────────────────────────────┐
                │ AllSimple?                         │
                │   YES → ScanPlan::is_match (fast)  │
                │   NO  → walk_and_scan:             │
                │         walk trie, transform text, │
                │         scan each variant (Pass 1),│
                │         evaluate rules (Pass 2)    │
                └───────────────────────────────────┘
```

---

## Text Transformation Pipeline

### ProcessType Bitflags

`ProcessType` is a `u8` bitflags type (via the `bitflags` crate) where each bit selects one transformation step. Flags compose freely with `|`:

| Flag | Bit | Description |
|------|-----|-------------|
| `None` | `0b00000001` | No transformation; match against the raw input. |
| `Fanjian` | `0b00000010` | Traditional Chinese to Simplified Chinese conversion. |
| `Delete` | `0b00000100` | Remove codepoints listed in the configured delete tables. |
| `Normalize` | `0b00001000` | Multi-character replacement via normalization tables (full-width forms, digit-like variants, etc.). |
| `PinYin` | `0b00010000` | Chinese characters to space-separated Pinyin syllables. |
| `PinYinChar` | `0b00100000` | Chinese characters to Pinyin with inter-syllable spaces stripped. |

Named aliases exist for common combinations: `DeleteNormalize` (0b00001100) and `FanjianDeleteNormalize` (0b00001110). These are the same as composing the individual flags with `|`.

The default value is `ProcessType::empty()` (no bits set), which differs from `ProcessType::None` (the explicit "raw text" flag at bit 0). `ProcessType::iter()` yields individual single-bit flags in ascending bit order.

Source data for each transformation:

| Map | Source Rule | Used By |
|-----|-------------|---------|
| `FANJIAN` | OpenCC `t2s` base plus `tw2s` / `hk2s` single-codepoint additions | `Fanjian` |
| `TEXT-DELETE` | `unicodedata.category()` over punctuation, symbol, mark, separator, control, and format categories used by the matcher | `Delete` |
| `NORM` | `unicodedata.normalize("NFKC", ch).casefold()` | `Normalize` |
| `NUM-NORM` | `unicodedata.numeric()` rendered to ASCII | `Normalize` |
| `PINYIN` / `PINYIN-CHAR` | `pypinyin` no-tone single-codepoint romanization | `PinYin`, `PinYinChar` |

### Transformation Backends

Each single-bit `ProcessType` maps to a low-level engine in `process/transform/`. The engine owns the compiled data structures for one class of transformation:

| ProcessType | Engine | Module | Data Structure | Complexity |
|---|---|---|---|---|
| `Fanjian` | `FanjianMatcher` | `replace.rs` | 2-stage page table. L1: `Box<[u16]>` (one per 256-codepoint block). L2: dense `Box<[u32]>` pages. A zero L1 entry means the entire block has no mapping. | O(1) per codepoint |
| `PinYin` / `PinYinChar` | `PinyinMatcher` | `replace.rs` | Same 2-stage page table, but L2 values pack `(offset << 8 \| length)` into a concatenated UTF-8 string buffer (`Cow<'static, str>`). `PinYinChar` trims leading/trailing spaces from each packed entry at construction time via `trim_pinyin_packed`. The current generated table has no ASCII keys, so ASCII input is a guaranteed no-op. | O(1) per codepoint |
| `Delete` | `DeleteMatcher` | `delete.rs` | ~139 KB flat BitSet covering U+0000 to U+10FFFF (`Cow<'static, [u8]>`). A 16-byte `ascii_lut` copy of the first 128 bits is kept inline for cache-hot ASCII checks. Uses a two-phase delete scan (seek + copy-skip) with SIMD bulk-skip of non-deletable ASCII. | O(1) per codepoint, branchless |
| `Normalize` | `NormalizeMatcher` | `replace.rs` | `AhoCorasick` DFA (leftmost-longest, via `aho-corasick` crate). Paired with a `replace_list: Vec<&'static str>` so pattern index `i` maps directly to its replacement. A `NormalizeFindAdapter` wraps `aho_corasick::FindIter` for use with `SliceReplacingByteIter`. | O(N) per text |
| `None` | `TransformStep::None` | `step.rs` | No-op step that preserves the input variant. | - |

The page-table lookup for Fanjian and Pinyin (shared `page_table_lookup` function in `replace.rs`):
```
page = l1[cp >> 8]       // which 256-codepoint block?
if page == 0 → no mapping
value = l2[page * 256 + (cp & 0xFF)]
if value == 0 → no mapping
```

L1 and L2 are accessed via `get_unchecked` with a bounds check on L1 and a `debug_assert!` on L2.

#### SIMD-Accelerated Skip Functions

The replacement iterators (in `replace.rs`) and delete scan (in `delete.rs`) use SIMD to skip over bytes that cannot produce a match, avoiding per-byte branching. All skip functions live in `transform/simd.rs`:

| Caller | Skip Function | What It Skips |
|--------|--------------|---------------|
| `FanjianFindIter` | `skip_ascii_simd` | All ASCII bytes (Fanjian only maps non-ASCII CJK codepoints) |
| `DeleteMatcher::delete` | `skip_ascii_non_delete_simd` | ASCII bytes that are NOT in the delete bitset (probes the 16-byte `ascii_lut` via SIMD table lookup) |
| `PinyinFindIter` | `skip_ascii_simd` | All ASCII bytes (the current generated Pinyin table has no ASCII keys) |

Each skip function dispatches to the best available kernel:

- **x86-64 with `simd_runtime_dispatch`**: Runtime detection via `is_x86_feature_detected!("avx2")`. A `SimdDispatch` struct holds function pointers (`SkipFn` / `SkipDeleteFn`) initialized once (via `OnceLock`), so subsequent calls are a single indirect call. AVX2 paths use 32-byte chunks with `_mm256_movemask_epi8` for non-ASCII detection and `_mm256_shuffle_epi8` for bitset probing. Falls back to portable `std::simd`.
- **aarch64 with `simd_runtime_dispatch`**: Compile-time NEON intrinsics. 16-byte chunks using `vmaxvq_u8` for fast any-non-ASCII tests and `vqtbl1q_u8` for bitset probing. Exact lane position is found by storing the chunk to a scratch buffer and scanning scalarly.
- **Portable fallback**: `std::simd` with 32-lane (or 16-lane for tail in delete) operations. Uses `swizzle_dyn` for the bitset lookup, which the compiler lowers to the appropriate target instructions.

All paths fall through to a scalar byte-at-a-time loop for the tail.

##### Delete-mask algorithm

The "non-delete" skip functions probe a 128-bit ASCII bitset (`ascii_lut`, 16 bytes) inside the SIMD loop using a shuffle-based lookup:

1. `byte_idx = byte >> 3` — selects which of the 16 LUT bytes to read.
2. `lut_byte = shuffle(ascii_lut, byte_idx)` — SIMD table lookup.
3. `bit_pos = byte & 7` — selects the bit within the LUT byte.
4. `bit_mask = shuffle(SHIFT_TABLE, bit_pos)` — converts bit position to a single-bit mask (1, 2, 4, ..., 128) via a precomputed `SHIFT_TABLE_16`/`SHIFT_TABLE_32`.
5. `deleted = lut_byte & bit_mask` — non-zero means the byte is deletable.

This is combined (OR) with the non-ASCII mask to produce a stop mask; the first set bit (via `trailing_zeros`) gives the exact stop offset.

#### UTF-8 Decoding

`transform/utf8.rs` provides a shared `decode_utf8_raw` function that decodes one non-ASCII UTF-8 codepoint from `bytes[offset..]` using `get_unchecked` reads. It handles 2, 3, and 4-byte sequences by branching on the lead byte's high bits. Both `replace.rs` and `delete.rs` import it as `pub(crate)`.

### TransformStep and StepOutput

`TransformStep` (in `step.rs`) wraps one of the low-level engines and provides a uniform `apply(&self, text: &str, parent_is_ascii: bool) -> StepOutput` interface. The six variants are: `None`, `Fanjian(FanjianMatcher)`, `Delete(DeleteMatcher)`, `Normalize(NormalizeMatcher)`, `PinYin(PinyinMatcher)`, `PinYinChar(PinyinMatcher)`.

`parent_is_ascii` indicates whether the incoming text is pure ASCII.

`StepOutput` carries two fields:
- `changed: Option<String>` — `None` when the step is a no-op (the text was unmodified). `Some(result)` when the text was transformed.
- `is_ascii: bool` — always describes whether the *post-step* text is pure ASCII, regardless of whether the text changed. Callers use this to select the bytewise or charwise AC engine for the next scan.

The `is_ascii` policy per step:
- **Fanjian** — CJK-to-CJK substitution; output is never ASCII (`is_ascii = false`). ASCII input is a guaranteed no-op.
- **Delete** — ASCII input stays ASCII (`is_ascii = true`); otherwise `is_ascii` is determined by `result.is_ascii()`.
- **Normalize** — ASCII input stays ASCII (`is_ascii = true`); otherwise `is_ascii` is determined by `result.is_ascii()`.
- **PinYin / PinYinChar** — always produces pure ASCII romanization for non-ASCII input (`is_ascii` from `result.is_ascii()`); ASCII input is a no-op.
- **None** — propagates `parent_is_ascii` unchanged.

ASCII fast-path: when `parent_is_ascii` is `true`, steps that can't modify ASCII (`Fanjian`, `PinYin`, `PinYinChar`) return `StepOutput::unchanged(true)` immediately. Steps that may modify ASCII (`Delete`, `Normalize`) still run but force `is_ascii = true` in the result, since ASCII-in → ASCII-out is guaranteed for all transforms (proven by process map analysis).

### Step Registry

`step.rs` holds both `TransformStep` / `StepOutput` and the lazy registry `TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8]` — one slot per bit position in the `u8` bitflags. `get_transform_step(process_type_bit)` uses `trailing_zeros()` as the array index, and initializes the slot on first access via `build_transform_step`.

Two build paths are feature-gated:
- **Default (not `runtime_build`)**: Deserializes or builds from build-time artifacts (`include_bytes!`/`include_str!` constants in `transform/constants.rs`). Page tables are decoded via `decode_u16_table`/`decode_u32_table`. The Normalize step always compiles an `aho-corasick` DFA from pattern strings.
- **`runtime_build`**: Parses the raw source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) from `process_map/` at process startup.

All `SimpleMatcher` instances share the same compiled steps, so the heavy initialization cost is paid at most once per step per process.

### Transformation DAG (ProcessTypeBitNode Tree)

When a `SimpleMatcher` is configured with multiple composite `ProcessType` values, their decomposed single-bit steps often share prefixes. For example, `Fanjian | Delete` and `Fanjian | Normalize` both start with a Fanjian step. Naively applying every composite pipeline independently would re-derive the Fanjian result twice.

`build_process_type_tree` (in `graph.rs`) constructs a flat-array trie that makes shared prefixes explicit:

```
Root (None)
├── Fanjian
│   ├── Delete            ← Fanjian | Delete terminates here
│   │   └── Normalize     ← Fanjian | Delete | Normalize terminates here
│   └── Normalize         ← Fanjian | Normalize terminates here
├── Delete
│   └── Normalize         ← Delete | Normalize terminates here
└── Normalize             ← Normalize terminates here
```

Each node (`ProcessTypeBitNode`) stores:
- `process_type_bit` — the single-bit transformation step this edge represents.
- `process_type_list: TinyVec<[ProcessType; 4]>` — which composite `ProcessType` values terminate at this node.
- `children: TinyVec<[usize; 4]>` — flat-array indices of the next transformation steps reachable from here.
- `step: Option<&'static TransformStep>` — a cached reference to the compiled step for this node (fetched from the registry at construction time). The root stores `None`.
- `pt_index_mask: u64` — pre-computed OR of `1u64 << pt.bits()` for every composite type in `process_type_list`. Used to tag output text variants so the scan phase can filter hits by process type without re-deriving the mask.

The "sequential index" (`pt_index`) deserves explanation. Raw `ProcessType::bits()` values can use bits up to position 5, and composite types produce values up to 0b00111111 = 63. Storing a full `u64` mask per `PatternEntry` would waste space. Instead, `build_pt_index_table` assigns each composite type used in the current matcher a sequential index 0, 1, 2, ... (with `ProcessType::None` always at 0). These compact indices let `PatternEntry.pt_index` fit in a `u8` while `pt_index_mask` stays a `u64` with small bit positions.

After tree construction, `recompute_mask_with_index` rewrites every node's `pt_index_mask` from raw-bit encoding to sequential-index encoding so it matches the `pt_index` stored in `PatternEntry`.

### Trie Traversal

`SimpleMatcher` uses `walk_and_scan` (in `search.rs`) to walk the trie and scan each variant immediately after production. The flat-array invariant guarantees every parent node has a lower index than its children, so a single forward pass visits parents before children.

- **Leaf + no-op** (parent is ASCII and step is guaranteed no-op on ASCII): reuses the parent's text and variant index, scanning with the child's `pt_index_mask` instead of materializing new text.
- **Leaf + real transform**: materializes via `TransformStep::apply` into a pooled `String`, scans the result, and returns the string to pool. The materialization path benefits from SIMD-optimized bulk processing in the transform engines.
- **Non-leaf**: materializes via `TransformStep::apply` into a `Vec<Cow<str>>` arena. `is_ascii` from `StepOutput` is stored per arena slot for downstream engine selection. Scans immediately if the node terminates.

#### Walk Allocation Strategy

The walk uses `TinyVec<[T; 16]>` for bookkeeping arrays (`ascii_flags`, `node_arena`, `node_variant`), keeping them stack-allocated for trees with ≤16 nodes (the practical maximum). The `texts` arena remains a `Vec<Cow<str>>` (lifetime-tied to input), but its capacity is cached in `SimpleMatchState::walk_arena_capacity` so that subsequent calls skip allocator probing. Combined with the string pool, the walk is effectively zero-allocation after the first call on a given thread.

#### Variant-Level Early Termination

In `process` mode (`exit_early=false`), the walk tracks a `resolved_count` in `SimpleMatchState`. Each time a rule first reaches `positive_generation == generation` (via `mark_positive`, bitmask, matrix, or single-and paths), the counter increments. After each variant scan, if `resolved_count >= rules.len()` and no rules have NOT segments (`!has_not_rules`), the walk breaks early — skipping remaining tree variants. This is safe because without NOT segments, a positively-satisfied rule cannot be vetoed by a later variant. For matchers where most rules are simple literals under `ProcessType::None`, this can skip the majority of variant scans after the root.

The standalone public APIs (`text_process`, `reduce_text_process`) in `api.rs` iterate `ProcessType` bits directly without the trie.

The thread-local string pool (`STRING_POOL`) recycles `String` allocations across calls.

---

## SimpleMatcher

### Input Format

Rules are provided as a nested map: `HashMap<ProcessType, HashMap<u32, &str>>` (aliased as `SimpleTable`). The outer key selects the normalization pipeline. The inner key is a `word_id` (caller-assigned, returned in match results). The inner value is the pattern string.

`SimpleTableSerde` is the same structure with `Cow<'a, str>` values for deserialization from owned strings.

`SimpleMatcherBuilder` provides a fluent API wrapping this map.

### Pattern Syntax

Each pattern string supports two operators:

| Operator | Meaning |
|----------|---------|
| `&` | All adjacent sub-patterns must appear (order-independent AND) |
| `~` | The following sub-pattern must be **absent** (NOT) |

```
"apple&pie"      → fires when both "apple" and "pie" appear
"banana~peel"    → fires when "banana" appears but "peel" does not
"a&b~c"          → fires when "a" and "b" appear and "c" does not
"a&a~b~b"        → fires when "a" appears at least twice and "b" appears fewer than twice
```

The `&`/`~` operators split the string into sub-patterns, each independently matched as a substring. The operators themselves are not part of the sub-pattern text. Empty sub-patterns (e.g., from leading `&`) are silently dropped.

Sub-patterns are counted: `"a&a"` requires two occurrences of `"a"`. This is tracked via `segment_counts` — the AND counter starts at the required count and decrements per hit. Similarly, `"a~b~b"` allows one occurrence of `"b"` but not two.

### Construction

`SimpleMatcher::new` (in `build.rs`) follows these steps:

1. **Build sequential ProcessType index table.** `build_pt_index_table` assigns compact 0..N indices (as `u8`) to each distinct composite `ProcessType` present in the input. `ProcessType::None` always gets index 0. The table is a `[u8; 64]` array indexed by `ProcessType::bits()`, with unused entries set to `u8::MAX`.

2. **Parse rules.** `parse_rules` iterates the rule map. For each pattern:
   - Splits on `&` and `~` into AND and NOT sub-patterns via `match_indices(['&', '~'])`, tracking the current operator state.
   - De-duplicates sub-patterns within the rule using `HashMap<&str, i32>` for both AND and NOT splits. AND patterns accumulate a positive count; NOT patterns accumulate a negative count. The resulting `segment_counts` vector has AND segments at `[0..and_count)` (initial values like `+1`, `+2` for repeated patterns) and NOT segments at `[and_count..)` (initial values like `0`, `-1`).
   - Determines `use_matrix` (when `and_count > 64`, total segments > 64, any AND segment has count ≠ 1, or any NOT segment has count ≠ 0) and `has_not` (when `and_count != segment_counts.len()`).
   - Generates all normalized text variants for each sub-pattern via `reduce_text_process_emit`, which applies `process_type - ProcessType::Delete` to the sub-pattern. The subtraction of `Delete` is deliberate: input text is Delete-transformed before scanning, so the sub-patterns must *not* be Delete-transformed themselves or they would be double-processed. They are stored verbatim and matched against the already-deleted text.
   - De-duplicates emitted pattern strings across all rules into a flat `dedup_patterns` list using a `HashMap<Cow<'_, str>, usize>`. Each unique pattern is assigned a dedup index. A `PatternEntry` links each dedup index back to its `(rule_idx, offset, pt_index, kind)`.
   - Returns a `ParsedRules` intermediate representation containing `dedup_patterns`, `dedup_entries`, and the compiled `RuleSet`.

3. **Build transformation tree and recompute masks.** `build_process_type_tree` produces the trie from the `HashSet` of process types, then `recompute_mask_with_index` re-encodes every node's `pt_index_mask` to use the sequential indices matching `PatternEntry.pt_index`.

4. **Choose search mode.** After scan plan compilation, checks if the tree has no children and every pattern is `PatternKind::Simple` — if so, sets `AllSimple`; otherwise `General`.

5. **Compile scan engines.** `ScanPlan::compile` (in `engine.rs`) receives the deduplicated patterns and entries:
   - Builds a `PatternIndex` from the entry buckets (flattens into contiguous storage with parallel `ranges`).
   - Builds a value map via `PatternIndex::build_value_map`, which assigns each deduplicated pattern a `u32` scan value. Single-entry simple patterns get `rule_idx | DIRECT_RULE_BIT`.
   - Delegates to `compile_automata` which builds AC engines:
     - **Bytewise engine** (`BytewiseMatcher`): With the `dfa` feature, all patterns ASCII, and count ≤ `AC_DFA_PATTERN_THRESHOLD` (7,000), uses `aho-corasick` DFA (`AcDfa` variant with a `to_value` remapping `Vec<u32>`). Otherwise uses `daachorse` bytewise DAAC with user-supplied `u32` values.
     - **Charwise engine** (`CharwiseMatcher`): `daachorse` charwise DAAC compiled over the entire pattern set. Only built when non-ASCII patterns exist. When both ASCII and non-ASCII patterns are present, the charwise engine contains the **full** pattern set so a single charwise pass covers everything on non-ASCII input.
   - **Harry engine** (with `harry` feature): After AC compilation, if `charwise_matcher` is `None` (all patterns are pure ASCII), builds a `HarryMatcher` from the full pattern set. Only succeeds when ≥ 64 patterns exist and at least one pattern has length ≥ 2.
   - AC engines are `None` when the corresponding pattern class is absent.

6. **Assemble matcher.** The final `SimpleMatcher` stores three immutable components: `ProcessPlan` (tree + `SearchMode`), `ScanPlan` (automata + `PatternIndex`), and `RuleSet` (hot + cold rule metadata).

### Three-Component Architecture

`SimpleMatcher` stores three named components:

```rust
struct SimpleMatcher {
    process: ProcessPlan,  // transform tree + SearchMode
    scan: ScanPlan,        // AC automata + PatternIndex
    rules: RuleSet,        // hot/cold rule metadata
}
```

**`ProcessPlan`** bundles the precomputed transformation trie (`Vec<ProcessTypeBitNode>`) and the `SearchMode` selected at construction time. It provides `tree()`, `mode()`, and `is_all_simple()` accessors.

**`ScanPlan`** bundles the optional bytewise, charwise, and Harry engines with the `PatternIndex` that maps raw automaton values back to rule metadata. It provides `is_match(text)`, `for_each_match_value(text, is_ascii, callback)`, `for_each_match_value_from_iter(iter, is_ascii, callback)`, and `patterns()`.

**`RuleSet`** stores parallel `Vec<RuleHot>` and `Vec<RuleCold>` indexed by rule id. It provides `process_entry`, `has_match`, `collect_matches`, and `push_result_if_new`.

### SearchMode

`SearchMode` is an enum determined at construction time:

| Variant | Condition | Behavior |
|---------|-----------|----------|
| `AllSimple` | Tree has no children (only root) AND every `PatternEntry` has `kind == PatternKind::Simple` | Bypasses state tracking entirely. `is_match` delegates directly to `ScanPlan::is_match`. `process` uses `process_simple` which emits results immediately per hit with generation-based dedup. |
| `General` | Any non-simple pattern or any text transformation | Full state machine with process-type mask checks on every hit. |

### Two-Pass Matching

```
┌────────────────────────────────────────────────────────────────┐
│ Input text                                                     │
│   ↓                                                            │
│ walk_and_scan: walk trie, scan each variant immediately         │
│   ↓                                                            │
│ ┌─── Pass 1: Pattern Scanning ───────────────────────────────┐ │
│ │ For each text variant:                                     │ │
│ │   Select engine via is_ascii flag (bytewise / charwise)    │ │
│ │   For each overlapping hit:                                │ │
│ │     Dispatch raw value via PatternIndex::dispatch          │ │
│ │     → DirectRule: mark_positive immediately                │ │
│ │     → SingleEntry / Entries: RuleSet::process_entry        │ │
│ │     (Early exit if exit_early && rule fully satisfied)     │ │
│ └────────────────────────────────────────────────────────────┘ │
│   ↓                                                            │
│ ┌─── Pass 2: Logical Evaluation ─────────────────────────────┐ │
│ │ For each rule_idx in touched_indices:                      │ │
│ │   Check positive_generation == generation (all ANDs met)   │ │
│ │   Check not_generation != generation (no NOT vetoed)       │ │
│ │   If both: emit SimpleResult { word_id, word }             │ │
│ └────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

### Scan Engine Selection

Engine selection at query time is driven by a simple `is_ascii: bool` flag computed once per text variant via `text.is_ascii()`.

`ScanPlan::for_each_match_value` and `ScanPlan::for_each_match_value_from_iter` accept `is_ascii: bool`. When `true` or when no charwise engine exists, the bytewise engine is used; otherwise the charwise engine handles the scan. If either engine is `None` (no patterns in that class), the call is a no-op.

`ScanPlan::is_match` uses a three-tier dispatch:

1. **Harry** (with `harry` feature) — used when the Harry matcher is present (pure-ASCII pattern set only) and either:
   - No DFA engine exists (pattern count > `AC_DFA_PATTERN_THRESHOLD`), **or**
   - The text contains non-ASCII bytes (`!text.is_ascii()`).

   On non-ASCII haystacks, Harry's column-0 early exit filters ~95% of chunks, giving 3–4× throughput over AC. On ASCII haystacks the DFA's zero-false-positive verification wins at N ≤ 7,000; above that its state table exceeds L2 and Harry wins.

2. **AC bytewise** — used when the text is ASCII or no charwise engine exists. This covers the DFA fast path for small ASCII pattern sets on ASCII text.

3. **AC charwise** — used when the text contains non-ASCII characters and a charwise engine was compiled (mixed ASCII + non-ASCII patterns).

```
  ScanPlan::is_match(text)
          │
          ▼
  ┌─ Harry present? ──┐
  │  YES               │ NO
  ▼                    │
  ┌─ No DFA OR ──┐     │
  │  !is_ascii?   │     │
  │ YES      NO   │     │
  ▼          │    │     │
 Harry       │    │     │
             ▼    ▼     ▼
        ┌─ is_ascii OR no charwise? ──┐
        │  YES                         │ NO
        ▼                              ▼
    AC bytewise                   AC charwise
    (DFA or DAAC)                 (DAAC, full
                                   pattern set)
```

### Harry Column-Vector SIMD Backend

`HarryMatcher` (in `simple_matcher/harry/`) is a column-vector SIMD scan engine implementing the Harry paper with a dual-index encoding. It serves as a fast path for `is_match` when the pattern set is large (≥ 64 patterns) and purely ASCII.

#### Architecture

Patterns are grouped into 8 buckets by `byte[0] & 0x07`. A **single unified matcher** covers all prefix lengths in the range 2..=8 (`MAX_SCAN_LEN`). Two mask tables per column — `low_mask` indexed by `byte & 0x3F` (bits [0:5]) and `high_mask` indexed by `(byte >> 1) & 0x3F` (bits [1:6]) — are ORed per lane. A hit fires only when BOTH tables have the bucket bit cleared, giving 7-bit coverage per byte. For ASCII patterns this dual-index scheme is zero-false-positive; for non-ASCII bytes, bit 7 is lost, creating false positives between bytes X and X^0x80, all caught by exact-match verification.

```
  Dual-Index Encoding — per column, per haystack byte

  Example: byte = 0x68 ('h') = 0b_0110_1000

  low_mask  index:  byte & 0x3F        = 0b_10_1000 = 40
  high_mask index:  (byte >> 1) & 0x3F = 0b_11_0100 = 52

     low_mask[col][40]  ──┐
                          OR ──► state[col]    (8 bits, one per bucket)
     high_mask[col][52] ──┘

  After all columns:  hit_mask = !state
  Bit k set in hit_mask ═► bucket k has a candidate match

  Coverage: low covers bits [0:5], high covers bits [1:6]
            Together they see 7 of 8 bits (bit 7 is lost)
            ASCII bytes use only bits [0:6] → zero false positives
```

#### Column-0 Early Exit

After applying column 0, the SIMD kernels check if every lane's state byte is 0xFF (no bucket has any candidate first byte). When true the entire chunk is skipped. This fires ~95% of the time on CJK haystacks with ASCII patterns, yielding 3–6× speedup over AC engines.

#### Wildcarding

Columns beyond a pattern's actual prefix length are wildcarded (bucket bit cleared for all 64 row entries in that column). This means patterns of different lengths coexist in the same unified mask tables without separate per-length matchers. A per-bucket `min_prefix_len` determines where wildcarding starts.

#### Verification

Bucket hits are verified via `BucketVerify`, which stores a `length_mask: u8` (bit `k-2` set ↔ prefix length `k` has entries) and a `PrefixMap` per registered prefix length. `PrefixMap` stores sorted parallel `keys: Box<[u64]>` and `values: Box<[PrefixGroup]>` arrays — binary search runs over the compact keys (contiguous `u64`s), then indexes into `PrefixGroup` only on a hit. Each `PrefixGroup` splits patterns into `exact_values` (prefix == full pattern) and `long_literals` (need suffix comparison).

```
  Verification Pipeline

  hit_mask (e.g. 0b00100010)
      │
      ├─► bucket 1 (bit 1)
      │       │
      │       ▼
      │   BucketVerify[1]
      │   length_mask = 0b00000101  (prefix lengths 2 and 4 registered)
      │       │
      │       ├─► len=2: PrefixMap.get(key) ── binary search on keys[]
      │       │     hit? ──► PrefixGroup
      │       │                ├─ exact_values[] ──► emit value
      │       │                └─ long_literals[] ──► compare suffix ──► emit
      │       │
      │       └─► len=4: PrefixMap.get(key) ── binary search on keys[]
      │             hit? ──► PrefixGroup (same as above)
      │
      └─► bucket 5 (bit 5)
              │
              ▼
          BucketVerify[5] ... (same structure)
```

#### SIMD Kernels

Three dispatch tiers:
- **AArch64 (NEON)**: compile-time intrinsics. 16-byte chunks via `uint8x16_t`. Early-exit via `vmaxvq_u8`. `max_prefix_len` determines lanes per chunk: `M = 16 - max_prefix_len + 1`.
- **x86-64 (AVX512-VBMI)**: runtime detection via `is_x86_feature_detected!("avx512vbmi")`. 64-byte chunks via `__m512i`. Uses `_mm512_permutexvar_epi8` for the dual-index lookup.
- **Scalar fallback**: byte-at-a-time through `match_mask_at` (column mask OR loop).

When `all_patterns_ascii` is true, dedicated ASCII-skip variants skip non-ASCII haystack bytes entirely (matches can only start at ASCII bytes).

#### Single-Byte Handling

Patterns of length 1 bypass the column-vector scan and are matched via a `single_byte_values: Box<[Vec<u32>; 128]>` lookup table. A `single_byte_match_mask: [u64; 2]` bitmask enables O(1) `is_match` for single-byte patterns. SIMD-accelerated single-byte scanning (NEON/AVX512) is used for `is_match` on ASCII haystacks with ≤ 4 distinct single-byte patterns.

#### Threading

`HarryMatcher` is immutable after construction. All mask tables and verification data are read-only. No thread-local state is needed — pure computation from pattern tables.

### Pass 1: Pattern Scanning

`search.rs` implements the runtime half. The unified entry point is `walk_and_scan`, which walks the process-type trie and scans each variant immediately after production:

- `walk_and_scan(text, exit_early, results)` — Walks the `ProcessTypeBitNode` trie once. Both leaf and non-leaf nodes materialize their transform output via `TransformStep::apply` (benefiting from SIMD-optimized bulk processing), then scan the result via `scan_variant`. Leaf nodes use a pooled `String` returned immediately after scanning; non-leaf nodes store their output in a `Vec<Cow<str>>` arena for children. When `exit_early=true` (`is_match`), stops on first satisfied rule. When `exit_early=false` (`process`), breaks early when all rules are resolved (see [Variant-Level Early Termination](#variant-level-early-termination)), otherwise exhausts all variants and calls `RuleSet::collect_matches`.
- `scan_variant` — Calls `ScanPlan::for_each_match_value` with `process_match` as the callback.
- `process_match` — Checks `DIRECT_RULE_BIT` inline to handle the common DirectRule case without calling `dispatch()`: extracts `pt_index` and `rule_idx` directly from the bit-packed value, checks `process_type_mask`, calls `mark_positive` (incrementing `resolved_count` on first positive), and returns `ctx.exit_early`. Falls through to `PatternIndex::dispatch` only for the rare non-DirectRule values (`SingleEntry` / `Entries`).
- `process_simple` — AllSimple fast path. Also checks `DIRECT_RULE_BIT` inline and extracts `rule_idx` directly (skipping `pt_index` entirely since AllSimple matchers have a single ProcessType). Falls through to `dispatch()` only for shared-pattern entries.

For the AC DFA bytewise engine, both `for_each_match_value` and `is_match` use hand-written state-stepping loops (`next_state` / `is_special` / `is_match` per byte) rather than the `try_find_overlapping_iter` or `try_find` iterator APIs. This eliminates iterator protocol overhead: all overlapping matches at a given DFA state are processed in one tight inner loop without re-entry. The `is_match` loop additionally checks `is_dead(sid)` for early exit. The DFA pre-computes per-state match lists, so `match_len(sid)` / `match_pattern(sid, i)` enumerate all overlapping matches at O(1) per match.

### Pass 2: Logical Evaluation

After all variants are scanned:
- **`is_match`**: `RuleSet::has_match` iterates `touched_indices` and checks each rule via `state.rule_is_satisfied(rule_idx)`.
- **`process`/`process_into`**: `RuleSet::collect_matches` iterates `touched_indices` and calls `push_result` for each satisfied rule, producing a `SimpleResult` with `word_id` and `word: Cow::Borrowed(&cold.word)`.

A rule is satisfied when:
- `positive_generation == generation` → all AND segments were satisfied.
- `not_generation != generation` → no NOT segment vetoed the rule.

---

## State Management

### Per-Rule State: RuleHot, RuleCold, WordState

Rules are split into hot and cold structs to keep cache lines tight during Pass 1:

**`RuleHot`** (accessed for every pattern hit in Pass 1, stored in `RuleSet`):
- `segment_counts: Vec<i32>` — per-segment initial counters. AND segments `[0..and_count)` start at their required occurrence count (typically `+1`). NOT segments `[and_count..)` start at their allowance (typically `0`; `-1` if one occurrence is tolerated).
- `and_count: usize` — boundary between AND and NOT segments.
- `use_matrix: bool` — `true` when `and_count > 64`, total segments > 64, any AND segment has count ≠ 1, or any NOT segment has count ≠ 0.
- `has_not: bool` — `true` when `and_count != segment_counts.len()`.

**`RuleCold`** (accessed only in Pass 2, stored in `RuleSet`):
- `word_id: u32` — caller-assigned identifier.
- `word: String` — original pattern string.

**`WordState`** (per-rule mutable state, one per rule in `SimpleMatchState.word_states`):
- `matrix_generation: u32` — set on first touch; enables lazy initialization.
- `positive_generation: u32` — set when all AND segments are satisfied.
- `not_generation: u32` — set when any NOT segment fires; permanently disqualifies the rule for this query.
- `satisfied_mask: u64` — bitmask tracking which AND segments have fired (bitmask fast path).
- `remaining_and: u16` — count of AND segments still unsatisfied; reaching 0 means satisfaction.

### Generation-Based State Reuse

`SimpleMatchState` avoids clearing its arrays between queries using a monotonic `generation: u32` counter, bumped in `prepare()`. A `WordState` field is considered unset if it does not match the current generation — an O(1) check that replaces the O(N) zero-fill that would otherwise be needed.

On `u32::MAX` overflow, all generation fields in `word_states` are explicitly reset to 0 before incrementing to 1. This happens once every ~4 billion queries, so it has negligible amortized cost.

### Sparse Set: touched_indices

`touched_indices: Vec<usize>` records which rules were first-touched during Pass 1. Pass 2 iterates only these entries instead of the full `word_states` array. This keeps evaluation cost proportional to the number of rules that received hits, not the total rule count. Cleared at the start of each scan in `prepare()`.

### PatternKind Dispatch

Each `PatternEntry` carries a `PatternKind` enum (`repr(u8)`) determined at construction time:

| Kind | Condition | Behavior in `process_entry` |
|------|-----------|----------------------------|
| `Simple` | `and_count == 1`, no NOT, no matrix | Skips all counter/bitmask logic. First touch sets `positive_generation` immediately. Subsequent hits for the same rule are a single generation comparison. |
| `And` | `offset < and_count` | Decrements a counter or sets a bitmask bit. Checks for full satisfaction. |
| `Not` | `offset >= and_count` | Increments a counter or sets `not_generation`. Permanently disqualifies the rule. |

Dispatching on a pre-computed enum avoids re-deriving the category from `offset` and `RuleHot` fields on every hit.

### PatternIndex and PatternDispatch

`PatternIndex` (in `rule.rs`) holds the flattened pattern entries and their bucket ranges. During construction, each unique pattern string may be attached to one or more `PatternEntry` values. Those per-pattern buckets are flattened into a single contiguous `entries: Vec<PatternEntry>`, and `ranges: Vec<(usize, usize)>` records the `(start, len)` slice for each deduplicated pattern id.

`PatternIndex::dispatch(raw_value) -> PatternDispatch` resolves a raw scan value:

| Variant | Condition | Behavior |
|---------|-----------|----------|
| `DirectRule(rule_idx)` | `raw_value & DIRECT_RULE_BIT != 0` | The rule index is decoded directly. No entry table lookup. |
| `SingleEntry(&entry)` | Entry slice has `len == 1` | Returns a reference to the single entry. Avoids loop overhead. |
| `Entries(&[entry])` | Entry slice has `len > 1` | Returns the full entry slice for iteration. |

All accesses use `get_unchecked` guarded by `debug_assert!`.

### DIRECT_RULE_BIT Fast Path

`DIRECT_RULE_BIT = 1 << 31` is used to encode the rule index directly in the automaton's raw value for single-entry simple patterns. When `PatternIndex::build_value_map` detects that a deduplicated pattern has exactly one `PatternEntry` with `kind == PatternKind::Simple`, it stores `rule_idx | DIRECT_RULE_BIT` instead of the dedup index.

At scan time, `PatternIndex::dispatch` checks the high bit first. If set, it returns `PatternDispatch::DirectRule(rule_idx)`, skipping the entry table entirely. This eliminates two indirections (range lookup + entry access) for the common case of simple single-pattern rules.

### Rule Evaluation Path Selection

Each rule is routed to one of three evaluation strategies at construction time, based on its pattern structure:

```
  Rule parsed from pattern string
          │
          ▼
  ┌─ and_count == 1, no NOT, no matrix? ──┐
  │  YES                                    │ NO
  ▼                                         ▼
 Simple                          ┌─ and_count ≤ 64 AND
 (PatternKind::Simple)           │  total segs ≤ 64 AND
  • first hit sets               │  no repeated AND  AND
    positive_generation          │  no repeated NOT?
  • no counters, no mask         │  YES            NO
                                 ▼                  ▼
                             Bitmask             Matrix
                              • satisfied_mask    • TinyVec<[i32;16]>
                                (u64 bit per      • [segs × variants]
                                 AND segment)       counter grid
                              • remaining_and     • matrix_status[]
                                (countdown)         tracks thresholds
```

### Bitmask Fast Path

Rules with all of: `and_count <= 64`, total segments `<= 64`, no repeated AND sub-pattern (count == 1), and no repeated NOT sub-pattern (count == 0), use the bitmask fast path:

- Each AND hit sets bit `offset` in `satisfied_mask` and decrements `remaining_and` (only on the first set for that bit).
- `remaining_and == 0` marks the rule as satisfied by setting `positive_generation = generation`.
- NOT hits immediately set `not_generation = generation`.
- `and_count == 1` is special-cased to skip the bitmask entirely and set `positive_generation` directly.

Early exit is returned when `exit_early && is_satisfied && !has_not && not_generation != generation`.

### Matrix Fallback

Rules exceeding bitmask capacity use a flat `TinyVec<[i32; 16]>` counter matrix, lazily initialized on first touch via `init_matrix` (in `state.rs`):

- Layout: `[num_segments × num_text_variants]`. Row `s`, variant `t` is at index `s * num_variants + t`.
- AND cells start at their `segment_counts` value (e.g. `+1`). A hit decrements the cell. When any variant's cell reaches `<= 0` (tracked by `matrix_status[segment]`), the segment is satisfied and `remaining_and` decrements.
- NOT cells start at their `segment_counts` value (e.g. `0`). A hit increments the cell. When any variant's cell exceeds `0`, the NOT fires and `not_generation` is set.
- `matrix_status: TinyVec<[u8; 16]>` tracks per-segment terminal state to avoid re-crossing the threshold on duplicate hits.
- `TinyVec<[i32; 16]>` stores up to 16 elements inline (covering rules with up to 16 segments × 1 variant), heap-allocating only for larger rules.
- `init_matrix` is marked `#[cold] #[inline(never)]` because it is rarely called (only on first touch of a matrix-path rule) and keeping it out-of-line improves instruction cache density on the hot path.

Matrix and status arrays are stored per-rule in `SimpleMatchState::matrix` and `SimpleMatchState::matrix_status`, both `Vec<TinyVec<...>>` indexed by rule id. They grow monotonically and are reused across calls (reset is implicit via `matrix_generation`).

### AllSimple Fast Path

When `SearchMode::AllSimple` is active (single `ProcessType::None`, every pattern is a simple literal with no `&`/`~`), both `is_match` and `process`/`process_into` use dedicated fast paths that bypass `walk_and_scan` entirely:

- **`is_match`** calls `is_match_simple`, which delegates directly to `ScanPlan::is_match(text)`. This dispatches to Harry (when present and applicable), the AC DFA, or the DAAC bytewise engine — completely bypassing TLS state, generation counters, `SimpleMatchState`, and overlapping iteration.
- **`process_into`** calls `process_simple`, which scans the automaton via `ScanPlan::for_each_match_value`. Each hit is dispatched through `PatternDispatch` — `DirectRule` hits call `RuleSet::push_result_if_new`, which uses `SimpleMatchState::mark_positive` for generation-based deduplication. This avoids tree walk overhead while still correctly deduplicating results when the same pattern appears multiple times in the text.

---

## Memory and Resource Efficiency

### Thread-Local Storage

All mutable state is thread-local. `SimpleMatcher` itself is `Send + Sync` and can be shared via `Arc` with zero lock contention.

Two TLS slots are used, both declared with `#[thread_local]` (a nightly attribute that compiles to a direct TLS segment-register read on x86/aarch64, eliminating the `thread_local!` macro's `.with()` closure overhead):

| Slot | Type | Module | Purpose |
|------|------|--------|---------|
| `SIMPLE_MATCH_STATE` | `UnsafeCell<SimpleMatchState>` | `simple_matcher/state.rs` | Generation-stamped per-rule word states, counter matrices, touched-index list, `resolved_count` for variant-level early termination, and `walk_arena_capacity` for allocation-free tree walks. Reused across calls. |
| `STRING_POOL` | `UnsafeCell<Vec<String>>` | `process/string_pool.rs` | Recycled `String` allocations for transformation output. Bounded to 128 entries. |

`UnsafeCell` is used instead of `RefCell` to eliminate runtime borrow-checking overhead. This is sound because `#[thread_local]` guarantees single-threaded access, and the code structure prevents re-entrant borrowing — each TLS slot is borrowed in exactly one function scope with no recursive calls back into the same slot.

### String Pool

`get_string_from_pool(capacity)` pops a `String` from the thread-local pool (clearing it and reserving to the requested capacity), or allocates a new one if the pool is empty. `return_string_to_pool(s)` pushes a `String` back, bounded at 128 entries so thread-local memory stays predictable.

The pool is used throughout the transformation pipeline. When a `Cow::Owned` result is replaced by a new transformation step, the old owned string is returned to the pool. In `walk_and_scan`, all `Cow::Owned` arena strings are returned to the pool after scanning completes.

### Static Transform Step Cache

`TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8]` (in `step.rs`) holds one compiled step per single-bit `ProcessType`. Each entry is lazily initialized on first access and shared as `&'static` across all `SimpleMatcher` instances and threads. The `OnceLock` ensures initialization happens exactly once with no subsequent lock contention.

### Global Allocator

The crate replaces the system allocator with `mimalloc` (v3) globally for improved multi-threaded allocation throughput and reduced fragmentation.

---

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `perf` | on | Meta-feature enabling all performance optimizations: `dfa`, `simd_runtime_dispatch`, and `harry`. This is the default feature. |
| `dfa` | on (via `perf`) | Enables `aho-corasick` DFA mode for the bytewise scan engine when all patterns are ASCII and pattern count ≤ `AC_DFA_PATTERN_THRESHOLD` (7,000). Other scan paths use `daachorse`. ~10x more memory than DAAC equivalents, but higher throughput up to the cache boundary. Note: `NormalizeMatcher` always uses `aho-corasick` DFA regardless of this flag. |
| `simd_runtime_dispatch` | on (via `perf`) | Dynamically selects the best SIMD instruction set at runtime for transformation skip functions (AVX2 on x86-64 via `SimdDispatch` + `OnceLock`, NEON on aarch64 at compile time, portable `std::simd` fallback). Also enables the NEON and AVX512-VBMI kernels in the Harry backend. Without this flag, only the portable/scalar paths are compiled. |
| `harry` | on (via `perf`) | Enables the Harry column-vector SIMD scan backend. When present, `ScanPlan::is_match` dispatches to Harry for large pure-ASCII pattern sets (≥ 64 patterns) on non-ASCII haystacks or when no DFA exists. See [Harry Column-Vector SIMD Backend](#harry-column-vector-simd-backend). |
| `runtime_build` | off | Builds transformation tables at runtime from source text files in `process_map/` instead of loading precompiled binary artifacts from `build.rs`. Slower initialization but allows custom or updated transformation data without recompiling the library. |

---

## Compiled vs. Runtime Transformation Tables

**Static (default):** `build.rs` pre-compiles all transformation tables into binary artifacts embedded in the library via `include_bytes!` / `include_str!`. At runtime, they are decoded lazily on first access by the step registry: `decode_u16_table` / `decode_u32_table` for page tables, compilation from pattern strings for the Normalize `aho-corasick` DFA. Zero startup cost beyond the first-use initialization.

**Runtime (`runtime_build` feature):** Tables are parsed from the raw source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) in `process_map/` at process startup. The `build_2_stage_table` helper in `replace.rs` converts sparse codepoint maps into the two-stage page-table layout. Slower initialization but allows dynamic rules or updated Unicode data without recompiling.
