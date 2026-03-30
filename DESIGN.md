# Design

This document describes the internal architecture of `matcher_rs` as it exists in the codebase today. It is intended for contributors and anyone integrating the library at a low level. Where the code has a non-obvious reason for doing something a particular way, this document explains the reasoning.

## Table of Contents

- [Text Transformation Pipeline](#text-transformation-pipeline)
  - [ProcessType Bitflags](#processtype-bitflags)
  - [Transformation Backends](#transformation-backends)
  - [TransformStep and StepOutput](#transformstep-and-stepoutput)
  - [Step Registry](#step-registry)
  - [Transformation DAG (ProcessTypeBitNode Tree)](#transformation-dag-processtypebitnode-tree)
  - [walk_process_tree](#walk_process_tree)
- [SimpleMatcher](#simplematcher)
  - [Input Format](#input-format)
  - [Pattern Syntax](#pattern-syntax)
  - [Construction](#construction)
  - [Three-Component Architecture](#three-component-architecture)
  - [SearchMode](#searchmode)
  - [Two-Pass Matching](#two-pass-matching)
  - [Scan Engine Selection](#scan-engine-selection)
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
  - [Const-Generic SINGLE_PT Dispatch](#const-generic-single_pt-dispatch)
- [Memory and Resource Efficiency](#memory-and-resource-efficiency)
  - [Thread-Local Storage](#thread-local-storage)
  - [String Pool](#string-pool)
  - [ProcessedTextMasks Pool](#processedtextmasks-pool)
  - [Static Transform Step Cache](#static-transform-step-cache)
  - [Global Allocator](#global-allocator)
- [Feature Flags](#feature-flags)
- [Compiled vs. Runtime Transformation Tables](#compiled-vs-runtime-transformation-tables)

---

## Text Transformation Pipeline

### ProcessType Bitflags

`ProcessType` is a `u8` bitflags type (via the `bitflags` crate) where each bit selects one transformation step. Flags compose freely with `|`:

| Flag | Bit | Description |
|------|-----|-------------|
| `None` | `0b00000001` | No transformation; match against the raw input. |
| `Fanjian` | `0b00000010` | Traditional Chinese to Simplified Chinese conversion. |
| `Delete` | `0b00000100` | Remove symbols, punctuation, and whitespace from the configured delete tables. |
| `Normalize` | `0b00001000` | Multi-character replacement via normalization tables (full-width forms, digit-like variants, etc.). |
| `PinYin` | `0b00010000` | Chinese characters to space-separated Pinyin syllables. |
| `PinYinChar` | `0b00100000` | Chinese characters to Pinyin with inter-syllable spaces stripped. |

Named aliases exist for common combinations: `DeleteNormalize` (0b00001100) and `FanjianDeleteNormalize` (0b00001110). These are the same as composing the individual flags with `|`.

The default value is `ProcessType::empty()` (no bits set), which differs from `ProcessType::None` (the explicit "raw text" flag at bit 0). `ProcessType::iter()` yields individual single-bit flags in ascending bit order.

Source data for each transformation:

| Map | Source Files | Used By |
|-----|-------------|---------|
| `FANJIAN` | `Unihan_Variants.txt`, `EquivalentUnifiedIdeograph.txt` | `Fanjian` |
| `TEXT-DELETE` | `DerivedGeneralCategory.txt` (symbols + punctuation) | `Delete` |
| `WHITE-SPACE` | Hardcoded 27 Unicode whitespace codepoints | `Delete` |
| `NORM` | `NormalizationTest.txt`, `DerivedGeneralCategory.txt` (alphanumeric + symbol variations) | `Normalize` |
| `NUM-NORM` | `DerivedNumericValues.txt` | `Normalize` |
| `PINYIN` / `PINYIN-CHAR` | `Unihan_Readings.txt` | `PinYin`, `PinYinChar` |

### Transformation Backends

Each single-bit `ProcessType` maps to a low-level engine in `process/transform/`. The engine owns the compiled data structures for one class of transformation:

| ProcessType | Engine | Module | Data Structure | Complexity |
|---|---|---|---|---|
| `Fanjian` | `FanjianMatcher` | `charwise.rs` | 2-stage page table. L1: `Box<[u16]>` (one per 256-codepoint block). L2: dense `Box<[u32]>` pages. A zero L1 entry means the entire block has no mapping. | O(1) per codepoint |
| `PinYin` / `PinYinChar` | `PinyinMatcher` | `charwise.rs` | Same 2-stage page table, but L2 values pack `(offset << 8 \| length)` into a concatenated UTF-8 string buffer (`Cow<'static, str>`). `PinYinChar` trims leading/trailing spaces from each packed entry at construction time via `trim_pinyin_packed`. | O(1) per codepoint |
| `Delete` | `DeleteMatcher` | `delete.rs` | ~139 KB flat BitSet covering U+0000 to U+10FFFF (`Cow<'static, [u8]>`). A 16-byte `ascii_lut` copy of the first 128 bits is kept inline for cache-hot ASCII checks. Uses a two-phase delete scan (seek + copy-skip) with SIMD bulk-skip of non-deletable ASCII. | O(1) per codepoint, branchless |
| `Normalize` | `NormalizeMatcher` | `normalize.rs` | `NormalizeEngine` enum wrapping either `daachorse` charwise Aho-Corasick (leftmost-longest, non-`dfa`) or `aho-corasick` DFA (`dfa` feature). Paired with a `replace_list: Vec<&'static str>` so pattern index `i` maps directly to its replacement. A `NormalizeFindIter` adapter normalizes the output format across backends. | O(N) per text |
| `None` | `TransformStep::None` | `step.rs` | No-op step that preserves the input variant. | - |

The page-table lookup for Fanjian and Pinyin (shared `page_table_lookup` function in `charwise.rs`):
```
page = l1[cp >> 8]       // which 256-codepoint block?
if page == 0 → no mapping
value = l2[page * 256 + (cp & 0xFF)]
if value == 0 → no mapping
```

L1 and L2 are accessed via `get_unchecked` with a bounds check on L1 and a `debug_assert!` on L2.

#### SIMD-Accelerated Skip Functions

The charwise iterators and delete scan use SIMD to skip over bytes that cannot produce a match, avoiding per-byte branching. All skip functions live in `transform/simd.rs`:

| Caller | Skip Function | What It Skips |
|--------|--------------|---------------|
| `FanjianFindIter` | `skip_ascii_simd` | All ASCII bytes (Fanjian only maps non-ASCII CJK codepoints) |
| `DeleteMatcher::delete` | `skip_ascii_non_delete_simd` | ASCII bytes that are NOT in the delete bitset (probes the 16-byte `ascii_lut` via SIMD table lookup) |
| `PinyinFindIter` | `skip_non_digit_ascii_simd` | Non-digit ASCII bytes (the Pinyin table only has entries for ASCII digits and non-ASCII codepoints) |

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

#### In-Place Fanjian Optimization

`FanjianMatcher::replace` has a same-length fast path: on first hit, it clones `text` into a pooled `String` from `get_string_from_pool` and overwrites the matched span directly via `unsafe { mapped.encode_utf8(&mut buf.as_bytes_mut()[start..end]) }`, avoiding the scan-and-rebuild allocations of the generic `replace_scan` path. Only when a replacement has a different UTF-8 byte width does it abandon the in-place buffer (returning it to the pool) and fall back to a full re-scan through `replace_scan`, which builds a new `String` by copying unchanged spans and pushing replacement chars.

#### UTF-8 Decoding

Both `charwise.rs` and `delete.rs` contain a private `decode_utf8_raw`/`decode_utf8` function that decodes one non-ASCII UTF-8 codepoint from `bytes[offset..]` using `get_unchecked` reads. These are kept as separate copies rather than shared to avoid cross-module coupling in the hot path. The functions handle 2, 3, and 4-byte sequences by branching on the lead byte's high bits.

### TransformStep and StepOutput

`TransformStep` (in `step.rs`) wraps one of the low-level engines and provides a uniform `apply(&self, text: &str, parent_is_ascii: bool) -> StepOutput` interface. The six variants are: `None`, `Fanjian(FanjianMatcher)`, `Delete(DeleteMatcher)`, `Normalize(NormalizeMatcher)`, `PinYin(PinyinMatcher)`, `PinYinChar(PinyinMatcher)`.

`StepOutput` carries two fields:
- `changed: Option<String>` — `None` when the step is a no-op (the text was unmodified). `Some(result)` when the text was transformed.
- `is_ascii: bool` — always describes the *post-step* text, regardless of whether the text changed.

The `is_ascii` policy per step:
- **Fanjian** — always `false` (output may contain CJK).
- **Delete** — `parent_is_ascii || is_ascii` (deletion can only remove non-ASCII chars, so if parent was ASCII the output is too; otherwise rescans).
- **Normalize** — rescans the output with `str::is_ascii()`.
- **PinYin / PinYinChar** — always `true` (Pinyin is ASCII).

### Step Registry

`registry.rs` holds `TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8]` — one slot per bit position in the `u8` bitflags. `get_transform_step(process_type_bit)` uses `trailing_zeros()` as the array index, and initializes the slot on first access via `build_transform_step`.

Two build paths are feature-gated:
- **Default (not `runtime_build`)**: Deserializes or builds from build-time artifacts (`include_bytes!`/`include_str!` constants in `transform/constants.rs`). For the Normalize step, the `dfa` feature chooses between compiling an `aho-corasick` DFA from pattern strings or deserializing a `daachorse` automaton from bytes.
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
- `process_type_list: Vec<ProcessType>` — which composite `ProcessType` values terminate at this node.
- `children: Vec<usize>` — flat-array indices of the next transformation steps reachable from here.
- `step: Option<&'static TransformStep>` — a cached reference to the compiled step for this node (fetched from the registry at construction time). The root stores `None`.
- `folded_mask: u64` — pre-computed OR of `1u64 << pt.bits()` for every composite type in `process_type_list`. Used to tag output text variants so the scan phase can filter hits by process type without re-deriving the mask.

The "sequential index" (`pt_index`) deserves explanation. Raw `ProcessType::bits()` values can use bits up to position 5, and composite types produce values up to 0b00111111 = 63. Storing a full `u64` mask per `PatternEntry` would waste space. Instead, `build_pt_index_table` assigns each composite type used in the current matcher a sequential index 0, 1, 2, ... (with `ProcessType::None` always at 0). These compact indices let `PatternEntry.pt_index` fit in a `u8` while `folded_mask` stays a `u64` with small bit positions.

After tree construction, `recompute_mask_with_index` rewrites every node's `folded_mask` from raw-bit encoding to sequential-index encoding so it matches the `pt_index` stored in `PatternEntry`.

### walk_process_tree

`walk_process_tree<const LAZY: bool, F>` (in `graph.rs`) traverses the trie, computing transformed text variants. It relies on the flat-array invariant that every parent node has a lower index than its children, so a single forward pass visits parents before children.

For each child node, the parent's text variant is transformed by the child's cached `TransformStep::apply`. The traversal only handles deduplication and mask propagation; per-step behavior lives behind `TransformStep::apply`, which returns a `StepOutput` with the changed string (if any) and the resulting `is_ascii` flag.

A `dedup_insert` function prevents duplicate text variants: if two trie paths converge on the same string (comparing by length first, then content), the existing entry is reused and its `mask` is OR'd with the new type's mask. Duplicate strings are returned to the pool via `return_string_to_pool`.

**`LAZY=true` mode** (used by `is_match`): Calls `on_variant(text, index, mask, is_ascii)` as soon as each new unique variant is produced. If the callback returns `true`, the walk stops early. A "delta phase" at the end re-invokes the callback for any entry whose mask grew after its initial callback (due to dedup merging), passing only the delta bits. A `TinyVec<[u64; 8]>` named `scanned_masks` tracks what has already been scanned per variant.

**`LAZY=false` mode** (used by `process`/`process_into`): The callback is never called. The function simply returns all text variants with their final masks. Dead code for the callback is eliminated by the compiler.

A thread-local `TRANSFORM_STATE` provides the scratch buffer (`tree_node_indices: Vec<usize>`) that maps trie node index to text variant index, plus a pool of recycled `ProcessedTextMasks` vectors. Both are bundled into a single TLS slot to avoid two lookups per call.

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

3. **Build transformation tree and recompute masks.** `build_process_type_tree` produces the trie from the `HashSet` of process types, then `recompute_mask_with_index` re-encodes every node's `folded_mask` to use the sequential indices matching `PatternEntry.pt_index`.

4. **Choose search mode.** Determines `single_pt_index` if only one process type is used. Sets the base mode to `SingleProcessType { pt_index }` or `General`. After scan plan compilation, checks if the tree has no children and every pattern is `PatternKind::Simple` — if so, overrides to `AllSimple`.

5. **Compile scan engines.** `ScanPlan::compile` (in `engine.rs`) receives the deduplicated patterns and entries:
   - Builds a `PatternIndex` from the entry buckets (flattens into contiguous storage with parallel `ranges`).
   - Builds a value map via `PatternIndex::build_value_map`, which assigns each deduplicated pattern a `u32` scan value. In `AllSimple` and `SingleProcessType` modes, single-entry simple patterns get `rule_idx | DIRECT_RULE_BIT`.
   - Delegates to `compile_automata` which partitions patterns by `is_ascii()` and builds:
     - **ASCII matcher** (`AsciiMatcher`): With the `dfa` feature and `≤ 2000` patterns, uses `aho-corasick` DFA (`AcDfa` variant with a `to_value` remapping `Vec<u32>`). Above the threshold or without `dfa`, uses `daachorse` bytewise DAAC with user-supplied `u32` values.
     - **Non-ASCII matcher** (`NonAsciiMatcher`): `daachorse` charwise DAAC. When both ASCII and non-ASCII patterns exist, the charwise matcher is compiled over *all* patterns (not just the non-ASCII subset), so a single charwise scan covers everything for non-ASCII input text.
   - Either or both matchers may be `None` when the corresponding pattern class is absent.

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

**`ScanPlan`** bundles the optional ASCII and non-ASCII matchers with the `PatternIndex` that maps raw automaton values back to rule metadata. It provides `is_match(text)`, `for_each_match_value(text, is_ascii, callback)`, and `patterns()`.

**`RuleSet`** stores parallel `Vec<RuleHot>` and `Vec<RuleCold>` indexed by rule id. It provides `process_entry`, `has_match`, `collect_matches`, and `push_result_if_new`.

### SearchMode

`SearchMode` is an enum determined at construction time:

| Variant | Condition | Behavior |
|---------|-----------|----------|
| `AllSimple` | Tree has no children (only root) AND every `PatternEntry` has `kind == PatternKind::Simple` | Bypasses state tracking entirely. `is_match` delegates directly to `ScanPlan::is_match`. `process` uses `process_simple` which emits results immediately per hit with generation-based dedup. |
| `SingleProcessType { pt_index }` | Only one composite `ProcessType` in the rule map | Enables `const SINGLE_PT: bool = true` monomorphization. The process-type mask check in `process_entry` compiles away. Also enables `DIRECT_RULE_BIT` encoding. |
| `General` | Multiple process types or any non-simple pattern | Full state machine with process-type mask checks on every hit. |

### Two-Pass Matching

```
┌────────────────────────────────────────────────────────────────┐
│ Input text                                                     │
│   ↓                                                            │
│ walk_process_tree → [TextVariant₀, TextVariant₁, ...]          │
│   ↓                                                            │
│ ┌─── Pass 1: Pattern Scanning ───────────────────────────────┐ │
│ │ For each text variant:                                     │ │
│ │   Select ASCII or charwise engine based on is_ascii flag   │ │
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

`ScanPlan::for_each_match_value` selects the engine based on two factors: whether a non-ASCII matcher exists, and whether the current text variant is ASCII.

1. **No non-ASCII matcher** (all patterns are ASCII): Always uses the ASCII matcher, regardless of `text.is_ascii()`. This avoids calling `text.is_ascii()` entirely.
2. **ASCII text + non-ASCII matcher exists**: Uses the ASCII matcher only. This avoids the per-character overhead of the charwise engine on pure ASCII input.
3. **Non-ASCII text + non-ASCII matcher exists**: Uses the charwise matcher only. Since it was compiled over all patterns (when both ASCII and non-ASCII patterns exist), one scan covers everything.

`ScanPlan::is_match` uses the same selection logic but calls the engine's `is_match` method (non-overlapping, first-match-only) for maximum throughput.

### Pass 1: Pattern Scanning

`search.rs` implements the runtime half. The entry points are:

- `is_match_inner<SINGLE_PT>` — Walks the tree with `LAZY=true`, scanning each variant as it is produced. Each variant scan constructs a `ScanContext` and calls `scan_variant`. If the callback returns `true` (a rule is satisfied), the tree walk stops.
- `process_preprocessed_into` — Takes pre-computed `ProcessedTextMasks`, calls `scan_all_variants`, then `RuleSet::collect_matches`.
- `scan_all_variants` — Dispatches to `scan_all_variants_inner::<true>` or `::<false>` based on `single_pt_index()`.
- `scan_all_variants_inner<SINGLE_PT>` — Iterates all variants (skipping those with `mask == 0`), constructs a `ScanContext` for each, and calls `scan_variant`.
- `scan_variant<SINGLE_PT>` — Calls `ScanPlan::for_each_match_value` with `process_match` as the callback.
- `process_match<SINGLE_PT>` — Dispatches the raw value via `PatternIndex::dispatch::<SINGLE_PT>`:
  - `DirectRule(rule_idx)` → `state.mark_positive(rule_idx)`, returns `ctx.exit_early`.
  - `SingleEntry(entry)` → `RuleSet::process_entry::<SINGLE_PT>(entry, ctx, state)`.
  - `Entries(entries)` → Iterates entries, short-circuiting if any `process_entry` returns `true`.

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

`PatternIndex::dispatch<SINGLE_PT>(raw_value) -> PatternDispatch` resolves a raw scan value:

| Variant | Condition | Behavior |
|---------|-----------|----------|
| `DirectRule(rule_idx)` | `SINGLE_PT && raw_value & DIRECT_RULE_BIT != 0` | The rule index is decoded directly. No entry table lookup. |
| `SingleEntry(&entry)` | Entry slice has `len == 1` | Returns a reference to the single entry. Avoids loop overhead. |
| `Entries(&[entry])` | Entry slice has `len > 1` | Returns the full entry slice for iteration. |

All accesses use `get_unchecked` guarded by `debug_assert!`.

### DIRECT_RULE_BIT Fast Path

`DIRECT_RULE_BIT = 1 << 31` is used to encode the rule index directly in the automaton's raw value for single-entry simple patterns. When `PatternIndex::build_value_map` detects that a deduplicated pattern has exactly one `PatternEntry` with `kind == PatternKind::Simple` (and the mode is `AllSimple` or `SingleProcessType`), it stores `rule_idx | DIRECT_RULE_BIT` instead of the dedup index.

At scan time, `PatternIndex::dispatch::<true>` checks the high bit first. If set, it returns `PatternDispatch::DirectRule(rule_idx)`, skipping the entry table entirely. This eliminates two indirections (range lookup + entry access) for the common case of simple single-pattern rules.

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

When `SearchMode::AllSimple` is active (single `ProcessType::None`, every pattern is a simple literal with no `&`/`~`), both `is_match` and `process`/`process_into` use dedicated fast paths that bypass `walk_process_tree` and `TRANSFORM_STATE` entirely:

- **`is_match`** calls `is_match_simple`, which delegates directly to `ScanPlan::is_match(text)`. This selects the appropriate engine based on `text.is_ascii()` and the available matchers, and uses `find_iter(...).next().is_some()` or `AhoCorasick::is_match(...)` directly. Completely bypasses TLS state, generation counters, `SimpleMatchState`, and overlapping iteration.
- **`process_into`** calls `process_simple`, which scans the automaton via `ScanPlan::for_each_match_value` with `dispatch::<true>`. Each hit is dispatched through `PatternDispatch` — `DirectRule` hits call `RuleSet::push_result_if_new`, which uses `SimpleMatchState::mark_positive` for generation-based deduplication. This avoids the `walk_process_tree` overhead, `TRANSFORM_STATE` TLS access, and `ProcessedTextMasks` allocation/deallocation, while still correctly deduplicating results when the same pattern appears multiple times in the text.

### Const-Generic SINGLE_PT Dispatch

When all rules share a single `ProcessType`, `SearchMode::SingleProcessType { pt_index }` is selected. The scan functions are monomorphized over `const SINGLE_PT: bool`:

- `scan_all_variants` calls `scan_all_variants_inner::<true>` or `::<false>`.
- `is_match_inner` is called as `is_match_inner::<true>` or `::<false>`.
- `process_match::<true>` enables `PatternIndex::dispatch::<true>`, which checks `DIRECT_RULE_BIT` first.
- `RuleSet::process_entry::<true>` compiles away the `ctx.process_type_mask & (1u64 << pt_index) == 0` check entirely, since there is only one process type and every hit is guaranteed to match.

This eliminates a branch and a shift+AND per `PatternEntry` in the inner loop.

---

## Memory and Resource Efficiency

### Thread-Local Storage

All mutable state is thread-local. `SimpleMatcher` itself is `Send + Sync` and can be shared via `Arc` with zero lock contention.

Three TLS slots are used, all declared with `#[thread_local]` (a nightly attribute that compiles to a direct TLS segment-register read on x86/aarch64, eliminating the `thread_local!` macro's `.with()` closure overhead):

| Slot | Type | Module | Purpose |
|------|------|--------|---------|
| `SIMPLE_MATCH_STATE` | `UnsafeCell<SimpleMatchState>` | `simple_matcher/state.rs` | Generation-stamped per-rule word states, counter matrices, and touched-index list. Reused across calls. |
| `STRING_POOL` | `UnsafeCell<Vec<String>>` | `process/variant.rs` | Recycled `String` allocations for transformation output. Bounded to 128 entries. |
| `TRANSFORM_STATE` | `UnsafeCell<TransformThreadState>` | `process/variant.rs` | Node-index-to-text-index scratch buffer (`tree_node_indices`) + recycled `ProcessedTextMasks` vectors (`masks_pool`, bounded to 16). Bundled into one slot to save a TLS lookup per call. |

`UnsafeCell` is used instead of `RefCell` to eliminate runtime borrow-checking overhead. This is sound because `#[thread_local]` guarantees single-threaded access, and the code structure prevents re-entrant borrowing — each TLS slot is borrowed in exactly one function scope with no recursive calls back into the same slot.

### String Pool

`get_string_from_pool(capacity)` pops a `String` from the thread-local pool (clearing it and reserving to the requested capacity), or allocates a new one if the pool is empty. `return_string_to_pool(s)` pushes a `String` back, bounded at 128 entries so thread-local memory stays predictable.

The pool is used throughout the transformation pipeline. When a `Cow::Owned` result is replaced by a new transformation step, the old owned string is returned to the pool. `return_processed_string_to_pool` drains a `ProcessedTextMasks` vector, returning all owned strings to the pool and recycling the empty vector itself into `TRANSFORM_STATE.masks_pool`.

### ProcessedTextMasks Pool

`walk_process_tree` pops a recycled `ProcessedTextMasks` vector from `TRANSFORM_STATE.masks_pool` at the start of each call. After the caller finishes with the variants, `return_processed_string_to_pool` recycles both the individual strings and the vector itself. The transmute from `ProcessedTextMasks<'static>` to `ProcessedTextMasks<'a>` is sound because the pooled vectors are always empty (all `Cow<'_, str>` elements have been drained). Similarly, the reverse transmute after `drain()` is sound because an empty `Vec` holds no values and `Cow<'_, str>` has identical layout regardless of lifetime.

### Static Transform Step Cache

`TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8]` (in `registry.rs`) holds one compiled step per single-bit `ProcessType`. Each entry is lazily initialized on first access and shared as `&'static` across all `SimpleMatcher` instances and threads. The `OnceLock` ensures initialization happens exactly once with no subsequent lock contention.

### Global Allocator

The crate replaces the system allocator with `mimalloc` (v3) globally for improved multi-threaded allocation throughput and reduced fragmentation.

---

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `dfa` | on | Enables `aho-corasick` DFA mode for: (1) the ASCII scan engine when pattern count is `<= 2000` (via `AC_DFA_PATTERN_THRESHOLD`), and (2) the Normalize multi-character matcher. Other paths still use `daachorse`. ~10x more memory than NFA/DAAC equivalents, but higher throughput. |
| `simd_runtime_dispatch` | on | Dynamically selects the best SIMD instruction set at runtime for the transformation skip functions (AVX2 on x86-64 via `SimdDispatch` + `OnceLock`, NEON on aarch64 at compile time, portable `std::simd` fallback). Without this flag, only the portable path is compiled. |
| `runtime_build` | off | Builds transformation tables at runtime from source text files in `process_map/` instead of loading precompiled binary artifacts from `build.rs`. Slower initialization but allows custom or updated transformation data without recompiling the library. |

---

## Compiled vs. Runtime Transformation Tables

**Static (default):** `build.rs` pre-compiles all transformation tables into binary artifacts embedded in the library via `include_bytes!` / `include_str!`. At runtime, they are decoded lazily on first access by the step registry: `decode_u16_table` / `decode_u32_table` for page tables, deserialization for the DAAC Normalize matcher (or compilation from pattern strings for the DFA Normalize matcher). Zero startup cost beyond the first-use initialization.

**Runtime (`runtime_build` feature):** Tables are parsed from the raw source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) in `process_map/` at process startup. The `build_2_stage_table` helper in `charwise.rs` converts sparse codepoint maps into the two-stage page-table layout. Slower initialization but allows dynamic rules or updated Unicode data without recompiling.
