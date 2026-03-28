# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

High-performance multi-language word/text matcher in Rust with Python, C, and Java bindings. Solves precision/recall problems in pattern matching via logical operators (`&`/`~`) and text normalization pipelines.

**Toolchain:** nightly Rust (see `rust-toolchain.toml`). Nightly is required — do not change this.

**Prerequisites:** `cargo-all-features` (`cargo install cargo-all-features`), `uv` (Python env manager), `prek` (pre-commit runner).

## Commands

```bash
# Build
make build                          # Full workspace + copy bindings artifacts
cargo build --release               # Rust only

# Test
cargo test                          # Default features
cargo all-features test             # All feature combinations (use in CI)
cargo test <test_name>              # Single test by name
cargo test --no-default-features    # Without DFA
cargo test --test test_simple_matcher          # Single test file by name
make test                           # All languages (Rust + Python + Java + C)

# Lint/Format
make lint                           # All languages
make lint-rs                        # cargo fmt + cargo clippy
cargo fmt --all
cargo all-features clippy --workspace --all-targets -- -D warnings

# Python bindings
cd matcher_py && uv sync && uv run pytest
cd matcher_py && uv run ruff check --fix && uv run ty check

# Benchmarks (two targets: bench, bench_engine)
cargo bench -p matcher_rs                      # All benchmarks (avoids Python linker errors)
cargo bench -p matcher_rs --bench bench        # Main benchmark suite only
cargo bench -p matcher_rs -- text_process      # Specific benchmark group
cargo bench -p matcher_rs > baseline.txt       # save baseline
cargo bench -p matcher_rs > new.txt            # run new benchmark
python3 matcher_rs/scripts/compare_benchmarks.py baseline.txt new.txt # compare results

# Profiling (uses release + debug symbols)
cargo build --profile profiling -p matcher_rs
```

**Pre-commit:** `.pre-commit-config.yaml` exists — run `prek run` before committing.

## Architecture

For exhaustive internal documentation, see [DESIGN.md](./DESIGN.md). Below is the essential mental model.

### Two-Pass Matching

1. **Pattern Scan** — All unique sub-patterns across all rules are deduplicated and compiled into a single automaton (Aho-Corasick via `daachorse`, optional `dfa`). O(N) text scan. ASCII-only patterns get a separate engine for fast dispatch when input is ASCII.
2. **Logical Evaluation** — Only rules that had ≥1 hit in Pass 1 are evaluated. Sparse-set via generation IDs for O(1) state reset. Bitmask fast-path for rules with ≤64 segments; matrix fallback otherwise. `all_simple` fast-path bypasses all state machinery for pure-literal matchers.

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
| `dfa` | on | Aho-Corasick DFA — faster but ~10x memory vs NFA |
| `simd_runtime_dispatch` | on | Runtime SIMD dispatch for ASCII deletion (AVX2/NEON/portable fallback) |
| `runtime_build` | off | Build transformation tables at runtime — slower init, dynamic rules |

### Workspace Layout

- `matcher_rs/` — Core library (`rlib`); all algorithms live here
- `matcher_py/` — Python bindings via PyO3
- `matcher_c/` — C FFI bindings (`cdylib`)
- `matcher_java/` — Java JNI bindings (`cdylib` + Maven)

### Key Source Files

**`matcher_rs/src/simple_matcher/`** — Core matching engine (directory module):
- `simple_matcher.rs` — Module root: `SimpleMatcher` struct, public API (`is_match`, `process`, `process_into`)
- `types.rs` — Internal types: `AsciiMatcher`, `NonAsciiMatcher`, `WordState`, `SimpleMatchState`, `ScanContext`, `RuleHot`, `RuleCold`, `PatternEntry`, `PatternKind`, TLS `SIMPLE_MATCH_STATE`
- `construction.rs` — `SimpleMatcher::new()` + helpers (`build_pt_index_table`, `parse_rules`, `compile_automata`, `flatten_dedup_entries`)
- `scan.rs` — Hot-path: `process_preprocessed_into`, `scan_all_variants`, `scan_variant`, `process_match`, `is_rule_satisfied`, `init_matrix`

**`matcher_rs/src/process/`** — Text normalization pipeline:
- `process_type.rs` — `ProcessType` bitflags + serde/display
- `string_pool.rs` — `TextVariant`, `ProcessedTextMasks`, thread-local `STRING_POOL`/`TRANSFORM_STATE`, pool functions
- `process_tree.rs` — `ProcessTypeBitNode`, `build_process_type_tree`, `walk_process_tree`
- `process_matcher.rs` — `ProcessMatcher` enum, `get_process_matcher`, `text_process`, `reduce_text_process*`; re-exports from siblings
- `transform/constants.rs` — Precompiled transformation tables (generated by `build.rs`)
- `transform/single_char_matcher.rs` — Fanjian/Delete/Pinyin (page-table + BitSet)
- `transform/multi_char_matcher.rs` — Normalize (Aho-Corasick)
- `transform/simd_utils.rs` — `portable_simd` helpers: `skip_ascii_simd`, `simd_ascii_delete_mask`, `skip_non_digit_ascii_simd`

**Other:**
- `matcher_rs/src/builder.rs` — `SimpleMatcherBuilder` fluent API
- `matcher_rs/process_map/` — Source text files (`FANJIAN.txt`, `PINYIN.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) consumed by `build.rs` and `runtime_build`

### Threading

`SimpleMatcher` is `Send + Sync`. All mutable match state is thread-local — pools per thread:
- `SIMPLE_MATCH_STATE` (`SimpleMatchState`) — generation-stamped word states and counter matrix, reused across calls
- `STRING_POOL` — recycled `String` allocations for transformation output
- `TRANSFORM_STATE` — node-index-to-text-index scratch buffer + recycled `ProcessedTextMasks` vectors; bundles both into a single TLS lookup per call

`PROCESS_MATCHER_CACHE` is a static `[OnceLock<ProcessMatcher>; 8]` — each single-bit `ProcessType` initializes its matcher once per process and shares it across all `SimpleMatcher` instances.

**Allocator:** `mimalloc` (v3) replaces the system allocator globally for improved multi-threaded allocation throughput.

## Important Notes

- ALWAYS run benchmarks to measure baseline performance before making optimizations, run it again after changes. Use `cargo bench -p matcher_rs` and the provided comparison script.
