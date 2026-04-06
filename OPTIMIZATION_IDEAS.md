# Optimization Ideas

Baseline profiles captured 2026-04-06 on Apple Silicon (M-series), 10K rules unless noted.
Traces: `/tmp/prof_{en-search,cn-search,cn-transform,en-and,en-large}.trace`

## Baseline Category Breakdown

| Category | en-search | cn-search | cn-transform | en-and | en-large (50K) |
|----------|-----------|-----------|--------------|--------|----------------|
| DFA scan | **73.4%** | 0.1% | 0.2% | **61.6%** | **72.5%** |
| Daachorse (charwise) | 0.2% | **80.2%** | **73.9%** | 0.6% | 1.0% |
| Engine dispatch / rule eval | 11.6% | 0.5% | 0.2% | **32.6%** | **19.5%** |
| Search hot path | 14.7% | 18.9% | 25.5% | 3.5% | 6.6% |

Note: en-search/en-and/en-large numbers updated after opt #1 (and_count in PatternEntry)
and profiling category fix (AC normalize scan → DFA scan).

## Candidates

### 1. Rule evaluation in AND patterns — 34.1% of en-and

The per-hit callback path (`BytewiseMatcher::for_each_match_value`) accounts for 35.4% of
total time. Within that, rule evaluation dominates:

| Symbol | % | Source |
|--------|---|--------|
| `RuleShape::use_matrix` | 14.0% | rule.rs:117 — branch gating matrix vs bitmask path |
| `RawVecInner::non_null<RuleHot>` | 8.7% | Vec pointer chase into `self.hot` on first-touch init |
| `RuleSet::process_entry` body | 5.2% | rule.rs — state transitions |
| `scan_variant::{closure#0}` | 3.1% | search.rs:125 — closure dispatch overhead |
| `process_match` | 1.4% | search.rs:163 — pattern dispatch |

**Root cause:** On first touch per rule per generation, `process_entry` (rule.rs:337-356)
loads `self.hot[rule_idx]` to read `and_count` and `use_matrix`. With 10K AND rules, this
is ~400KB of `RuleHot` structs (each ~40 bytes due to inner `Vec<i32>`). Random access
into this array causes L1 cache misses. The fields read are:
- `rule.and_count` — needed to init `word_state.remaining_and`
- `rule.use_matrix` — to decide whether to call `init_matrix` (cold path)

**Proposed fix: Store `and_count` in PatternEntry**

`PatternEntry` is 9 bytes padded to 12 (3 bytes wasted). Adding `and_count: u8` costs 0
extra bytes. Since `shape.use_matrix()` already derives `use_matrix` from the entry, the
init block would only need `RuleHot` for matrix-mode rules (`#[cold]` path). Non-matrix
rules (vast majority) skip the `self.hot` load entirely.

Expected mechanism: eliminate 8.7% pointer-chase cost for bitmask/single-and rules.
May also reduce branch pressure on `use_matrix()` (14%) since the init block becomes
smaller and more predictable.

**Profile result:** `RawVecInner::non_null<RuleHot>` dropped **8.7% → 0.9%** (-7.8pp).
DFA scan share rose 49.1% → 51.3% (more headroom). Mechanism confirmed.

Also removed `and_count` and `use_matrix` fields from `RuleHot` entirely — they were
only needed in `process_entry` init, which now reads from `PatternEntry` + `RuleShape`.
`RuleHot` shrank from ~40 bytes to ~24 bytes (just `segment_counts: Vec<i32>`).

**Benchmark result (rule_complexity filter, 3 repeats, full bench profile):**

| Benchmark | Baseline | Candidate | Delta |
|-----------|----------|-----------|-------|
| shape_process / and | 9.223 ms | 8.654 ms | **-6.17%** |
| shape_is_match / and | 9.005 ms | 8.614 ms | **-4.34%** |
| shape_is_match / literal | 539.8 µs | 495.1 µs | **-8.28%** |
| shape_process / not | 4.535 ms | 4.342 ms | **-4.26%** |
| shape_is_match / not | 4.090 ms | 3.921 ms | **-4.13%** |
| shape_is_match / or | 515.3 µs | 494.0 µs | **-4.13%** |

