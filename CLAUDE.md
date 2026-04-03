# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

High-performance multi-language word/text matcher in Rust with Python, C, and Java bindings. Solves precision/recall problems in pattern matching via logical operators (`&`/`~`) and text normalization pipelines.

**Toolchain:** nightly Rust (see `rust-toolchain.toml`), edition 2024. Nightly is required — do not change this. Edition 2024 has behavioral changes: `gen` is reserved, `unsafe` blocks required inside `unsafe fn`, etc.

**Prerequisites:** `cargo-all-features` (`cargo install cargo-all-features`), `cargo-nextest` (`cargo install cargo-nextest`), `uv` (Python env manager), `prek` (pre-commit runner).

## Commands

```bash
# Build
make build                          # Full workspace + copy bindings artifacts
cargo build --release               # Rust only

# Quick iteration
make check                          # Fast type-check (no codegen)
make test-quick                     # Default-features tests only
make fmt                            # Auto-format
make fmt-check                      # Check formatting without modifying

# Test
make test                           # All languages (Rust + Python + Java + C)
make test-rs                        # All feature combos + doctests + docs
make test-py                        # Python bindings
make test-java                      # Java bindings
make test-c                         # C bindings
cd matcher_rs && cargo nextest run <test_name>                  # Single test by name
cd matcher_rs && cargo nextest run --no-default-features        # Without DFA
cd matcher_rs && cargo nextest run --test test_engine           # Single test file by name

# Lint/Format
make lint                           # All languages (rs + py + java)
make lint-rs                        # cargo fmt + cargo clippy (matcher_rs)
make lint-py                        # cargo fmt + cargo clippy + ruff + ty check (matcher_py)
make lint-java                      # cargo fmt + cargo clippy + mvn checkstyle (matcher_java)
make lint-c                         # cargo fmt + cargo clippy (matcher_c)

# Benchmarks (harness: divan, two targets: bench, bench_engine)
make bench-search                   # Main throughput workflow
make bench-build                    # Matcher construction workflow
make bench-engine-search            # Raw engine throughput workflow
make bench-engine-build             # Raw engine build workflow
python3 matcher_rs/scripts/run_benchmarks.py --preset search --quick  # Quick directional signal (~2-3 min)
python3 matcher_rs/scripts/run_benchmarks.py --profile bench-dev      # Faster rebuild (thin LTO)
make bench-compare BASELINE=<baseline_dir> CANDIDATE=<candidate_dir>
python3 matcher_rs/scripts/compare_benchmark_runs.py <baseline_dir> <candidate_dir>  # direct invocation
python3 matcher_rs/scripts/compare_benchmarks.py baseline.txt new.txt                # raw file-to-file fallback

# Profiling (uses release + debug symbols)
cd matcher_rs && cargo build --profile profiling

# Coverage
make coverage                       # cargo tarpaulin → matcher_rs/tarpaulin-report.html

# Dependency updates
make update                         # cargo update --breaking + cargo upgrade
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
| `dfa` | via `perf` | Aho-Corasick DFA — faster but ~10x memory vs NFA; only preferred for pure-ASCII sets ≤ 7,000 patterns (cache cliff above that) |
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
- `engine.rs` — `ScanPlan`, `BytewiseMatcher` (AC DFA or DAAC bytewise for ASCII), `CharwiseMatcher` (DAAC charwise), `PatternIndex` — AC automaton compilation and scan iteration
- `harry.rs` — `HarryMatcher` — column-vector SIMD scan engine (Harry12b dual-index encoding); NEON on AArch64, AVX512-VBMI on x86-64, scalar fallback; auto-selected for `is_match` via `ScanPlan` when ≥ 64 patterns exist
- `rule.rs` — `RuleSet`, `RuleHot`, `RuleCold`, `PatternEntry`, `PatternKind`, `PatternDispatch`, `DIRECT_RULE_BIT`, `SimpleTable`/`SimpleTableSerde` type aliases, state transition logic (`process_entry`)
- `search.rs` — Hot-path: `is_match_simple`, `walk_and_scan` (unified streaming tree walk), `process_simple`, `scan_variant`, `scan_variant_streaming`, `process_match`
- `state.rs` — `WordState`, `SimpleMatchState`, `ScanContext`, TLS `SIMPLE_MATCH_STATE`, generation-based state reset

**`matcher_rs/src/process/`** — Text normalization pipeline:
- `process_type.rs` — `ProcessType` bitflags + serde/display
- `string_pool.rs` — Thread-local `STRING_POOL` (string buffer recycling)
- `graph.rs` — `ProcessTypeBitNode`, `build_process_type_tree` (trie construction, `pub(crate)`)
- `step.rs` — `TransformStep` enum, `StepOutput`, `TRANSFORM_STEP_CACHE`, `get_transform_step` — uniform apply interface + lazy per-process init
- `api.rs` — Standalone helpers: `text_process`, `reduce_text_process`, `reduce_text_process_emit`
- `transform/replace.rs` — `FanjianMatcher`, `PinyinMatcher` (page-table + SIMD skip), `NormalizeMatcher` (Aho-Corasick), shared `ReplacementFinder` trait
- `transform/delete.rs` — `DeleteMatcher` (flat BitSet + ASCII LUT + SIMD bulk-skip)
- `transform/utf8.rs` — Shared `decode_utf8_raw` unsafe helper (used by `replace.rs` and `delete.rs`)
- `transform/simd.rs` — `portable_simd` helpers: `skip_ascii_simd`, `simd_ascii_delete_mask`, `skip_non_digit_ascii_simd`
- `transform/constants.rs` — Precompiled transformation tables (generated by `build.rs`)

**Other:**
- `matcher_rs/src/builder.rs` — `SimpleMatcherBuilder` fluent API
- `matcher_rs/process_map/` — Source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) consumed by `build.rs` and `runtime_build`

### Threading

`SimpleMatcher` is `Send + Sync`. All mutable match state is thread-local — pools per thread:
- `SIMPLE_MATCH_STATE` (`SimpleMatchState`) — generation-stamped word states and counter matrix, reused across calls
- `STRING_POOL` — recycled `String` allocations for transformation output

`TRANSFORM_STEP_CACHE` is a static `[OnceLock<TransformStep>; 8]` — each single-bit `ProcessType` initializes its step once per process and shares it across all `SimpleMatcher` instances.

**Allocator:** `mimalloc` (v3) replaces the system allocator globally for improved multi-threaded allocation throughput.

## Important Notes

- ALWAYS run benchmarks to measure baseline performance before making optimizations, run them again after changes, and compare repeated-run aggregates with `run_benchmarks.py` plus `compare_benchmark_runs.py`.
- Benchmarks use `divan` (not `criterion`) — write new benchmarks with `#[divan::bench]` attributes.
- `proptest` is available for property-based testing in `matcher_rs`.
