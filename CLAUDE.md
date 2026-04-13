# Project

High-performance multi-language word/text matcher in Rust with Python, C, and Java bindings. Solves precision/recall problems in pattern matching via logical operators (`&`/`~`/`|`/`\b`) and text normalization pipelines.

**Toolchain:** nightly Rust (see `rust-toolchain.toml`), edition 2024. Nightly is required — do not change this. Edition 2024 has behavioral changes: `gen` is reserved, `unsafe` blocks required inside `unsafe fn`, etc.

**Prerequisites:** `just` (command runner), `cargo-all-features` (`cargo install cargo-all-features`), `cargo-nextest` (`cargo install cargo-nextest`), `uv` (Python env manager), `prek` (pre-commit runner).

## Commands

All commands run from the workspace root via `just` — no `cd` needed.

```bash
# Build
just build                          # Full workspace + copy bindings artifacts
just update                         # cargo update --breaking + cargo upgrade

# Quick iteration
just check                          # Fast type-check (no codegen)
just fmt                            # Auto-format
just fmt-check                      # Check formatting without modifying

# Test (test-rs and test-quick accept pass-through args)
just test                           # All languages (Rust + Python + Java + C)
just test-rs                        # All feature combos + doctests
just test-quick                     # Default-features tests only
just test-quick test_name           # Single test by name (substring match)
just test-quick --no-default-features
just test-quick --test test_engine  # Single test file by name
just test-py                        # Python bindings
just test-java                      # Java bindings
just test-c                         # C bindings

# Lint
just lint                           # All languages + workspace clippy + doc build
just lint-check                     # Check-only (no auto-fix) — used in CI

# Benchmarks (harness: divan, orchestration: scripts/run_benchmarks.py)
# Bench targets: bench_search (throughput), bench_transform (transforms), bench_build (construction)
# Pass-through args: --quick, --filter <pattern>, --repeats N, --profile <name>
just bench-search                   # Search + transform presets (full run)
just bench-search --quick           # Quick directional signal
just bench-search --filter scaling  # Only scaling benchmarks
just bench-search --filter rule_complexity
just bench-search --filter "scaling::process_cn"       # Single benchmark group
just bench-search --profile bench-dev                  # Faster rebuild (thin LTO)
just bench-build                    # Construction benchmarks
just bench-all                      # All presets (search + build)
just bench-compare <baseline> <candidate>              # Compare runs, dirs, or raw files
just bench-viz <run_dir>                               # Interactive HTML dashboard (Plotly)
just bench-viz <baseline_dir> <candidate_dir>          # Comparison visualization

# Profiling (macOS Instruments Time Profiler)
just profile record --scene en-search --analyze        # Record + auto-analyze
just profile record --scene all --seconds 5            # All scenes, 5s each
just profile record --target build --dict cn --rules 50000 --analyze
just profile analyze scripts/profile_records/prof_*.trace   # Analyze existing trace

# Coverage
just coverage                       # cargo tarpaulin → cobertura.xml

# Release
scripts/bump-version.sh <version>   # Update version in all manifests + CHANGELOG
```

**Pre-commit:** `.pre-commit-config.yaml` exists — run `prek run` before committing.

**Cargo profiles:** `bench` (full LTO + debug symbols — authoritative measurements via `run_benchmarks.py`), `bench-dev` (thin LTO + incremental — faster rebuild for iterative bench development), `profiling` (release + debug symbols — for `instruments`/`perf`/`samply`).

## Architecture

For the full narrative walkthrough with a running example, see [DESIGN.md](./DESIGN.md). Below is the essential mental model.

### How a Query Works

1. **Transform** — Walk a shared-prefix trie of `ProcessType` steps, producing text variants (VariantNorm, Delete, Normalize, Romanize, RomanizeChar, EmojiNorm). Intermediate results are reused across combinations.
2. **Scan** — Each variant is scanned by a single deduplicated Aho-Corasick automaton (bytewise or charwise, selected by character density via `bytecount::num_chars` at threshold 0.55). Hits update per-rule state.
3. **Evaluate** — Touched rules are checked: all AND segments satisfied + no NOT veto → match.
4. **`is_match` fast path** — When no text transforms are needed and all rules are simple literals without boundaries, `is_match` delegates directly to the AC automaton without TLS state setup.

