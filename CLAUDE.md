# Project

High-performance multi-language word/text matcher in Rust with Python, C, and Java bindings. Solves precision/recall problems in pattern matching via logical operators (`&`/`~`) and text normalization pipelines.

**Toolchain:** nightly Rust (see `rust-toolchain.toml`), edition 2024. Nightly is required — do not change this. Edition 2024 has behavioral changes: `gen` is reserved, `unsafe` blocks required inside `unsafe fn`, etc.

**Prerequisites:** `just` (command runner), `cargo-all-features` (`cargo install cargo-all-features`), `cargo-nextest` (`cargo install cargo-nextest`), `uv` (Python env manager), `prek` (pre-commit runner).

## Commands

```bash
# Build
just build                          # Full workspace + copy bindings artifacts
cargo build --release               # Rust only

# Quick iteration
just check                          # Fast type-check (no codegen)
just test-quick                     # Default-features tests only
just fmt                            # Auto-format
just fmt-check                      # Check formatting without modifying

# Test
just test                           # All languages (Rust + Python + Java + C)
just test-rs                        # All feature combos + doctests + docs
just test-py                        # Python bindings
just test-java                      # Java bindings
just test-c                         # C bindings
cd matcher_rs && cargo nextest run <test_name>                  # Single test by name
cd matcher_rs && cargo nextest run --no-default-features        # Without DFA
cd matcher_rs && cargo nextest run --test test_engine           # Single test file by name

# Lint/Format
just lint                           # All languages (rs + py + java)
just lint-rs                        # cargo fmt + cargo clippy (matcher_rs)
just lint-py                        # cargo fmt + cargo clippy + ruff + ty check (matcher_py)
just lint-java                      # cargo fmt + cargo clippy + mvn checkstyle (matcher_java)
just lint-c                         # cargo fmt + cargo clippy (matcher_c)

# Benchmarks (harness: divan, two targets: bench, bench_engine)
# All bench recipes accept pass-through args: --quick, --profile, --repeats, etc.
just bench-search                          # Main throughput workflow
just bench-search --quick                  # Quick directional signal (~2-3 min)
just bench-search --profile bench-dev      # Faster rebuild (thin LTO)
just bench-build                           # Matcher construction workflow
just bench-engine-search                   # Raw engine throughput workflow
just bench-engine-build                    # Raw engine build workflow
just bench-engine-is-match                 # Engine is_match (Harry) workflow
just bench-all                             # All presets
just bench-compare <baseline_dir> <candidate_dir>      # aggregated run-set comparison
just bench-compare-raw <baseline.txt> <candidate.txt>  # raw file-to-file comparison
just bench-viz <run_dir>                               # interactive HTML dashboard (Plotly)
just bench-viz <baseline_dir> <candidate_dir>          # comparison visualization

# Profiling (uses release + debug symbols)
cd matcher_rs && cargo build --profile profiling

# Coverage
just coverage                       # cargo tarpaulin → matcher_rs/tarpaulin-report.html

# Dependency updates
just update                         # cargo update --breaking + cargo upgrade

# Release
scripts/bump-version.sh <version>   # Update version in all manifests + CHANGELOG
```

**Pre-commit:** `.pre-commit-config.yaml` exists — run `prek run` before committing.

**Cargo profiles:** `bench` (full LTO + debug symbols — authoritative measurements via `run_benchmarks.py`), `bench-dev` (thin LTO + incremental — faster rebuild for iterative bench development), `profiling` (release + debug symbols — for `instruments`/`perf`/`samply`).

## Architecture

For exhaustive internal documentation, see [DESIGN.md](./DESIGN.md). Below is the essential mental model.

### Two-Pass Matching

1. **Pattern Scan** — All unique sub-patterns across all rules are deduplicated and compiled into a single automaton (Aho-Corasick via `daachorse`, optional `dfa`). O(N) text scan. ASCII-only patterns get a separate engine for fast dispatch when input is ASCII.
2. **Logical Evaluation** — Only rules that had ≥1 hit in Pass 1 are evaluated. Sparse-set via generation IDs for O(1) state reset. Bitmask fast-path for rules with ≤64 segments; matrix fallback otherwise. `SearchMode::AllSimple` fast-path bypasses all state machinery for pure-literal matchers.

### Text Transformation Pipeline

Before matching, text is transformed through a DAG of `ProcessType` steps (bitflags composable with `|`):

