# Design

How `matcher_rs` works, explained through a concrete example. The library is a three-phase pipeline: **Transform** text variants → **Scan** with Aho-Corasick automata → **Evaluate** rule satisfaction. Construction compiles rules into this pipeline; queries execute it.

## Table of Contents

- [Running Example](#running-example)
- [1. Construction](#1-construction)
- [2. Query](#2-query)
- [3. `is_match` Fast Path](#3-is_match-fast-path)
- [Why It's Fast](#why-its-fast)
- [Transform Pipeline](#transform-pipeline)
- [Feature Flags](#feature-flags)

---

## Running Example

Three rules, each using a different text transformation:

| Rule | ProcessType | word_id | Pattern |
|------|-------------|---------|---------|
| R1 | `None` | 1 | `"hello&world"` |
| R2 | `VariantNorm \| Delete` | 2 | `"你好"` |
| R3 | `Romanize` | 3 | `"zhongguo"` |

Query text: `"Hello! 你好世界 china is cool"`

---

## 1. Construction

`SimpleMatcher::new` runs four stages.

### 1.1 Parse Rules

Each rule string is split on `&` (AND), `~` (NOT), and `|` (OR within a segment):

**R1: `"hello&world"` under `ProcessType::None`** — two AND segments, both simple literals. Both sub-patterns emit unchanged since `None` has no transforms.

**R2: `"你好"` under `VariantNorm | Delete`** — single AND segment. Emitted under `VariantNorm` (Delete is subtracted from the indexing ProcessType — see below). Already in normalized form, so emits unchanged.

**R3: `"zhongguo"` under `Romanize`** — single AND segment. Emitted under `Romanize`. Pure ASCII, so emits unchanged.

**Why subtract Delete?** Delete is the only non-bijective transform — it destroys information. Patterns are stored verbatim (not delete-transformed), and the AC automaton scans **both** the original text and the delete-transformed text. This dual scan is necessary because patterns may contain deletable characters that only exist in the original text. Indexing patterns under `process_type - Delete` avoids applying Delete to the pattern itself.

**Operators:**

- **AND (`&`):** All segments must match for the rule to fire. `"hello&world"` requires both `"hello"` and `"world"` in the text.
- **NOT (`~`):** Any NOT segment vetoes the rule. `"hello~spam"` matches text with `"hello"` but not `"spam"`.
- **OR (`|`):** Alternatives within a segment. `"color|colour&bright"` means (color OR colour) AND bright. OR binds tighter than `&`/`~`.
- **Word boundaries (`\b`):** `"\bcat\b"` matches `"cat"` only at word boundaries. Checked at hit positions during scanning.

After deduplication, the flat pattern table is:

```
patterns: ["hello", "world", "你好", "zhongguo"]
```

Each pattern has metadata linking it back to its rule, segment offset, ProcessType index, and operator kind (AND or NOT).

### 1.2 Build Transform Trie

The three ProcessTypes — `{None, VariantNorm|Delete, Romanize}` — are decomposed into single-bit steps and merged into a shared-prefix trie:

```
[0] Root (None)  ← terminates: has rules under ProcessType::None
 ├─[1] VariantNorm
 │  └─[2] Delete ← terminates: has rules under VariantNorm|Delete
 └─[3] Romanize  ← terminates: has rules under Romanize
```

Each node caches a reference to a lazily-initialized transform step. The root applies no transformation. Shared prefixes reuse intermediate results: if both `VariantNorm` and `VariantNorm|Delete` exist, the VariantNorm output is computed once and shared.

### 1.3 Compile Scan Engines

Two Aho-Corasick engines are built from **all** deduplicated patterns:

- **Bytewise engine**: operates on raw bytes. With the `dfa` feature, uses a `dfa::DFA` accessed via the `Automaton` trait. When the DFA has a Teddy SIMD prefilter (<~100 patterns), the prefilter is exploited by materializing text first and using a stateful `try_find_overlapping` loop; otherwise, a custom `next_state` byte loop eliminates iterator overhead and runs directly over the transform stream without materialization. Falls back to DAAC without `dfa`.
- **Charwise engine**: operates on Unicode codepoints. CJK characters are 3 UTF-8 bytes — charwise does 1 state transition instead of 3, making it ~1.6–1.9× faster on CJK-heavy text.

Both engines are correct for any input. Engine selection is a pure speed optimization decided at runtime per text based on character density (codepoints / bytes, via `bytecount::num_chars`).

### 1.4 Assemble

The final `SimpleMatcher` holds: the transform trie, the dual scan engines with pattern metadata, the compiled rule set, and a flag indicating whether the `is_match` AC-direct fast path is available. The fast path is enabled only when all rules are simple literals under a single ProcessType with no word boundaries.

---

## 2. Query

`process("Hello! 你好世界 china is cool")` walks the trie, transforming and scanning at each node.

### 2.1 Prepare State

A thread-local state buffer bumps a generation counter. No arrays are zeroed — entries from previous calls have a stale generation and are ignored. This gives O(1) amortized reset regardless of rule count.

### 2.2 Walk the Trie

Each trie node is visited in flat-array order (parents before children). At each terminating node, the transformed text is scanned and hits update per-rule state.

---

**Node 0 — Root (None)**: no transformation.

```
text = "Hello! 你好世界 china is cool"
char_density = 0.72 (≥ 0.55 threshold) → bytewise engine selected
```

The bytewise AC scans the full text. No matches — `"hello"` doesn't match `"Hello"` (case-sensitive raw scan). No state updates.

---

**Node 1 — VariantNorm**: normalizes CJK variant forms (Chinese T→S, Japanese Kyūjitai→Shinjitai, half-width katakana→full-width).

`"你好世界"` is already in normalized form → no change. This is a non-terminating node, so no scan happens here.

---

**Node 2 — Delete** (child of VariantNorm): strips punctuation, symbols, and whitespace.

```
input:  "Hello! 你好世界 china is cool"
output: "Hello你好世界chinaisscool"    (char_density ≈ 0.70 → bytewise)
```

This node terminates (has rules under `VariantNorm|Delete`). The bytewise AC finds `"你好"` in the deleted text. R2 is a single-segment rule — the first hit satisfies it immediately.

---

**Node 3 — Romanize**: converts CJK to romanized form (Chinese Pinyin, Japanese Romaji, Korean Revised Romanization).

```
input:  "Hello! 你好世界 china is cool"
output: "Hello!  ni  hao  shi  jie  china is cool"    (char_density = 1.0 → bytewise)
```

This node terminates. The bytewise AC finds no match for `"zhongguo"` (the text contains `"ni hao shi jie"`, not `"zhongguo"`). No state update for R3.

---

### 2.3 Evaluate

After the tree walk, touched rules are checked for satisfaction. A rule is satisfied when all its AND segments have been matched and no NOT segment has vetoed it:

| Rule | All AND segments matched? | No NOT veto? | Result |
|------|--------------------------|-------------|--------|
| R1 | No (0 of 2 matched) | — | Not satisfied |
| R2 | Yes (1 of 1) | Yes (no NOT segments) | **Satisfied** → `{ word_id: 2, word: "你好" }` |

R3 was never touched (no hit). Final output: `[{ word_id: 2, word: "你好" }]`.

---

## 3. `is_match` Fast Path

When all rules are simple single-segment literals under one ProcessType with no word boundaries, the matcher skips the full pipeline entirely. `is_match` delegates directly to the AC automaton — a single character-density-based engine dispatch returns a boolean. No thread-local state, no generation counters, no trie walking.

All other query methods (`process`, `process_into`, `for_each_match`, `find_match`) always use the full trie walk. For simple matchers without transforms, this naturally short-circuits: the trie has only a root node, so one scan handles everything.

## 4. Batch API

`batch_is_match`, `batch_process`, and `batch_find_match` are always available. When the `rayon` feature is enabled, they distribute N texts across all CPU cores via rayon's work-stealing scheduler; without `rayon`, they run sequentially — a convenience wrapper around the single-text API.

This works without locking because `SimpleMatcher` is `Send + Sync` (all fields are read-only after construction) and all mutable scan state lives in thread-local `SIMPLE_MATCH_STATE` — each rayon worker thread gets its own independent state. The dispatch is handled by a private `maybe_par_iter!` macro that selects `par_iter()` (rayon) or `iter()` (sequential) at compile time: `maybe_par_iter!(texts).map(|t| self.method(t)).collect()`.

Throughput scales linearly with core count when `rayon` is enabled: 2.6–7.2× on M3 Max (12P + 4E cores) for typical workloads, with higher gains on CJK text (more work per line amortizes scheduling overhead).

*Source: `simple_matcher/mod.rs` (batch methods in the main `impl SimpleMatcher` block, `maybe_par_iter!` macro)*

---

## Why It's Fast

### Density-Based Engine Dispatch

**Problem:** DFA is 2–5× faster than DAAC on ASCII text, but charwise DAAC is ~1.6× faster on CJK text (1 transition per character vs 3 bytewise). No single engine wins everywhere.

**Solution:** `bytecount::num_chars` computes character density (codepoints / bytes) via SIMD. If the density is ≥ 0.55 (~40% CJK characters), the bytewise/DFA engine is used; below that, charwise wins. Both engines are built from all patterns, so either is correct for any input.

The threshold was calibrated from an 8,932-point characterization sweep across 12 pattern sizes × 11 pattern CJK compositions × 11 text CJK densities. The crossover is consistent regardless of pattern composition.

*Source: `simple_matcher/scan.rs`*

### Generation-Based State Reuse

**Problem:** Each `process()` call needs a clean per-rule state array. Zeroing N rule slots is O(N) — expensive at 100K+ rules.

**Solution:** A monotonic `u16` generation counter is bumped each call. A rule's state is "live" only when its stored generation matches the current one; stale entries are invisible. First touch initializes the slot and records its index in a touched-list. Evaluation iterates only touched rules.

Cost: O(1) amortized reset. The counter wraps at `u16::MAX` (~65K calls), triggering a bulk reset that costs ~15µs — amortized to <1ns per scan.

*Source: `simple_matcher/state.rs`*

### Direct-Rule Bypass

**Problem:** Every AC hit requires looking up the pattern's metadata (which rule, which segment, which ProcessType). The indirection through an entry table costs a cache miss on the hottest path.

**Solution:** For simple single-entry patterns (the majority), the metadata is bit-packed directly into the 32-bit AC automaton value. The scan loop checks one bit — if set, it decodes the rule index, segment offset, and operator kind inline without touching the entry table.

Falls back to the indirect table for multi-entry patterns, matrix-mode rules, or rule indices exceeding the packed capacity.

*Source: `simple_matcher/pattern.rs`*

### Fused Transform-Scan

**Problem:** The normal path materializes a transformed `String`, then scans it — allocating memory and traversing the text twice.

**Solution:** For streaming-friendly transforms (Delete, Normalize, VariantNorm, Romanize), an iterator adapter feeds transformed bytes directly into the scan loop. This eliminates the intermediate allocation and the second traversal.

A 4-way dispatch selects the strategy:

| Condition | Strategy | Rationale |
|---|---|---|
| DFA + char_density ≥ 0.55 + has Teddy prefilter | Materialize → DFA `try_find_overlapping` loop | Teddy skips non-matching regions — needs the full buffer |
| DFA + char_density ≥ 0.55 + no Teddy prefilter | Stream → DFA `next_state` loop | No prefilter to lose; custom loop eliminates iterator overhead and avoids materialization |
| No DFA + char_density ≥ 0.55 | Stream → DAAC bytewise | Best available without DFA |
| char_density < 0.55 | Stream → DAAC charwise | Charwise wins on CJK; streaming avoids allocation |

Teddy is active when the pattern count is below ~100. With `next_state`, the loop checks `is_special(sid)` (fires only for dead/match states) and falls through on most bytes with zero branching overhead.

*Source: `simple_matcher/search.rs`, `process/step.rs`*

### No-Op Scan Folding

**Problem:** When text is pure ASCII, CJK-only transforms (VariantNorm, Romanize, RomanizeChar, EmojiNorm) produce identical output. Scanning the same text N times with different ProcessType masks wastes N−1 full AC traversals.

**Solution:** Before scanning, fold no-op children's masks into the parent's scan mask. The parent scans once with the merged mask; no-op children are skipped. Correctness holds because each pattern has a fixed ProcessType index, so hits route to the right rule regardless of which scan produced them.

For a matcher with ProcessTypes {None, VariantNorm, Romanize, Delete} on ASCII text, this reduces 4 scans to 2 (root + VN + Romanize merged; Delete separate).

*Source: `simple_matcher/search.rs`*

### Split-Borrow State Access

**Problem:** Passing `&mut SimpleMatchState` through the scan loop forces the compiler to reload struct fields after each method call (pointer aliasing).

**Solution:** `ScanState` borrows individual fields as separate mutable slices. The compiler keeps base pointers in registers across the scan loop, eliminating redundant loads. Profiled: 3–6% throughput improvement.

*Source: `simple_matcher/state.rs`*

---

## Transform Pipeline

### ProcessType Bitflags

`ProcessType` is a `u8` where each bit selects a transformation step:

| Flag | Bit | Effect | Data Source |
|------|-----|--------|-------------|
| `None` | 0 | No transformation | — |
| `VariantNorm` | 1 | CJK variant normalization (Chinese T→S, Japanese Kyūjitai→Shinjitai, half-width katakana→full-width) | OpenCC + Unihan + JIS |
| `Delete` | 2 | Remove punctuation/symbols/whitespace | Unicode categories |
| `Normalize` | 3 | NFKC casefold + numeric normalization | Unicode standard |
| `Romanize` | 4 | CJK → space-separated romanization (Pinyin, Romaji, Revised Romanization) | pypinyin + kana/hangul tables |
| `RomanizeChar` | 5 | CJK → concatenated romanization (no spaces) | Same as Romanize |
| `EmojiNorm` | 6 | Emoji → English words, strips modifiers | CLDR short names |

Flags compose with `|`. Named aliases: `DeleteNormalize`, `VariantNormDeleteNormalize`.

**Gotcha:** `EmojiNorm | Delete` doesn't work — Delete removes emoji before EmojiNorm sees them. Use `EmojiNorm | Normalize` instead.

### Page-Table Lookup

VariantNorm, Romanize, EmojiNorm, and Normalize use a two-stage page table for O(1) codepoint lookup. The first stage indexes by `codepoint >> 8` (which 256-codepoint block); the second stage indexes within the block. Unmapped codepoints are passed through unchanged. Both stages use `get_unchecked` for branchless access.

Romanize and EmojiNorm string buffers store each replacement with a **build-time-prepended leading space** for word boundary separation (source data in `process_map/` is space-free). `RomanizeChar` trims this space at runtime via `trim_romanize_packed`.

*Source: `process/transform/page_table.rs`*

### SIMD Skip Functions

Transform iterators use SIMD to skip irrelevant ASCII byte runs:

| Transform | Skip Function | What It Skips |
|-----------|--------------|---------------|
| VariantNorm, Romanize | `skip_ascii_simd` | All ASCII bytes (only CJK keys exist) |
| Delete | `skip_ascii_non_delete_simd` | ASCII bytes not in the delete bitset |

Dispatch: AVX2 on x86-64 (runtime detection), NEON on AArch64 (compile-time), portable `std::simd` fallback.

*Source: `process/transform/simd.rs`*

### Compiled Tables

`build.rs` compiles transformation data from source files in `process_map/` into binary artifacts embedded via `include_bytes!`. At runtime, page tables are decoded lazily on first access.

---

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `perf` | on | Meta-feature enabling `dfa` + `simd_runtime_dispatch` |
| `dfa` | via `perf` | Aho-Corasick DFA for bytewise engine. ~17× more memory, ~1.7–3.3× faster. |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD kernel selection for transforms (AVX2/NEON) and `bytecount` character density (NEON/AVX2) |
| `rayon` | off | Enables parallel execution for batch methods (always available; sequential without this feature). Enabled by binding crates. |