Zero regressions. The literal is_match improvement (-8.28%) is a bonus from `RuleHot`
shrinking — smaller struct improves overall cache behavior.

**Status:** adopted (cd3cdea)

### 2. AllSimple closure overhead at scale — 19.8% of en-large

`process_simple` closure (mark_positive_simple + cold access + Vec push) grows with rule
count. At 50K rules the per-hit callback is ~20% of total.

**Attempted: Two-phase scan/collect split**

Split `process_simple` into phase 1 (DFA scan + dedup, no cold access) and phase 2 (build
results from cold array). Hypothesis: removing cold[] random access from the DFA scan loop
reduces cache pollution.

Profile confirmed mechanism: closure dropped 19.8% → 14.1%, cold access moved to phase 2.
But benchmark showed a crossover: helps at 50K+ rules (-4.6%), **regresses at ≤10K (+6.6%
to +20.1%)**. At small rule counts, the cold array fits in cache and the extra Vec<usize>
allocation + phase 2 loop is pure overhead.

| Benchmark | Baseline | Candidate | Delta |
|-----------|----------|-----------|-------|
| process_en / 1000 | 1.327 ms | 1.594 ms | **+20.12%** |
| process_en / 10000 | 3.569 ms | 3.804 ms | **+6.58%** |
| process_en / 50000 | 8.566 ms | 8.174 ms | -4.58% |
| process_en / 100000 | 16.520 ms | 15.660 ms | -5.21% |

**Status:** reverted — regression at common sizes outweighs gain at extreme scale

### 3. Charwise iterator overhead — 82.5% of cn-search

Reprofiled with corrected categories. Daachorse charwise automaton is 82.5% of cn-search.
Our callback closure is only 1.5%. Full breakdown of daachorse internals:

| Symbol | % |
|--------|---|
| `next_state_id_unchecked` (state transition) | 31.9% |
| `State]::get_unchecked` (array index) | 13.8% |
| `FindOverlappingIterator::next` (control flow) | 15.0% |
| `StrIterator::next` (UTF-8 decode) | 6.5% |
| `State::output_pos/check/base` | 11.4% |
| `[u8]::get` + `u32::from` | 7.1% |

This is entirely inside the daachorse library. No optimization surface in our code.
Possible future directions (all require library changes or replacement):
- Contribute SIMD-accelerated UTF-8 → char iteration to daachorse
- Investigate whether aho-corasick crate's DFA mode works for CJK (it currently doesn't
  have a charwise mode, but CJK chars are 3 UTF-8 bytes → 3 DFA transitions vs 1 charwise)

**Status:** not actionable — bottleneck is in external library

---

## New Candidates (from cn-transform reprofiling)

cn-transform (VariantNorm+Delete+Normalize, 10K CJK rules): 73.1% daachorse, **26.1% our
transform code**. Corrected-category breakdown of our code:

| Hot spot | % of total | Source |
|----------|-----------|--------|
| NormalizeFilterIterator::next | 15.2% | normalize.rs (lines 94, 108, 133, 141) |
| Enumerate wrapper overhead | 8.3% | enumerate.rs wrapping NormalizeFilterIterator |
| page_table_lookup | 6.9% | mod.rs (two-level page table) |
| utf8::decode_utf8_raw | 3.4% | utf8.rs (lead-byte branching) |
| DeleteMatcher::delete | 3.3% | delete.rs (seek + copy-skip) |
| skip_ascii_neon | 1.3% | simd.rs (NEON ASCII fast-forward) |

### 4. NormalizeFilterIterator per-byte branch overhead — 15.2%

**Initial hypothesis (ASCII bulk-skip) was wrong:** NormalizeFilterIterator already gates
`page_table_lookup` behind `is_ascii_uppercase()` — it does NOT call the page table on
every byte. The cost is the three-level branch structure in `next()`:

```
line 94:  if replace_pos < replace_bytes.len()  → yield replacement byte
line 100: if char_remaining > 0                  → yield continuation byte
line 108: if offset >= bytes.len()               → return None
```