```
None | Fanjian | Delete | Normalize | DeleteNormalize | FanjianDeleteNormalize | PinYin | PinYinChar
```

The DAG is a Trie — intermediate results are reused across combinations. Transformations use `Cow<'_, str>` to avoid allocations when no change occurs. Transformation tables are compiled at build time (`build.rs`) from source files in `matcher_rs/process_map/`.

### Construction subtlety: Delete and AC pattern indexing

During `SimpleMatcher::new`, each sub-pattern is indexed under `process_type - ProcessType::Delete` rather than the full `ProcessType`. Delete-normalized text is what the automaton scans, so patterns must NOT themselves be Delete-transformed before indexing — they are stored verbatim and matched against the already-deleted text variants.

### Feature Flags

| Flag | Default | Notes |
|------|---------|-------|
| `perf` | on | Meta-feature enabling `dfa + simd_runtime_dispatch + harry` |
| `dfa` | via `perf` | Aho-Corasick DFA — faster but ~17× memory vs DAAC; preferred for pure-ASCII sets ≤ 25,000 patterns (above that combined DFA+charwise exceeds L2/cache budget) |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD dispatch for ASCII deletion (AVX2/NEON/portable fallback) |
| `harry` | via `perf` | Harry column-vector SIMD scan backend; auto-selected for `is_match` when ≥ 64 patterns exist; handles both ASCII and CJK |
| `runtime_build` | off | Build transformation tables at runtime — slower init, dynamic rules |

### Workspace Layout

- `matcher_rs/` — Core library (`rlib`); all algorithms live here
- `matcher_py/` — Python bindings via PyO3
- `matcher_c/` — C FFI bindings (`cdylib`)
- `matcher_java/` — Java JNI bindings (`cdylib` + Maven)

### Key Source Files

**`matcher_rs/src/simple_matcher/`** — Core matching engine (directory module). `SimpleMatcher` stores three components: `ProcessPlan` (transform tree + `SearchMode`), `ScanPlan` (AC automata + pattern index), `RuleSet` (rule metadata + state transitions).
- `mod.rs` — `SimpleMatcher`, `SimpleResult`, `ProcessPlan`, `SearchMode` enum (`AllSimple`/`General`), public API (`is_match`, `process`, `process_into`)
- `build.rs` — `SimpleMatcher::new()` + helpers (`build_pt_index_table`, `parse_rules`), `ParsedRules` intermediate representation
- `engine.rs` — `ScanPlan`, `BytewiseMatcher` (AC DFA or DAAC bytewise for ASCII), `CharwiseMatcher` (DAAC charwise) — AC automaton compilation and scan iteration; Harry dispatch in `is_match`
- `harry/` — `HarryMatcher` — column-vector SIMD scan engine (Harry12b dual-index encoding); `mod.rs` (core types + dispatch + scalar), `build.rs` (construction), `neon.rs` (AArch64), `avx512.rs` (x86-64); auto-selected for `is_match` via `ScanPlan` when ≥ 64 patterns exist
- `rule.rs` — `RuleSet`, `RuleHot`, `RuleCold`, `PatternEntry`, `PatternKind`, `PatternDispatch`, `DIRECT_RULE_BIT`, `SimpleTable`/`SimpleTableSerde` type aliases, state transition logic (`process_entry`)
- `search.rs` — Hot-path: `is_match_simple`, `walk_and_scan` (unified tree walk with materialize+scan), `process_simple`, `scan_variant`, `process_match`
- `state.rs` — `WordState`, `SimpleMatchState`, `ScanState` (split-borrow view for register-cached base pointers), `ScanContext`, TLS `SIMPLE_MATCH_STATE`, generation-based state reset

**`matcher_rs/src/process/`** — Text normalization pipeline:
- `process_type.rs` — `ProcessType` bitflags + serde/display
- `string_pool.rs` — Thread-local `STRING_POOL` (string buffer recycling)
- `graph.rs` — `ProcessTypeBitNode`, `build_process_type_tree` (trie construction, `pub(crate)`)
- `step.rs` — `TransformStep` enum, `StepOutput`, `TRANSFORM_STEP_CACHE`, `get_transform_step` — uniform apply interface + lazy per-process init
- `api.rs` — Standalone helpers: `text_process`, `reduce_text_process`, `reduce_text_process_emit`
- `transform/replace.rs` — `FanjianMatcher`, `PinyinMatcher` (page-table + SIMD skip), `NormalizeMatcher` (Aho-Corasick)
- `transform/delete.rs` — `DeleteMatcher` (flat BitSet + ASCII LUT + SIMD bulk-skip)
- `transform/utf8.rs` — Shared `decode_utf8_raw` unsafe helper (used by `replace.rs` and `delete.rs`)
- `transform/simd.rs` — `portable_simd` helpers: `skip_ascii_simd`, `simd_ascii_delete_mask`, `skip_non_digit_ascii_simd`
- `transform/constants.rs` — Precompiled transformation tables (generated by `build.rs`)

