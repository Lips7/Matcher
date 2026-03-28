# Design

This document describes the internal architecture of `matcher_rs` as it exists in the codebase today. It is intended for contributors and anyone integrating the library at a low level. Where the code has a non-obvious reason for doing something a particular way, this document explains the reasoning.

## Table of Contents

- [Text Transformation Pipeline](#text-transformation-pipeline)
  - [ProcessType Bitflags](#processtype-bitflags)
  - [Transformation Backends](#transformation-backends)
  - [Transformation DAG (ProcessTypeBitNode Tree)](#transformation-dag-processtypebitnode-tree)
  - [walk_process_tree](#walk_process_tree)
- [SimpleMatcher](#simplematcher)
  - [Input Format](#input-format)
  - [Pattern Syntax](#pattern-syntax)
  - [Construction](#construction)
  - [Two-Pass Matching](#two-pass-matching)
  - [Scan Engine Selection](#scan-engine-selection)
  - [Pass 1: Pattern Scanning](#pass-1-pattern-scanning)
  - [Pass 2: Logical Evaluation](#pass-2-logical-evaluation)
- [State Management](#state-management)
  - [Per-Rule State: RuleHot, RuleCold, WordState](#per-rule-state-rulehot-rulecold-wordstate)
  - [Generation-Based State Reuse](#generation-based-state-reuse)
  - [Sparse Set: touched_indices](#sparse-set-touched_indices)
  - [PatternKind Dispatch](#patternkind-dispatch)
  - [Bitmask Fast Path](#bitmask-fast-path)
  - [Matrix Fallback](#matrix-fallback)
  - [all_simple Fast Path](#all_simple-fast-path)
  - [Const-Generic SINGLE_PT Dispatch](#const-generic-single_pt-dispatch)
- [Memory and Resource Efficiency](#memory-and-resource-efficiency)
  - [Thread-Local Storage](#thread-local-storage)
  - [String Pool](#string-pool)
  - [ProcessedTextMasks Pool](#processedtextmasks-pool)
  - [Static ProcessMatcher Cache](#static-processmatcher-cache)
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

Each single-bit `ProcessType` maps to a `ProcessMatcher` enum, either `SingleChar` (per-codepoint lookup) or `MultiChar` (Aho-Corasick multi-character substitution). The data structures are chosen to match the access pattern of each step:

| ProcessType | Backend | Data Structure | Complexity |
|---|---|---|---|
| `Fanjian` | `SingleCharMatcher::Fanjian` | 2-stage page table. L1: `u16[4352]` (one per 256-codepoint block). L2: dense `u32` pages. A zero L1 entry means the entire block has no mapping. | O(1) per codepoint |
| `PinYin` / `PinYinChar` | `SingleCharMatcher::Pinyin` | Same 2-stage page table, but L2 values pack `(offset << 8 \| length)` into a concatenated UTF-8 string buffer. `PinYinChar` trims leading/trailing spaces from each packed entry at construction time. | O(1) per codepoint |
| `Delete` | `SingleCharMatcher::Delete` | 139 KB flat BitSet covering U+0000 to U+10FFFF. A 16-byte `ascii_lut` copy of the first 128 bits is kept inline for cache-hot ASCII checks. Uses a specialized `delete_direct` single-pass scan (no iterator protocol) with SIMD bulk-skip of non-deletable ASCII. | O(1) per codepoint, branchless |
| `Normalize` | `MultiCharMatcher` | `daachorse` charwise Aho-Corasick (leftmost-longest, non-`dfa`) or `aho-corasick` DFA (`dfa` feature). Paired with a `replace_list: Vec<&'static str>` so pattern index `i` maps directly to its replacement. | O(N) per text |
| `None` | `MultiCharMatcher` (empty) | An empty Aho-Corasick automaton; no-op. | - |

The page-table lookup for Fanjian and Pinyin is:
```
page = l1[cp >> 8]       // which 256-codepoint block?
if page == 0 → no mapping
value = l2[page * 256 + (cp & 0xFF)]
if value == 0 → no mapping
```

#### SIMD-Accelerated Skip Functions

The `SingleCharMatcher` iterators and `delete_direct` use SIMD to skip over bytes that cannot produce a match, avoiding per-byte branching:

| Caller | Skip Function | What It Skips |
|--------|--------------|---------------|
| `FanjianFindIter` | `skip_ascii_simd` | All ASCII bytes (Fanjian only maps non-ASCII CJK codepoints) |
| `delete_direct` | `skip_ascii_non_delete_simd` | ASCII bytes that are NOT in the delete bitset (probes the 16-byte `ascii_lut` via SIMD table lookup) |
| `PinYinFindIter` | `skip_non_digit_ascii_simd` | Non-digit ASCII bytes (the Pinyin table only has entries for ASCII digits and non-ASCII codepoints) |

Each skip function dispatches to the best available kernel:

- **x86-64 with `simd_runtime_dispatch`**: Runtime detection via `is_x86_feature_detected!("avx2")`. A `SimdDispatch` struct holds function pointers initialized once (via `OnceLock`), so subsequent calls are a single pointer comparison. AVX2 paths use 32-byte chunks with `_mm256_movemask_epi8` for non-ASCII detection and `_mm256_shuffle_epi8` for bitset probing. Falls back to portable `std::simd`.
- **aarch64 with `simd_runtime_dispatch`**: Compile-time NEON intrinsics. 16-byte chunks using `vmaxvq_u8` for fast any-non-ASCII tests and `vqtbl1q_u8` for bitset probing.
- **Portable fallback**: `std::simd` with 32-lane (or 16-lane for tail) operations. Uses `swizzle_dyn` for the bitset lookup, which the compiler lowers to the appropriate target instructions.

All paths fall through to a scalar byte-at-a-time loop for the tail.

#### In-Place Fanjian Optimization

The `replace_all_fanjian` path exploits a property of the Fanjian mapping: 99%+ of Traditional-to-Simplified mappings produce a replacement character with the same UTF-8 byte length. When all replacements in a text are same-length, the method clones the input string once and overwrites the mapped spans directly via `unsafe { c.encode_utf8(&mut buf.as_bytes_mut()[start..end]) }`, avoiding the scan-and-rebuild allocations of the generic `replace_scan` path. On the rare byte-length mismatch, it abandons the in-place buffer and falls back to the standard path.

### Transformation DAG (ProcessTypeBitNode Tree)

When a `SimpleMatcher` is configured with multiple composite `ProcessType` values, their decomposed single-bit steps often share prefixes. For example, `Fanjian | Delete` and `Fanjian | Normalize` both start with a Fanjian step. Naively applying every composite pipeline independently would re-derive the Fanjian result twice.

`build_process_type_tree` constructs a flat-array trie that makes shared prefixes explicit:

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
- `process_type_list` — which composite `ProcessType` values terminate at this node.
- `children` — flat-array indices of the next transformation steps reachable from here.
- `matcher` — a cached `&'static ProcessMatcher` for this step (avoids a hash lookup in the hot traversal loop). The root stores `None`.
- `folded_mask` — pre-computed OR of `1u64 << pt_index` for every composite type in `process_type_list`. Used to tag output text variants so the scan phase can filter hits by process type without re-deriving the mask.

The "sequential index" (`pt_index`) deserves explanation. Raw `ProcessType::bits()` values can use bits up to position 5, and composite types produce values up to 0b00111111 = 63. Storing a full `u64` mask per `PatternEntry` would waste space. Instead, `build_pt_index_table` assigns each composite type used in the current matcher a sequential index 0, 1, 2, ... (with `ProcessType::None` always at 0). These compact indices let `PatternEntry.pt_index` fit in a `u8` while `folded_mask` stays a `u64` with small bit positions.

After tree construction, `recompute_mask_with_index` rewrites every node's `folded_mask` from raw-bit encoding to sequential-index encoding so it matches the `pt_index` stored in `PatternEntry`.

### walk_process_tree

`walk_process_tree<const LAZY: bool, F>` traverses the trie, computing transformed text variants. It relies on the flat-array invariant that every parent node has a lower index than its children, so a single forward pass visits parents before children.

For each child node, the parent's text variant is transformed by the child's cached `ProcessMatcher`. Both `replace_all` and `delete_all` return `Option<(String, bool)>` where the `bool` is an `is_ascii` flag for the result, avoiding a redundant post-hoc scan. The flag is determined per step using static knowledge:
- **Fanjian**: maps CJK to CJK, so the result is always non-ASCII if changed (`false`).
- **PinYin/PinYinChar**: output is romanized text, so always ASCII if changed (`true`).
- **Delete**: can only remove characters. If the parent was ASCII, the result stays ASCII. Otherwise, post-hoc SIMD `is_ascii()` check on the shorter result.
- **Normalize**: post-hoc SIMD `is_ascii()` check on the result (replacements can be ASCII or not).

A `dedup_insert` function prevents duplicate text variants: if two trie paths converge on the same string, the existing entry is reused and its `mask` is OR'd with the new type's mask. Duplicate strings are returned to the pool.

**`LAZY=true` mode** (used by `is_match`): Calls `on_variant(text, index, mask, is_ascii)` as soon as each new unique variant is produced. If the callback returns `true`, the walk stops early. A "delta phase" at the end re-invokes the callback for any entry whose mask grew after its initial callback (due to dedup merging), passing only the delta bits.

**`LAZY=false` mode** (used by `process`/`process_into`): The callback is never called. The function simply returns all text variants with their final masks. Dead code for the callback is eliminated by the compiler.

A thread-local `TRANSFORM_STATE` provides the scratch buffer (`tree_node_indices: Vec<usize>`) that maps trie node index to text variant index, plus a pool of recycled `ProcessedTextMasks` vectors. Both are bundled into one TLS slot to avoid two lookups per call.

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

`SimpleMatcher::new` follows these steps:

1. **Build sequential ProcessType index table.** `build_pt_index_table` assigns compact 0..N indices to each composite `ProcessType` used in the rule map. `ProcessType::None` always gets index 0.

2. **Parse rules.** `parse_rules` iterates the rule map. For each pattern:
   - Splits on `&` and `~` into AND and NOT sub-patterns.
   - De-duplicates sub-patterns within the rule, counting occurrences. AND patterns accumulate a positive count; NOT patterns accumulate a negative count. The resulting `segment_counts` vector has AND segments at `[0..and_count)` (initial values like `+1`, `+2` for repeated patterns) and NOT segments at `[and_count..)` (initial values like `0`, `-1`).
   - Generates all normalized text variants for each sub-pattern via `reduce_text_process_emit`, which applies `process_type - ProcessType::Delete` to the sub-pattern. The subtraction of `Delete` is deliberate: input text is Delete-transformed before scanning, so the sub-patterns must *not* be Delete-transformed themselves or they would be double-processed. They are stored verbatim and matched against the already-deleted text.
   - De-duplicates emitted pattern strings across all rules into a flat `dedup_patterns` list. Each unique pattern is assigned a dedup index. A `PatternEntry` links each dedup index back to its `(rule_idx, offset, pt_index, kind)`.
   - Determines `use_matrix` (whether the rule requires the matrix fallback) and `has_not` (whether the rule has any NOT segments).

3. **Compile scan engines.** `compile_automata` partitions deduplicated patterns into ASCII-only and non-ASCII buckets, then builds separate matchers:
   - **ASCII matcher**: With the `dfa` feature and `<=2000` patterns, uses `aho-corasick` DFA (`AcDfa` variant with a `to_dedup` remapping table). Above the threshold or without `dfa`, uses `daachorse` bytewise DAAC where the automaton value directly encodes the dedup index.
   - **Non-ASCII matcher**: `daachorse` charwise DAAC. When both ASCII and non-ASCII patterns exist, the charwise matcher is compiled over *all* patterns (not just the non-ASCII subset), so a single charwise scan covers everything for non-ASCII input text.

4. **Flatten dedup entries.** The per-pattern `Vec<PatternEntry>` lists are flattened into one contiguous `ac_dedup_entries` array, with a parallel `ac_dedup_ranges` array where `ranges[i] = (start, len)` maps dedup pattern index `i` to its slice.

5. **Build transformation tree and recompute masks.** `build_process_type_tree` produces the trie, then `recompute_mask_with_index` re-encodes every node's `folded_mask` to use the sequential indices matching `PatternEntry.pt_index`.

6. **Compute `all_simple`.** If the tree has only the root node (no transformations needed) and every `PatternEntry` has `kind == PatternKind::Simple`, this flag enables a zero-overhead `is_match` fast path.

### Two-Pass Matching

```
┌──────────────────────────────────────────────────────────────────┐
│ Input text                                                       │
│   ↓                                                              │
│ walk_process_tree → [TextVariant₀, TextVariant₁, ...]           │
│   ↓                                                              │
│ ┌─── Pass 1: Pattern Scanning ───────────────────────────────┐  │
│ │ For each text variant:                                      │  │
│ │   Select ASCII or charwise engine based on is_ascii flag    │  │
│ │   For each overlapping hit:                                 │  │
│ │     Look up PatternEntry slice via ac_dedup_ranges          │  │
│ │     For each entry: process_match updates WordState         │  │
│ │     (Early exit if exit_early && rule fully satisfied)       │  │
│ └─────────────────────────────────────────────────────────────┘  │
│   ↓                                                              │
│ ┌─── Pass 2: Logical Evaluation ─────────────────────────────┐  │
│ │ For each rule_idx in touched_indices:                        │  │
│ │   Check positive_generation == generation (all ANDs met)     │  │
│ │   Check not_generation != generation (no NOT vetoed)         │  │
│ │   If both: emit SimpleResult { word_id, word }               │  │
│ └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### Scan Engine Selection

For each text variant, the scan dispatches based on the variant's `is_ascii` flag:

1. **ASCII text + ASCII matcher exists**: Use the ASCII matcher only. This avoids the per-character overhead of the charwise engine on pure ASCII input.
2. **Non-ASCII text + non-ASCII matcher exists**: Use the charwise matcher only. Since it was compiled over all patterns (when both ASCII and non-ASCII patterns exist), one scan covers everything.
3. **Non-ASCII text + no non-ASCII matcher (only ASCII patterns)**: Fall through to the ASCII matcher. This handles the case where all patterns are ASCII but the input contains non-ASCII characters.

This three-way dispatch avoids running both engines when one suffices.

### Pass 1: Pattern Scanning

`scan_all_variants` iterates all text variants. For each variant whose `mask != 0`, it constructs a `ScanContext` bundling the variant's index, process-type mask, total variant count, early-exit flag, and ASCII flag, then calls `scan_variant`.

`scan_variant` runs the selected engine's `find_overlapping_iter`. For each hit, it obtains the dedup index (directly from the DAAC value, or via the `to_dedup` table for AcDfa) and calls `process_match`.

`process_match` looks up the `PatternEntry` slice for the dedup index via `ac_dedup_ranges`. When the slice has exactly one entry (`len == 1`), it calls `process_entry` directly on the single entry, avoiding loop setup overhead. For multi-entry slices, it iterates and calls `process_entry` for each. This single-entry fast path benefits the common case where most deduplicated patterns map to exactly one rule.

`process_entry` handles the per-entry logic. The check `ctx.process_type_mask & (1u64 << pt_index) == 0` filters hits from a text variant that does not match the entry's pipeline. When `SINGLE_PT=true`, this check compiles away entirely. All array accesses in the hot path (`ac_dedup_ranges`, `ac_dedup_entries`, `word_states`, `rule_hot`, `matrix`, `matrix_status`) use `get_unchecked` with `debug_assert!` guards, since all indices are construction-time invariants.

### Pass 2: Logical Evaluation

After all variants are scanned, Pass 2 iterates `touched_indices` and checks each rule:
- `positive_generation == generation` → all AND segments were satisfied.
- `not_generation != generation` → no NOT segment vetoed the rule.

Rules that pass both checks emit a `SimpleResult` with the rule's `word_id` and original pattern string (borrowed from `RuleCold`).

---

## State Management

### Per-Rule State: RuleHot, RuleCold, WordState

Rules are split into hot and cold structs to keep cache lines tight during Pass 1:

**`RuleHot`** (accessed for every pattern hit in Pass 1):
- `segment_counts: Vec<i32>` — per-segment initial counters. AND segments `[0..and_count)` start at their required occurrence count (typically `+1`). NOT segments `[and_count..)` start at their allowance (typically `0`; `-1` if one occurrence is tolerated).
- `and_count: usize` — boundary between AND and NOT segments.
- `use_matrix: bool` — `true` when `and_count > 64`, total segments > 64, any AND segment repeats, or any NOT segment has a non-zero allowance.
- `has_not: bool` — `true` when `and_count != segment_counts.len()`.

**`RuleCold`** (accessed only in Pass 2):
- `word_id: u32` — caller-assigned identifier.
- `word: String` — original pattern string.

**`WordState`** (per-rule mutable state, one per rule in `SimpleMatchState.word_states`):
- `matrix_generation: u32` — set on first touch; enables lazy initialization.
- `positive_generation: u32` — set when all AND segments are satisfied.
- `not_generation: u32` — set when any NOT segment fires; permanently disqualifies the rule for this query.
- `satisfied_mask: u64` — bitmask tracking which AND segments have fired (bitmask fast path).
- `remaining_and: u16` — count of AND segments still unsatisfied; reaching 0 means satisfaction.

### Generation-Based State Reuse

`SimpleMatchState` avoids clearing its arrays between queries using a monotonic `generation: u32` counter. A `WordState` field is considered unset if it does not match the current generation — an O(1) check that replaces the O(N) zero-fill that would otherwise be needed.

On `u32::MAX` overflow, all generation fields in `word_states` are explicitly reset to 0 before incrementing to 1. This happens once every ~4 billion queries, so it has negligible amortized cost.

### Sparse Set: touched_indices

`touched_indices: Vec<usize>` records which rules were first-touched during Pass 1. Pass 2 iterates only these entries instead of the full `word_states` array. This keeps evaluation cost proportional to the number of rules that received hits, not the total rule count.

### PatternKind Dispatch

Each `PatternEntry` carries a `PatternKind` enum determined at construction time:

| Kind | Condition | Behavior in `process_match` |
|------|-----------|----------------------------|
| `Simple` | `and_count == 1`, no NOT, no matrix | Skips all counter/bitmask logic. First touch sets `positive_generation` immediately. Subsequent hits for the same rule are a single generation comparison. |
| `And` | `offset < and_count` | Decrements a counter or sets a bitmask bit. Checks for full satisfaction. |
| `Not` | `offset >= and_count` | Increments a counter or sets `not_generation`. Permanently disqualifies the rule. |

Dispatching on a pre-computed enum avoids re-deriving the category from `offset` and `RuleHot` fields on every hit.

### Bitmask Fast Path

Rules with all of: `and_count <= 64`, total segments `<= 64`, no repeated AND sub-pattern, and no repeated NOT sub-pattern, use the bitmask fast path:

- Each AND hit sets bit `offset` in `satisfied_mask` and decrements `remaining_and` (only on the first set for that bit).
- `remaining_and == 0` marks the rule as satisfied by setting `positive_generation = generation`.
- NOT hits immediately set `not_generation = generation`.
- `and_count == 1` is special-cased to skip the bitmask entirely and set `positive_generation` directly.

### Matrix Fallback

Rules exceeding bitmask capacity use a flat `TinyVec<[i32; 16]>` counter matrix, lazily initialized on first touch via `init_matrix`:

- Layout: `[num_segments * num_text_variants]`. Row `s`, variant `t` is at index `s * num_variants + t`.
- AND cells start at their `segment_counts` value (e.g. `+1`). A hit decrements the cell. When any variant's cell reaches `<= 0` (tracked by `matrix_status[segment]`), the segment is satisfied and `remaining_and` decrements.
- NOT cells start at their `segment_counts` value (e.g. `0`). A hit increments the cell. When any variant's cell exceeds `0`, the NOT fires and `not_generation` is set.
- `matrix_status: TinyVec<[u8; 16]>` tracks per-segment terminal state to avoid re-crossing the threshold on duplicate hits.
- `TinyVec<[i32; 16]>` stores up to 16 elements inline (covering rules with up to 16 segments * 1 variant), heap-allocating only for larger rules.
- `init_matrix` is marked `#[cold] #[inline(never)]` because it is rarely called (only on first touch of a matrix-path rule) and keeping it out-of-line improves instruction cache density on the hot path.

### all_simple Fast Path

When `all_simple` is `true` (single `ProcessType::None`, every pattern is a simple literal with no `&`/`~`), both `is_match` and `process`/`process_into` use dedicated fast paths that bypass `walk_process_tree` and `TRANSFORM_STATE` entirely:

- **`is_match`** calls `is_match_simple`, which checks `text.is_ascii()`, dispatches to the appropriate engine, and uses `find_iter(...).next().is_some()` or `is_match(...)` directly. Completely bypasses TLS state, generation counters, `SimpleMatchState`, and overlapping iteration.
- **`process_into`** calls `process_simple`, which scans the automaton directly and uses generation-based deduplication from `SIMPLE_MATCH_STATE` to collect all matching rules. This avoids the `walk_process_tree` overhead, `TRANSFORM_STATE` TLS access, and `ProcessedTextMasks` allocation/deallocation, while still correctly deduplicating results when the same pattern appears multiple times in the text.

### Const-Generic SINGLE_PT Dispatch

When all rules share a single `ProcessType`, `single_pt_index` is `Some(idx)`. The scan functions are monomorphized over `const SINGLE_PT: bool`:

- `scan_all_variants` calls `scan_all_variants_inner::<true>` or `::<false>`.
- `is_match` calls `is_match_inner::<true>` or `::<false>`.
- `process_match::<true>` compiles away the `ctx.process_type_mask & (1u64 << pt_index) == 0` check entirely, since there is only one process type and every hit is guaranteed to match.

This eliminates a branch and a shift+AND per `PatternEntry` in the inner loop.

---

## Memory and Resource Efficiency

### Thread-Local Storage

All mutable state is thread-local. `SimpleMatcher` itself is `Send + Sync` and can be shared via `Arc` with zero lock contention.

Three TLS slots are used, all declared with `#[thread_local]` (a nightly attribute that compiles to a direct TLS segment-register read on x86/aarch64, eliminating the `thread_local!` macro's `.with()` closure overhead):

| Slot | Type | Purpose |
|------|------|---------|
| `SIMPLE_MATCH_STATE` | `UnsafeCell<SimpleMatchState>` | Generation-stamped per-rule word states, counter matrices, and touched-index list. Reused across calls. |
| `STRING_POOL` | `UnsafeCell<Vec<String>>` | Recycled `String` allocations for transformation output. Bounded to 128 entries. |
| `TRANSFORM_STATE` | `UnsafeCell<TransformThreadState>` | Node-index-to-text-index scratch buffer (`tree_node_indices`) + recycled `ProcessedTextMasks` vectors (`masks_pool`, bounded to 16). Bundled into one slot to save a TLS lookup per call. |

`UnsafeCell` is used instead of `RefCell` to eliminate runtime borrow-checking overhead. This is sound because `#[thread_local]` guarantees single-threaded access, and the code structure prevents re-entrant borrowing — each TLS slot is borrowed in exactly one function scope with no recursive calls back into the same slot.

### String Pool

`get_string_from_pool(capacity)` pops a `String` from the thread-local pool (clearing it and reserving to the requested capacity), or allocates a new one if the pool is empty. `return_string_to_pool(s)` pushes a `String` back, bounded at 128 entries so thread-local memory stays predictable.

The pool is used throughout the transformation pipeline. When a `Cow::Owned` result is replaced by a new transformation step, the old owned string is returned to the pool. `return_processed_string_to_pool` drains a `ProcessedTextMasks` vector, returning all owned strings to the pool and recycling the empty vector itself into `TRANSFORM_STATE.masks_pool`.

### ProcessedTextMasks Pool

`walk_process_tree` pops a recycled `ProcessedTextMasks` vector from `TRANSFORM_STATE.masks_pool` at the start of each call. After the caller finishes with the variants, `return_processed_string_to_pool` recycles both the individual strings and the vector itself. The transmute from `ProcessedTextMasks<'static>` to `ProcessedTextMasks<'a>` is sound because the pooled vectors are always empty (all `Cow<'_, str>` elements have been drained).

### Static ProcessMatcher Cache

`PROCESS_MATCHER_CACHE: [OnceLock<ProcessMatcher>; 8]` holds one compiled matcher per single-bit `ProcessType`. Each entry is lazily initialized on first access and shared as `&'static` across all `SimpleMatcher` instances and threads. The `OnceLock` ensures initialization happens exactly once with no subsequent lock contention.

### Global Allocator

The crate replaces the system allocator with `mimalloc` (v3) globally for improved multi-threaded allocation throughput and reduced fragmentation.

---

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `dfa` | on | Enables `aho-corasick` DFA mode for: (1) the ASCII scan engine when pattern count is `<= 2000`, and (2) the Normalize multi-character matcher. Other paths still use `daachorse`. ~10x more memory than NFA/DAAC equivalents, but higher throughput. |
| `simd_runtime_dispatch` | on | Dynamically selects the best SIMD instruction set at runtime for the transformation skip functions (AVX2 on x86-64, NEON on aarch64, portable `std::simd` fallback). Without this flag, only the portable path is compiled. |
| `runtime_build` | off | Builds transformation tables at runtime from source text files in `process_map/` instead of loading precompiled binary artifacts from `build.rs`. Slower initialization but allows custom or updated transformation data without recompiling the library. |

---

## Compiled vs. Runtime Transformation Tables

**Static (default):** `build.rs` pre-compiles all transformation tables into binary artifacts embedded in the library via `include_bytes!`. At runtime, they are decoded lazily on first access (byte-slice casts for page tables, deserialization for the DAAC Normalize matcher). Zero startup cost beyond the first-use initialization.

**Runtime (`runtime_build` feature):** Tables are parsed from the raw source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) in `process_map/` at process startup. Slower initialization but allows dynamic rules or updated Unicode data without recompiling.