### Key Concepts

- **ProcessType**: `u8` bitflags composable with `|`. Controls which transforms are applied before matching. `None` is standalone-only (stripped from composites via `normalize()`).
- **Transform trie**: shared-prefix DAG so `VariantNorm|Delete` reuses the VariantNorm result.
- **ScanPlan**: `Engines` struct bundling `BytewiseMatcher` (holds `BytewiseDFAEngine` + DAAC) and `CharwiseMatcher` (DAAC, CJK-optimized). `BytewiseDFAEngine` owns `dfa::DFA`, `dfa_to_value`, and `has_prefilter`; the `has_prefilter` flag drives a 4-way fused-path dispatch: Teddy prefilter active → materialize + `try_find_overlapping`; no prefilter → stream via custom `next_state` loop. Engine selection via character density (`bytecount::num_chars / len`): ≥0.55 → bytewise, <0.55 → charwise. Unified behind `ScanEngine` trait, dispatched via `dispatch!` macro.
- **RuleSet**: `Rule` stores cold data (`segment_counts` + `word_id` + `word`); `RuleInfo` stores hot data (`and_count`, `SatisfactionMethod`, `has_not`). All hits routed through unified `eval_hit()`. Generation-stamped sparse set for O(1) state reset.
- **DIRECT_RULE_BIT**: single-entry non-matrix patterns encode `(kind, pt_index, boundary, offset, rule_idx)` in one uniform 32-bit layout (bit 31 set), skipping the entry table. Decoded directly into `eval_hit()` args.

### Construction subtlety: Delete and AC pattern indexing

During `SimpleMatcher::new`, each sub-pattern is indexed under `process_type - ProcessType::Delete` rather than the full `ProcessType`. Delete is the only non-bijective transform — patterns are stored verbatim (not delete-transformed). The AC automaton scans **both** the original text and the delete-transformed text (dual scan), because patterns may contain deletable characters that only exist in the original. When Delete is a direct child of root in the transform trie, `build_process_type_tree` propagates the mask bit to root to enable this dual scan.

**`None` in composites:** `ProcessType::None` is only meaningful standalone. Combining it with any transform is redundant — the `None` bit is silently stripped during construction via `ProcessType::normalize()`.

### Feature Flags

| Flag | Default | Notes |
|------|---------|-------|
| `perf` | on | Meta-feature enabling `dfa + simd_runtime_dispatch` |
| `dfa` | via `perf` | Aho-Corasick DFA — 1.7–3.3× faster than DAAC; ~17× more memory |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD dispatch for transforms (AVX2/NEON/portable) and `bytecount` character density (NEON/AVX2) |
| `rayon` | off | Enables parallel execution for batch methods (`batch_is_match`, `batch_process`, `batch_find_match`). Without this feature, batch methods still work but run sequentially. Enabled by all binding crates. |

**Note:** `EmojiNorm` (bit 6) maps emoji to English words via CLDR short names. Does NOT compose usefully with `Delete` — Delete removes emoji before EmojiNorm sees them. Use `EmojiNorm | Normalize` for emoji→word matching.

### Workspace Layout

- `matcher_rs/` — Core library (`rlib`); all algorithms live here
- `matcher_py/` — Python bindings via PyO3
- `matcher_c/` — C FFI bindings (`cdylib`)
- `matcher_java/` — Java JNI bindings (`cdylib` + Maven)

### Key Source Files