Every byte pays 2-3 branch checks. For CJK (3 bytes/char): lead byte hits all 3, continuation
bytes hit 2. For ASCII: 3 checks + `is_ascii_uppercase`.

**Proposed fix: Unified remainder slice.** Replace `replace_bytes`/`replace_pos` (24 bytes)
and `char_remaining` (1 byte) with a single `remaining: &[u8]` (16 bytes). The fast path
becomes one `split_first()` check instead of two separate branch chains. Saves 9 bytes per
struct and reduces branch count from 3 to 2 on the hot path.

**Fix: Unified remainder slice.** Replaced `replace_bytes`/`replace_pos`/`char_remaining`
with a single `remaining: &[u8]` slice. Profile confirmed mechanism: NormalizeFilterIterator
dropped 15.2% → 8.2% (-7pp). Benchmark showed no measurable throughput change because the
charwise automaton (74.6%) is the pipeline bottleneck — but the optimization reduces per-byte
CPU work and struct size, leaving more headroom when the automaton isn't the limiting factor.

**Status:** adopted (reduces branch overhead; throughput-neutral due to automaton bottleneck)

### 5. ~~Branchless UTF-8 sequence length — 3.4%~~

**Attempted: CLZ-based length detection.** Replaced two threshold comparisons (`b0 < 0xE0`,
`b0 < 0xF0`) with `(!b0).leading_zeros()` + integer equality checks. Profile showed
decode cost dropped 4.2% → 2.9% (-1.3pp). But benchmark **regressed** CJK transforms:
variant_norm +9.4%, delete +3.8%, romanize_char +3.9%.

**Why it failed:** For CJK text (99%+ 3-byte chars), the original `b0 < 0xE0` branch is
perfectly predicted by the CPU (almost always false). CLZ adds latency before the branch
instead of just doing the comparison. The branch predictor was already optimal; CLZ added
overhead on the critical path.

**Status:** reverted — branch predictor beats branchless on uniform CJK workloads

### ~~6. DeleteMatcher lazy allocation — 3.3%~~

**Profile disproves hypothesis.** Reprofiled: string pool allocation (`get_string_from_pool`)
doesn't appear in samples at all — it's either too fast or the pool is warmed up. The 3.3%
DeleteMatcher cost is entirely in the seek/copy-skip loops: UTF-8 decode (2.5%), bitset
check (2.5%), byte load (0.7%). No optimization surface for allocation deferral.

Note: `delete()` already returns `None` (zero allocation) when no deletable codepoint is
found — the seek phase exits early before allocating.

**Status:** not actionable — allocation cost is negligible, bottleneck is in core loop

---

## Round 3: Fresh Target Survey (post-opt #1 and #4)

Full benchmark landscape (quick run, 10K rules unless noted):

| Benchmark | Time | Bottleneck |
|-----------|------|-----------|
| search_mode / general / process | **16.6 ms** | DFA 71.5% + callback 23.3% |
| scaling / process_en / 100K | 16.4 ms | DFA (AllSimple, scale) |
| scaling / process_cn / 100K | 15.0 ms | Daachorse (charwise, scale) |
| text_transform / cn / romanize | **10.3 ms** | DFA 46.8% + **memmove 9.3%** |
| text_transform / cn / romanize_char | 9.7 ms | Similar to romanize |
| rule_complexity / shape_process / and | 9.1 ms | DFA 51.3% + rule eval 32.6% |
| rule_complexity / shape_process / or | 8.7 ms | DFA 71.5% + mark_positive 7.5% |

Key asymmetries:
- **is_match vs process (AllSimple)**: 491 µs vs 3.67 ms = 7.5× gap
- **is_match vs process (General)**: 1.3 ms vs 16.6 ms = 12.8× gap

### 7. Romanize materialization cost — 9.3% memmove in cn-romanize

cn-romanize profile shows 9.3% in `_platform_memmove`, the highest of any scene. Romanized
CJK text is much longer than the original (e.g., "中国" → "zhongguo", 6 bytes → 8 bytes).
The `replace_spans` path does repeated `push_str` calls that trigger String reallocation.
Also 1.1% in `str::is_char_boundary` from string slicing.