**Other:**
- `matcher_rs/src/builder.rs` — `SimpleMatcherBuilder` fluent API
- `matcher_rs/process_map/` — Source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) consumed by `build.rs` and `runtime_build`

## Important Notes

- ALWAYS profile before AND after optimizations to validate the mechanism, then bench-compare for the final adopt/revert decision. Profile comparison is fast (~30s) and shows whether the target category % actually changed; bench-compare is slow (~15 min) but is the authoritative throughput measurement.
- ALWAYS update `DESIGN.md` after making any non-trivial code changes to keep documentation accurate.
- Benchmarks use `divan` (not `criterion`) — write new benchmarks with `#[divan::bench]` attributes.
- `proptest` is available for property-based testing in `matcher_rs`.
- With heavy `#[inline(always)]` + full LTO, LLVM applies CSE across function boundaries. Source-level "redundancy" (e.g., duplicate `text.is_ascii()` calls) may already be a single operation in generated code. Profile category % is the ground truth, not source reading.

### Failed Optimizations

Do not re-attempt these. Each was profiled and/or benchmarked; the mechanism was validated as ineffective.

| Optimization | Expected | Actual | Why it failed |
|---|---|---|---|
| Cache `text.is_ascii()` at top of `ScanPlan::is_match` | -7% ASCII check on is_match/en | +8% regression (bench); profile: ASCII check % unchanged on DFA path, 28% overhead added on Harry path | LLVM already CSE'd the duplicate call on the DFA path. On Harry path (>25K rules), original code avoids `is_ascii()` entirely via `!is_dfa` short-circuit — unconditional caching adds a wasted 580KB linear scan per call. |
| Replace `Option::as_ref().is_some_and()` with `if let Some` in engine dispatch | -2% dispatch overhead | Neutral to slight regression | `is_some_and` compiles to tighter code under LTO than nested `if let` patterns. |
| Compact `RuleHot` by splitting out `segment_counts` to separate `Vec<Vec<i32>>` | -3% on process/and (smaller hot array fits L1) | +3.9% regression on process_hit, +3.7% on shape_process/literal | Moving `segment_counts` to a separate field in `RuleSet` changed struct layout, degrading cache behavior. The `non_null<RuleHot>` overhead only dropped 5.4%→4.9% — the Vec pointer resolution cost is per-access regardless of element size. The extra `Vec<Vec<i32>>` added indirection without offsetting the cache benefit. |
| Pre-resolve `&[RuleHot]` slice before AC scan closure (ScanState-style split-borrow for immutable data) | -5-8% on process/and (eliminate Vec pointer re-resolution in `process_entry`, same technique as ScanState split-borrow) | Neutral: shape_process/and +0.3%, shape_process/literal +0.03%, shape_process/not -1.1% (all within noise) | Profiling build (thin LTO) confirmed the mechanism: `RawVecInner::non_null<RuleHot>` dropped 5.8%→1.1%. But bench build (full LTO) showed zero throughput change — LLVM's full LTO already hoists the `Vec<RuleHot>` base pointer across closure boundaries. The profiling build's optimization barrier is an artifact of thinner LTO, not present in the authoritative bench profile. |
| NEON bitmask extraction for `skip_ascii_non_delete_neon` / `first_non_ascii_in_neon` slow path | -3-5% on text_transform/delete (replace scratch buffer + `.iter().position()` scalar scan with `vandq_u8(stop_mask, powers)` + `vaddv_u8` + `trailing_zeros`) | Neutral: en/delete -1.1%/+3.0%, cn/delete +0.9%, en/normalize -2.7%/-7.4% (all within run-to-run noise) | The bitmask extraction only fires on the NEON slow path (chunks containing a stop byte). For ASCII-heavy text with sparse deletes/non-ASCII, the slow path triggers too infrequently for the per-call improvement to be measurable. Profile confirmed the slow path `.position()` dropped from 3.5%→0%, but the savings were absorbed by increased iteration count (same total runtime). |