**`matcher_rs/src/simple_matcher/`** — Core matching engine (directory module). `SimpleMatcher` stores: `tree` (transform trie), `scan` (`ScanPlan`), `rules` (`RuleSet`), `is_match_fast` (AC-direct bypass flag).
- `mod.rs` — `SimpleMatcher`, `SimpleResult`, public API (`is_match`, `process`, `process_into`, `for_each_match`, `find_match`; `batch_is_match`, `batch_process`, `batch_find_match` always available, parallel when `rayon` is on)
- `build.rs` — `SimpleMatcher::new()` + helpers (`build_pt_index_table`, `parse_rules`), `ParsedRules` intermediate representation
- `scan.rs` — `ScanPlan`, `Engines`, `ScanEngine` trait, `BytewiseMatcher` (holds `BytewiseDFAEngine` + DAAC), `BytewiseDFAEngine` (owns `dfa::DFA`, `dfa_to_value`, `has_prefilter`; all DFA scan logic), `CharwiseMatcher` (DAAC charwise), `dispatch!` macro, `text_char_density` (via `bytecount::num_chars`) — AC automaton compilation, density-based dispatch, prefilter-aware scan (Teddy → materialize+`try_find_overlapping`; no prefilter → stream via `next_state` loop)
- `pattern.rs` — `PatternEntry`, `PatternKind`, `PatternIndex`, `PatternDispatch` — deduplicated pattern storage and dispatch. Also contains direct-rule bit-packing (`encode_direct`/`decode_direct`, `DIRECT_RULE_BIT`), capacity limits (`BITMASK_CAPACITY`, `PROCESS_TYPE_TABLE_SIZE`)
- `rule.rs` — `RuleSet`, `Rule` (cold: `segment_counts` + `word_id` + `word`), `RuleInfo` (hot: `and_count` + `SatisfactionMethod` + `has_not`), unified `eval_hit()`, `SimpleTable`/`SimpleTableSerde` type aliases
- `search.rs` — Hot-path: `walk_and_scan`/`walk_and_scan_with` (unified tree walk with materialize+scan), `scan_variant`, `process_match`
- `state.rs` — `RuleState` (fused per-rule state: generation + countdown + veto + bitmask in one cache line), `SimpleMatchState`, `ScanState` (split-borrow view for register-cached base pointers), `ScanContext`, TLS `SIMPLE_MATCH_STATE`, generation-based state reset
- `tree.rs` — `ProcessTypeBitNode`, `build_process_type_tree` (trie construction for transform prefix sharing)

**`matcher_rs/src/process/`** — Text normalization pipeline:
- `mod.rs` — `ProcessType` re-export + public helpers: `text_process`, `reduce_text_process`, `reduce_text_process_emit`
- `process_type.rs` — `ProcessType` bitflags + serde/display
- `step.rs` — `TransformStep` enum, `TRANSFORM_STEP_CACHE`, `get_transform_step` — uniform apply interface + lazy per-process init
- `transform/page_table.rs` — Shared two-stage page-table lookup infrastructure for replacement engines
- `transform/variant_norm.rs` — `VariantNormMatcher` (CJK variant normalization)
- `transform/normalize.rs` — `NormalizeMatcher` (Unicode NFKC + casefolding)
- `transform/romanize.rs` — `RomanizeMatcher` (CJK romanization)
- `transform/delete.rs` — `DeleteMatcher` (flat BitSet + ASCII LUT + SIMD bulk-skip), `DeleteFilterIterator` (streaming)
- `transform/utf8.rs` — Shared `decode_utf8_raw` unsafe helper
- `transform/simd.rs` — SIMD helpers: `skip_ascii_simd`, `skip_ascii_non_delete_simd`
- `transform/constants.rs` — Precompiled transformation tables (generated by `build.rs`)

**Other:**
- `matcher_rs/src/builder.rs` — `SimpleMatcherBuilder` fluent API
- `matcher_rs/process_map/` — Source text files (`VARIANT_NORM.txt`, `ROMANIZE.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`, `EMOJI_NORM.txt`) consumed by `build.rs`

## Important Notes

- ALWAYS profile before AND after optimizations to validate the mechanism, then bench-compare for the final adopt/revert decision. Profile comparison is fast (~30s) and shows whether the target category % actually changed; bench-compare is slow (~15 min) but is the authoritative throughput measurement.
- ALWAYS update `DESIGN.md` after making any non-trivial code changes to keep documentation accurate.
- Benchmarks use `divan` (not `criterion`) — write new benchmarks with `#[divan::bench]` attributes.
- `proptest` is available for property-based testing in `matcher_rs`.
- With heavy `#[inline(always)]` + full LTO, LLVM applies CSE across function boundaries. Source-level "redundancy" (e.g., duplicate `text.is_ascii()` calls) may already be a single operation in generated code. Profile category % is the ground truth, not source reading.