**Attempted: Pre-allocate with expansion factor (text.len() * 3).** Added `capacity`
parameter to `replace_spans`. Profile backtrace revealed the 9.3% memmove is from
`copy_nonoverlapping` (push_str data copy), NOT from `grow_amortized` (reallocation) —
realloc was only 0.0%. The memmove is **inherent to materialization**: you must copy bytes
to build the output string.

Benchmark confirmed: no improvement on romanize, slight regression on normalize (+3.1%).

Remaining approach: fused romanize-scan (avoid materialization entirely). But romanize
has no `filter_bytes` iterator — it would need one to support streaming. The expansion
from 3-byte CJK to multi-byte ASCII pinyin makes a byte-level streaming iterator
complex (one input char → many output bytes, with spaces between syllables).

**Status:** reverted — memmove is inherent data copy, not reallocation

### ~~8. OR-rule mark_positive overhead — 7.5% in en-or~~

**Investigated.** The en-or benchmark stresses worst-case cross-rule pattern sharing (each
word in 3 rules, creating len=3 dedup buckets). In real usage, unique OR alternatives
already get DIRECT_RULE_BIT and AllSimple mode.

The 7.5% mark_positive is inherent to overlapping AC scan: 3 OR alternatives per rule
means 3 hits per matched rule. Each hit checks `word_states[rule_idx]` generation stamp
(one comparison, ~260KB random access into L2 cache). No structural optimization available
without changing AC automaton behavior. The dedup check is already minimal.

**Status:** not actionable — inherent to overlapping scan, benchmark-specific worst case

### ~~9. is_match → process gap — 7.5× for AllSimple~~

**Attempted: Early-exit when all rules resolved.** Added `-> bool` return to
`for_each_rule_idx_simple` callback, stopping the DFA scan when `results.len() >= num_rules`.
Benchmark: no improvement, CJK regression (+3.2%).

**Why it failed:** With 10K-100K dictionary words and Sherlock text, only a fraction of
rules match — `results.len()` never reaches `num_rules`. The DFA scans the entire text
regardless. The extra `>= num_rules` comparison per callback adds overhead on every hit.

The 7.5× gap between is_match and process is inherent: is_match uses the native
`ac.is_match()` (non-overlapping early-exit on first match), while process must use
`find_overlapping_iter()` to report every match position for deduplication.

**Status:** reverted — early-exit condition rarely triggers in practice

---

## Round 4: Remaining Optimization Surface

Post-optimization code % across all scenes:

| Scene | External (DFA/Daachorse) | Our Code | Notes |
|-------|-------------------------|----------|-------|
| cn-romanize | 46.2% | **53.8%** | Only scene where we're the bottleneck |
| en-and | 63.8% | 36.2% | Rule eval, already optimized (opt #1) |
| en-large | 70.3% | 29.7% | Callback overhead, inherent |
| en-or | 71.7% | 28.3% | Inherent overlapping scan |
| cn-transform | 75.0% | 25.0% | Transform pipeline, already optimized (opt #4) |
| en-search | 79.4% | 20.6% | Callback + result collection |
| en-boundary | 79.8% | 20.2% | 11.1% in boundary checking |
| cn-search | 82.8% | 17.2% | Almost entirely daachorse |

### 10. Fused romanize-scan path — 53.8% our code in cn-romanize

cn-romanize is the only scene where our code is the bottleneck. Romanize is the only
transform without a `filter_bytes` streaming iterator — it always materializes the full
output String (with 9.3% memmove), then scans the materialized text separately.

Delete, Normalize, and VariantNorm all have fused paths that stream transformed bytes
directly into the charwise/bytewise automaton, avoiding materialization entirely. Adding
a `RomanizeFilterIterator` would eliminate:
- String allocation + pool overhead
- 9.3% memmove from push_str data copies
- Separate DFA scan pass on the materialized text

Challenge: romanize expands 3-byte CJK chars into variable-length ASCII strings (e.g.,
"中" → " zhong "), so the streaming iterator must yield multi-byte replacement sequences
one byte at a time — similar to NormalizeFilterIterator's replacement handling.

**Fix: Added `RomanizeFilterIterator` + hooked into fused scan dispatch.** Profile:
our-code share dropped 53.8% → 11%, memmove eliminated. Bench: cn/romanize **-11.2%**,
cn/romanize_char **-11.0%**, zero regressions.

Also rewrote `text_transform` benchmark to use full `SimpleMatcher::process()` pipeline
instead of standalone `text_process()`, so it correctly measures fused paths.

**Status:** adopted (87fde7f)

### ~~11. Build time optimization — 280ms at 100K rules~~

**Profiled.** Added `profile_build` example and `--target build` support to the profiler.
Build at 10K rules (24ms):

| Category | % |
|----------|---|
| aho-corasick DFA build | 43.3% |
| daachorse DAAC build | 28.7% |
| Our code (pattern dedup + rule parsing) | 22.2% |
| Allocator | 3.9% |

72% of build time is in external library automaton compilation. Our 22% is rule parsing
and pattern deduplication — limited optimization surface without forking the libraries.
The two automata already build in parallel threads.

**Status:** not actionable — bottleneck is in external library automaton builders


### 12. Word boundary lookup table — 11.1% of en-boundary

`check_word_boundary` (9.9%) + `is_word_byte` (1.2%) = 11.1% of the en-boundary scene.
`is_word_byte` does three checks: `is_ascii_alphanumeric() || b == b'_' || b >= 0x80`.
A 256-byte lookup table would replace this with a single indexed load.

**Fix: 256-byte WORD_BYTE_LUT + unchecked indexing.** Replaced `is_word_byte`'s 5 per-byte
comparisons with a single LUT load, inlined directly into `check_word_boundary`. Also
replaced checked `[]` indexing with `get_unchecked` after explicit bounds guards.

Bench: word_boundary is_match **-5.5%**, zero regressions.

**Status:** adopted (18e1703)

---

## Future Directions

Current ceiling: 63-89% of search time is in external AC libraries (aho-corasick DFA,
daachorse charwise). Our code is lean. Platform: Apple Silicon (no AVX-512).
Hyperscan/Vectorscan tested and ruled out (Vectorscan slower than current on ARM).

### A. Overlap-free pattern detection → non-overlapping scan

**Benchmarked: `find_iter` vs `find_overlapping_iter` across all 3 engines:**

| Engine | Lang | 1K | 10K | 50K | 100K |
|--------|------|-----|------|------|------|
| **aho-corasick DFA** | EN | -0.4% | +0.7% | **-16.8%** | **-22.3%** |
| **DAAC bytewise** | EN | -0.4% | **-12.2%** | **-36.9%** | **-58.7%** |
| **DAAC charwise** | CN | -1.3% | +1.0% | **-9.7%** | **-22.8%** |

DAAC bytewise shows the biggest gap: **-12% at 10K, -59% at 100K**. DFA has Teddy prefilter
which masks the difference at small scale. DAAC charwise benefits at 50K+.

**Correctness constraint**: Non-overlapping scan can miss patterns that overlap with
other patterns. Example: "he" and "hello" — `find_iter` finds "he" at position 0, skips
to position 2, misses "hello". For AllSimple `process` (which needs ALL matching rules),
this is incorrect unless the pattern set is **overlap-free**: no pattern is a substring of
or overlaps with any other pattern.

**Detection approach**: At build time, use the AC automaton to scan each pattern. If any
pattern produces a match for a DIFFERENT pattern, there's an overlap. O(total_pattern_bytes)
one-time cost. Store `overlap_free: bool` in ScanPlan; use `find_iter` when true.

**Applicability**: Content moderation keyword lists often have overlapping patterns (e.g.,
"sex" ⊂ "sexual"). The optimization only applies to overlap-free sets. But the 12-59%
speedup for qualifying workloads is substantial.

**Status**: benchmarked, correctness analysis complete, implementation pending

### B. Bloom filter prescreen for `is_match`

At build time, compute a Bloom filter from n-gram prefixes of all patterns. At `is_match`
time, hash n-grams of the text against the Bloom filter. If no hits → return false
immediately (no DFA scan). Only helps sparse-match scenarios but could give 5-30× for
the common "no match" case.

Reference: CMU feed-forward Bloom filter paper (2-30× speedup for exact matching).