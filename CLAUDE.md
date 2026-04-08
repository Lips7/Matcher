# Project

High-performance multi-language word/text matcher in Rust with Python, C, and Java bindings. Solves precision/recall problems in pattern matching via logical operators (`&`/`~`/`|`/`\b`) and text normalization pipelines.

**Toolchain:** nightly Rust (see `rust-toolchain.toml`), edition 2024. Nightly is required — do not change this. Edition 2024 has behavioral changes: `gen` is reserved, `unsafe` blocks required inside `unsafe fn`, etc.

**Prerequisites:** `just` (command runner), `cargo-all-features` (`cargo install cargo-all-features`), `cargo-nextest` (`cargo install cargo-nextest`), `uv` (Python env manager), `prek` (pre-commit runner).

## Commands

```bash
# Build
just build                          # Full workspace + copy bindings artifacts
cargo build --release               # Rust only

# Quick iteration
just check                          # Fast type-check (no codegen)
just fmt                            # Auto-format
just fmt-check                      # Check formatting without modifying

# Test (test-rs and test-quick accept pass-through args)
just test                                          # All languages (Rust + Python + Java + C)
just test-rs                                       # All feature combos + doctests + docs
just test-quick                                    # Default-features tests only
just test-quick test_name                          # Single test by name (substring match)
just test-quick --no-default-features              # Without DFA
just test-quick --test test_engine                 # Single test file by name
just test-py                                       # Python bindings
just test-java                                     # Java bindings
just test-c                                        # C bindings

# Lint/Format
just lint                           # All languages + workspace clippy + doc build
just lint-check                     # Check-only (no auto-fix) — used in CI
just lint-rs                        # cargo fmt + cargo clippy (matcher_rs)
just lint-py                        # cargo fmt + cargo clippy + ruff + ty check (matcher_py)
just lint-java                      # cargo fmt + cargo clippy + mvn checkstyle (matcher_java)
just lint-c                         # cargo fmt + cargo clippy (matcher_c)

# Benchmarks (harness: divan, single target: bench)
# All bench recipes accept pass-through args: --quick, --profile, --repeats, --filter, etc.
just bench-search                                      # Main throughput workflow (~15 min)
just bench-search --quick                              # Quick directional signal (~2-3 min)
just bench-search --filter text_transform              # Only transform benchmarks (~2 min)
just bench-search --filter rule_complexity             # Only rule shape benchmarks (~3 min)
just bench-search --filter "scaling::process_cn"       # Single benchmark group (~1 min)
just bench-search --profile bench-dev                  # Faster rebuild (thin LTO)
just bench-build                                       # Matcher construction workflow
just bench-all                                         # All presets (search + build)
just bench-compare <baseline> <candidate>               # compare runs, dirs, or raw files
just bench-viz <run_dir>                               # interactive HTML dashboard (Plotly)
just bench-viz <baseline_dir> <candidate_dir>          # comparison visualization

# Profiling (scene-based, uses release + debug symbols)
cargo run --profile profiling --example profile_search -p matcher_rs -- --list        # list scenes
cargo run --profile profiling --example profile_search -p matcher_rs -- --scene all   # all scenes
just profile record --scene en-search --analyze        # Instruments + auto-analyze

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

For the full narrative walkthrough with a running example, see [DESIGN.md](./DESIGN.md). Below is the essential mental model.

### How a Query Works

1. **Transform** — Walk a shared-prefix trie of `ProcessType` steps, producing text variants (VariantNorm, Delete, Normalize, Romanize). Intermediate results are reused across combinations.
2. **Scan** — Each variant is scanned by a single deduplicated Aho-Corasick automaton (bytewise or charwise, selected by SIMD density scan at threshold 0.67). Hits update per-rule state.
3. **Evaluate** — Touched rules are checked: all AND segments satisfied + no NOT veto → match.
4. **AllSimple bypass** — When all rules are pure literals under one `ProcessType`, the trie + state machinery is skipped entirely — each automaton hit maps directly to a result.

### Key Concepts

- **ProcessType**: `u8` bitflags composable with `|`. Controls which transforms are applied before matching.
- **Transform trie**: shared-prefix DAG so `VariantNorm|Delete` reuses the VariantNorm result.
- **ScanPlan**: bytewise AC (all patterns, optional DFA) + charwise AC (all patterns, CJK-optimized). Engine selection via SIMD density scan (≤0.67 non-ASCII → bytewise, >0.67 → charwise).
- **RuleSet**: hot/cold split for cache efficiency. Generation-stamped sparse set for O(1) state reset.
- **DIRECT_RULE_BIT**: single-entry simple patterns encode `rule_idx | (1 << 31)` directly in the automaton value, skipping the entry table on the hot path.

### Construction subtlety: Delete and AC pattern indexing

During `SimpleMatcher::new`, each sub-pattern is indexed under `process_type - ProcessType::Delete` rather than the full `ProcessType`. Delete-normalized text is what the automaton scans, so patterns must NOT themselves be Delete-transformed before indexing — they are stored verbatim and matched against the already-deleted text variants.

### Feature Flags

| Flag | Default | Notes |
|------|---------|-------|
| `perf` | on | Meta-feature enabling `dfa + simd_runtime_dispatch` |
| `dfa` | via `perf` | Aho-Corasick DFA — 1.7–3.3× faster than DAAC; ~17× more memory |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD dispatch for transforms (AVX2/NEON/portable) and density counting |

**Note:** `EmojiNorm` (bit 6) maps emoji to English words via CLDR short names. Does NOT compose usefully with `Delete` — Delete removes emoji before EmojiNorm sees them. Use `EmojiNorm | Normalize` for emoji→word matching.

### Workspace Layout

- `matcher_rs/` — Core library (`rlib`); all algorithms live here
- `matcher_py/` — Python bindings via PyO3
- `matcher_c/` — C FFI bindings (`cdylib`)
- `matcher_java/` — Java JNI bindings (`cdylib` + Maven)

### Key Source Files

**`matcher_rs/src/simple_matcher/`** — Core matching engine (directory module). `SimpleMatcher` stores four fields: `tree` (transform trie), `mode` (`SearchMode`), `scan` (`ScanPlan`), `rules` (`RuleSet`).
- `mod.rs` — `SimpleMatcher`, `SimpleResult`, `SearchMode` enum (`AllSimple`/`General`), public API (`is_match`, `process`, `process_into`)
- `build.rs` — `SimpleMatcher::new()` + helpers (`build_pt_index_table`, `parse_rules`), `ParsedRules` intermediate representation
- `encoding.rs` — Bit-packing constants (`DIRECT_RULE_BIT`, `DIRECT_PT_SHIFT`, etc.), capacity limits (`BITMASK_CAPACITY`, `PROCESS_TYPE_TABLE_SIZE`)
- `engine.rs` — `ScanPlan`, `BytewiseMatcher` (AC DFA or DAAC bytewise), `CharwiseMatcher` (DAAC charwise) — AC automaton compilation, density-based dispatch, scan iteration
- `pattern.rs` — `PatternEntry` (includes `and_count` for cache locality), `PatternKind`, `PatternIndex`, `PatternDispatch` — deduplicated pattern storage and dispatch
- `rule.rs` — `RuleSet`, `RuleHot` (matrix-only: `segment_counts`), `RuleCold`, `RuleShape`, `SimpleTable`/`SimpleTableSerde` type aliases, state transition logic (`process_entry`)
- `search.rs` — Hot-path: `is_match_simple`, `walk_and_scan` (unified tree walk with materialize+scan), `process_simple`, `scan_variant`, `process_match`
- `simd.rs` — `count_non_ascii_simd` — SIMD non-ASCII byte counting for density-based engine dispatch (NEON/AVX2/portable)
- `state.rs` — `WordState`, `SimpleMatchState`, `ScanState` (split-borrow view for register-cached base pointers), `ScanContext`, TLS `SIMPLE_MATCH_STATE`, generation-based state reset

**`matcher_rs/src/process/`** — Text normalization pipeline:
- `process_type.rs` — `ProcessType` bitflags + serde/display
- `string_pool.rs` — Thread-local `STRING_POOL` (string buffer recycling)
- `graph.rs` — `ProcessTypeBitNode`, `build_process_type_tree` (trie construction, `pub(crate)`)
- `step.rs` — `TransformStep` enum, `TRANSFORM_STEP_CACHE`, `get_transform_step` — uniform apply interface + lazy per-process init
- `api.rs` — Standalone helpers: `text_process`, `reduce_text_process`, `reduce_text_process_emit`
- `transform/replace/` — `VariantNormMatcher` (`variant_norm.rs`), `RomanizeMatcher` (`romanize.rs`), `NormalizeMatcher` (`normalize.rs`), shared page-table helpers (`mod.rs`)
- `transform/delete.rs` — `DeleteMatcher` (flat BitSet + ASCII LUT + SIMD bulk-skip), `DeleteFilterIterator` (streaming)
- `transform/utf8.rs` — Shared `decode_utf8_raw` unsafe helper
- `transform/simd.rs` — SIMD helpers: `skip_ascii_simd`, `skip_ascii_non_delete_simd`
- `transform/constants.rs` — Precompiled transformation tables (generated by `build.rs`)

**Other:**
- `matcher_rs/src/builder.rs` — `SimpleMatcherBuilder` fluent API
- `matcher_rs/process_map/` — Source text files (`VARIANT_NORM.txt`, `ROMANIZE.txt`, `TEXT-DELETE.txt`, `NORM.txt`, `NUM-NORM.txt`) consumed by `build.rs`

## Important Notes

- ALWAYS profile before AND after optimizations to validate the mechanism, then bench-compare for the final adopt/revert decision. Profile comparison is fast (~30s) and shows whether the target category % actually changed; bench-compare is slow (~15 min) but is the authoritative throughput measurement.
- ALWAYS update `DESIGN.md` after making any non-trivial code changes to keep documentation accurate.
- Benchmarks use `divan` (not `criterion`) — write new benchmarks with `#[divan::bench]` attributes.
- `proptest` is available for property-based testing in `matcher_rs`.
- With heavy `#[inline(always)]` + full LTO, LLVM applies CSE across function boundaries. Source-level "redundancy" (e.g., duplicate `text.is_ascii()` calls) may already be a single operation in generated code. Profile category % is the ground truth, not source reading.
