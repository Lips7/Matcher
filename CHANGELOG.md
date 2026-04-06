# Changelog


## 0.14.1 - 2026-04-07

### Performance
- LUT + unchecked indexing for word boundary checks.
- Fused romanize-scan path via `RomanizeFilterIterator`.
- Store `and_count` in `PatternEntry` to eliminate `RuleHot` cache misses.

### Documentation
- Add `# Panics` sections for `compile_automata`, `walk_and_scan`, `process_entry`, and `get_transform_step`.
- Fix 2 broken `RuleHot::and_count` intra-doc links (field moved to `PatternEntry`).
- Update CLAUDE.md and DESIGN.md for RuleHot/PatternEntry restructure.

### Refactor
- Unify `NormalizeFilterIterator` state into single remainder struct.
- Remove OPTIMIZATION_IDEAS.md (no longer needed).

### Bug Fixes
- Remove unused import of `text_process` in bench.rs.
- Update profiling category rules for current architecture.
- Expand DFA scan category in time profile parser for improved accuracy.

### Tooling
- Add `profile_build` example and `--target build` support to profiler.
- Add overlap comparison benchmarks for 3 AC engines.
- Rewrite `text_transform` benchmarks to measure full matcher pipeline.

## 0.14.0 - 2026-04-06

### Features
- Add word boundary matching (`\b`) for whole-word precision in pattern rules.
- Add OR operator (`|`) for alternative patterns within rules.
- Add `EmojiNorm` ProcessType for emoji-to-English-word normalization via CLDR short names.
- Generalize CJK transforms — rename Fanjian→VariantNorm, PinYin→Romanize with expanded JP/KR data.

### Performance
- Replace `is_ascii` + Harry SIMD dispatch with density-based engine selection (`count_non_ascii_simd` NEON/AVX2/portable). Harry matcher removed entirely.
- 3-way fused scan dispatch — DFA materialize at low density, streaming charwise at high density, with 0.67 non-ASCII threshold.
- Always build DFA + DAAC bytewise together; raise DFA pattern threshold to 25K.
- Replace Normalize AC DFA with page-table + fused streaming scan.
- Implement fused delete-scan path to stream non-deleted bytes directly into AC.
- Eliminate `Vec` pointer re-resolution in scan hot path via `ScanState` split-borrow.
- Optimize AC scan closure by pre-resolving `&[RuleHot]` slice and removing per-hit indirection.
- Enhance bytewise matcher with prefilter acceleration.
- Replace PrefixMap binary search with AHashMap for O(1) verification.
- Specialize `AllSimple` process loop for single-transform-type matchers.
- Skip `is_ascii()` dispatch when all patterns are ASCII.

### Refactor
- Split `simple_matcher/rule.rs` into `encoding.rs`, `pattern.rs`, and `rule.rs` modules.
- Split `replace.rs` into `variant_norm.rs`, `romanize.rs`, `normalize.rs` sub-modules.
- Add `Fanjian` streaming byte iterator and integrate into transform pipeline.
- Replace `#[inline(always)]` with `#[inline]` for improved inlining heuristics.
- Remove `runtime_build` feature.
- Merge duplicate leaf-node scan paths in `walk_and_scan`.
- Remove dead abstractions and fix stale doc links.

### Bug Fixes
- Resolve broken rustdoc links after module split.
- Propagate transform output density for correct engine dispatch.

### Tooling
- Add interactive benchmark visualization with Plotly (`just bench-viz`).
- Add engine dispatch characterization example and visualization.
- Add Instruments profiling with `atos` inline resolution and source attribution.
- Add pre-commit configuration with hooks for all languages.
- Simplify bench/profiling tooling and add missing operator coverage.

### Documentation
- Enhance documentation with examples and performance notes across modules.
- Document `ScanState` split-borrow optimization and `RuleHot` compaction in DESIGN.md.
- Streamline CLAUDE.md with updated architecture and commands.

## 0.13.0 - 2026-04-03

### Features
- Add `heap_bytes()` to `HarryMatcher` and `SimpleMatcher` for heap memory introspection across all matcher components (AC automata, Harry tables, rule metadata, process-type trie).

### Performance
- Unify HarryMatcher into a single matcher with wildcarded columns, eliminating per-prefix-length scans (6x on CJK, 3-4x on mixed haystacks).
- Column-0 early exit in NEON/AVX512 kernels skips columns 1-7 for ~95% of non-ASCII chunks.
- Replace AHashMap with sorted split-array PrefixMap in Harry verification for L1-friendly binary search.
- Gate Harry dispatch on ASCII-only patterns and DFA absence; improve non-ASCII haystack routing.
- Const-generic SIMD kernels with PREFIX_LEN-scoped column loading.

### Testing
- Add 15 targeted coverage tests (process type display, streaming scan paths, NEON edge cases, threaded compilation).
- Coverage: 86% of testable lines (excluding platform-gated AVX512, binding crates, benchmarks).

### CI
- Fix SIGILL on x86_64 CI runners by overriding `target-cpu=native` from `.cargo/config.toml`.
- Add separate coverage workflow with tarpaulin and Codecov integration.
- Replace Makefile with Justfile for all build/test/bench/lint commands.

### Build
- Add `scripts/bump-version.sh` and `scripts/dev-setup.sh` for release and onboarding automation.

## 0.12.3 - 2026-04-02

### Performance
- Add per-plan `charwise_density_threshold` to `ScanPlan`; `AcDfa` never routes to charwise at any density, `DaacBytewise` uses 0.1.
- Raise `AC_DFA_PATTERN_THRESHOLD` 5000 → 7000 based on M3 Max benchmarks (+14% at 7k, -15% cliff at 8k due to L2 cache boundary).
- Align ASCII transform fast paths — consolidate `is_ascii` / `output_density` tracking across `TransformStep`, simplify per-transform ASCII detection.

### Bug Fixes
- Fix leaf-transform noop handling: leaf nodes in the process trie that are ASCII no-ops were incorrectly re-scanning instead of reusing the parent variant.

### Data
- Regenerate all process maps (VARIANT_NORM, NORM, NUM-NORM, ROMANIZE, TEXT-DELETE) from updated Python sources.
- Move map generator script into `matcher_rs/scripts/generate_process_map.py`; add `manifest.json` for reproducibility.
- Remove large raw Unicode source files from `data/str_conv/` (now generated on demand).

### Benchmarks
- Add `density_dispatch` bench module to calibrate the charwise threshold.
- Add `pattern_mix_en` / `pattern_mix_cn` modules with CJK-% sweep to validate the `all_ascii` guard.
- Extend `search_ascii_en` with 6000/7000/8000 pattern counts around the DFA threshold.

### Testing
- Add `proptest`-based property tests for transform correctness.
- Extend transform unit tests; remove redundant `matcher_rs` coverage.

### Documentation
- Major rustdoc pass: `ReplacementFinder`, string pool, `decode_utf8_raw`, `AsciiInputBehavior`, `get_transform_step`, `build_process_type_tree`, `multibyte_density`, SIMD skip functions.
- Update DESIGN.md: density-based engine selection, `StepOutput` shape, `ScanPlan` accessor list, threshold and constructor docs.
- Add `#![warn(missing_docs)]` to crate root.

## 0.12.2 - 2026-03-31

### Performance
- Fix Romanize regression by eliminating `Replacement` enum indirection in replacement engines.
- Unify streaming tree walk into single `walk_and_scan` method — 25% faster `process`, 33% faster `is_match`.
- Lazy transform pipeline for `is_match` — skips materializing text variants when early exit is possible.

### Refactor
- Merge charwise + normalize into unified `replace.rs` with shared `ReplacementFinder` trait.
- Deduplicate SIMD dispatch and AVX2 entry points with macros.
- Extract shared UTF-8 decoder to `transform/utf8.rs`.
- Merge `step.rs` and `registry.rs` into single `step` module.
- Remove dead public API after `walk_and_scan` unification.
- Remove unused optimizations (masks pool, VariantNorm in-place, `SingleProcessType` const generic).
- Remove unused `daachorse` dependency and related non-overlapping code.

### Bug Fixes
- Remove broken single-step match processing methods from `SimpleMatcher`.

### Testing
- Add unit tests for critical internals and improve coverage infrastructure.
- Simplify runtime build test configuration.

### Documentation
- Add doc tests and expand rustdoc for public API gaps.
- Update `CLAUDE.md` and `DESIGN.md` for post-refactor accuracy.

## 0.12.1 - 2026-03-30

### Performance
- Optimize search throughput 10-17% via six hot-path improvements.
- Encode `rule_idx` directly in automaton values for simple single-PT patterns (`DIRECT_RULE_BIT`), eliminating one indirection per hit.
- Skip `text.is_ascii()` scan when only ASCII patterns exist.
- Optimize `is_match` hot path with two targeted improvements.
- Raise `AC_DFA_PATTERN_THRESHOLD` to 5000 and optimize `bench_engine`.
- Improve `SimpleMatcher` build performance up to 42%.
- Replace std `HashMap` with `ahash` in `runtime_build` transform init.

### API
- `SimpleMatcher::new` and `builder::build` now return `Result` instead of panicking.
- `SimpleMatcherBuilder::add_word` accepts owned `String` in addition to `&str`.
- Add `#[must_use]` to public types and query methods.
- Derive `PartialEq`/`Eq` on `SimpleResult`; add `Send + Sync` static assertions.
- Add manual `Debug` impl for `SimpleMatcher`.

### Features
- Release GIL and add batch methods (`is_match_batch`, `process_batch`) in Python bindings.

### Bug Fixes
- Harden construction against invalid `ProcessType` and edge-case rules.
- Fix Romanize handling to correctly track `is_ascii` for unmapped characters.
- Resolve broken intra-doc link to cfg-gated private function.

### Safety
- Deny `unsafe_op_in_unsafe_fn` lint, document all unsafe blocks with `SAFETY` comments.
- Add `SAFETY` comments to all unsafe blocks in AVX2 SIMD functions.
- Add crate-level Safety section documenting unsafe usage.

### Refactor
- Reorganize `simple_matcher` internals into focused modules (`build.rs`, `engine.rs`, `rule.rs`, `search.rs`, `state.rs`).
- Reorganize transform pipeline into dedicated modules under `process/`.
- Replace `FLAG_*` bit flags with `RuleShape` enum in `PatternEntry`.

### Testing
- Add 6 property tests for correctness invariants.
- Reorganize test suite by system-under-test.

### Documentation
- Rewrite `DESIGN.md` and update `CLAUDE.md` to match refactored codebase.
- Add API tutorial and profiling targets in `examples/`.
- Update `DESIGN.md` to reflect search throughput optimizations.

### CI
- Adopt `cargo-nextest` across all test workflows.
- Enable `rust-lld` linker for test and bench builds.
- Streamline cargo installation in release workflow.
- Improve CI workflow reliability and efficiency.

## 0.12.0 - 2026-03-28

### Performance
- `all_simple` fast path for `is_match` — bypasses TLS state, generation counters, and overlapping iteration for pure-literal matchers.
- Dedup length pre-filter to skip redundant pattern entries during construction.
- Thread-local `TRANSFORM_STATE` bundles scratch buffers into a single TLS lookup per call; literal fast path avoids TLS entirely for simple cases.
- In-place VariantNorm optimization — exploits same-byte-length property of 99%+ Traditional-to-Simplified mappings to avoid scan-and-rebuild allocations.
- Shrink `PatternEntry` from 16 to 8 bytes via sequential process-type indexing.
- Embed dedup indices directly in DAAC automaton values, eliminating one indirection per hit.
- Track `is_ascii` flag through the transform pipeline to skip redundant charwise scans on ASCII-only text.
- Auto-select DAAC bytewise engine over AC DFA when ASCII pattern count exceeds 2000.

### Refactor
- Replace `PatternEntry` boolean flags with `PatternKind` enum for clearer dispatch in `process_match`.
- Reorganize `matcher_rs` into focused single-responsibility modules: `simple_matcher/` split into `types.rs`, `construction.rs`, `scan.rs`; `process/` split into `process_type.rs`, `string_pool.rs`, `process_tree.rs`, `transform/`.
- Improve code clarity via named structs (`ScanContext`, `RuleHot`, `RuleCold`) and bundled TLS parameters.

### Dependencies
- Bump `sonic-rs` to 0.5.8, `tinyvec` to 1.11.0, `proptest` to 1.11.0.
- Migrate `matcher_java` JNI bindings to `jni` 0.22.4.

### Documentation
- Rewrite `DESIGN.md` to reflect current implementation with detailed sections on state management, SIMD dispatch, and const-generic optimizations.
- Update all READMEs to match current package APIs: document `text_process`/`reduce_text_process` in C and Java bindings, add ProcessType reference tables, fix paths, improve build instructions.

## 0.11.0 - 2026-03-12

### Breaking Changes
- Removed the `vectorscan` backend to simplify the build process and eliminate the external Boost dependency requirement.

### Performance
- Simplified SIMD utility dispatching by removing `OnceLock`/`SimdDispatch` for AArch64 (NEON is now always baseline) and gating it for x86_64 only.
- Removed dead API surface and unused parameters in SIMD hot paths.
- Optimized search hot paths and benchmark tooling in `matcher_rs`.
- Added comprehensive benchmark results for MacBook Air M4 (Apple Silicon).

### Documentation
- Filled documentation gaps, added `# Panics` / `# Errors` / `# Arguments` sections, and explained internal implementations in `matcher_rs`.
- Aligned public documentation and improved comments on private items for better maintainability.

## 0.10.3 - 2026-03-11

### Performance
- Hot/cold struct split, pre-computed masks, TLS consolidation for reduced per-call overhead in `SimpleMatcher`.
- Skip unused text variants during process-tree traversal, avoiding redundant transformations.
- Cache Romanize trim metadata to eliminate repeated recomputation.
- Lazy tree walking for unique text variants — process-tree nodes are now visited on demand rather than eagerly.

### Refactor
- Extract `is_rule_satisfied` as a dedicated method for clarity and measurable performance improvement.
- Optimize tree node index handling in `walk_process_tree` (formerly `reduce_text_process_with_tree`).
- Rename traversal function to `walk_process_tree` and update terminology throughout.
- Improve encapsulation: `SingleCharMatcher`/`SingleCharMatch` visibility narrowed; `SimpleMatcher` internals use more descriptive struct names (`RuleHot`, etc.).
- Add safety assertions in `page_table_lookup`.
- Update type-ignore comments in test cases for clarity.

### Documentation
- Update terminology and traversal descriptions in `DESIGN.md`.
- Update benchmark records in `README.md` with new results.

## 0.10.2 - 2026-03-10

### Bug Fixes
- Fix `DeleteFindIter` SIMD fast-skip incorrectly advancing past deletable ASCII bytes (e.g. spaces) that appear before a non-ASCII character in the same 16-byte chunk. The `non_ascii_mask` was checked before `del_mask`, causing the skip to jump to the first non-ASCII byte and silently drop intervening deletable characters. Fixed by ORing both masks and stopping at the first set bit in either.

## 0.10.1 - 2026-03-10

### Performance
- Monomorphize `SingleChar` iterators and add SIMD ASCII chunk-skip for faster inner loops.
- Byte-level `Romanize`/`Delete` iterators and `ascii_lut` fast-path, eliminating UTF-8 decoding overhead on ASCII-heavy input.
- `portable_simd` SIMD helpers (`skip_ascii_simd`, `simd_ascii_delete_mask`, `skip_non_digit_ascii_simd`) for 16-byte parallel probing in `SingleChar` skip loops.

### Features
- Exhaustive property-based and unit tests for `VariantNorm`, `Delete`, `Normalize`, and `Romanize` process types.
- Macro-based benchmark generation with `BytesCount` metric for normalized throughput measurement.

### Refactor
- Improve clarity and consistency across the `process` module.

### Documentation
- Improve `CLAUDE.md` with benchmark scoping, test-file syntax, and architecture details.
- Move benchmark output to `bench_records/` and link from README.
- Clarify `get_or_init_matcher` return type in docs.

## 0.10.0 - 2026-03-07

### Breaking Changes
- Removed `Matcher`, `RegexMatcher`, and `SimMatcher` components to focus on the high-performance `SimpleMatcher`.
- Updated C and Java FFI interfaces to only support `SimpleMatcher`.

### Documentation
- Updated `README.md`, `DESIGN.md`, and `GEMINI.md` to reflect the focus on `SimpleMatcher`.
- Cleaned up documentation and examples across all language bindings.

## 0.9.0 - 2026-03-05

### Refactor & Performance
- Replace standard `HashMap` and `HashSet` with `FxHashMap` and `FxHashSet` for improved execution speed.
- Replace `Vec<i32>` with `TinyVec` in `simple_matcher` for improved performance.
- Optimize inner loop with `Vec` indexing and flat matrix in `simple_matcher`.
- Use `FxHashMap` + `u64` bitmask for the inner loop of `simple_matcher`.
- Rename `ProcessedTextSet` to `ProcessedTextMasks` and update its representation to use a `u64` bitmask for process types.
- Simplify `TextMatcherTrait` by deriving `is_match` and `process_iter` from `process`, and remove the `TextMatcherInternal` trait.
- Simplify word splitting logic in `SimpleMatcher::new` using a helper closure and adjust lifetime bounds for borrowed types.
- Simplify C FFI panic handling and wrap all `panic::catch_unwind` calls in FFI functions with `AssertUnwindSafe`.
- Remove `word_id` from match result structs, refine regex pattern handling and matching.
- Unconditionally configure mimalloc as the global allocator and remove conditional allocator dependencies.

### Maintenance & Documentation
- Standardize Rust documentation and include detailed algorithm explanations across all matching engines.
- Update benchmark results in README.md after modifications to the simple matcher.
- Configure `rustflags` to use 8 compilation threads.
- Streamline CI Rust testing by adopting `cargo-all-features` and enabling `RUST_BACKTRACE`.
- Expand and update CI workflows (upgrade action runners to `ubuntu-24.04-arm` and `macos-latest`).
- Remove `AGENTS.md` and legacy tracker files.

## 0.8.1 - 2026-03-01

### Refactor & Performance
- Replace `nohash-hasher`, `id-set`, `FxHashMap` (`rustc-hash`), and `micromap` with std collections (`HashMap`/`HashSet`), removing these external dependencies.
- Replace `tinyvec::ArrayVec` with `std::vec::Vec` for dynamic collections in the process matcher.

### Maintenance & Documentation
- Standardize rustdoc comments and add intra-doc links to type names across the project for improved readability.
- Improve build/linting commands and remove outdated feature mentions.

## 0.8.0 - 2026-02-28

### Breaking Changes
- Implement sealed trait pattern for `TextMatcherTrait`.

### Refactor & Performance
- Use `Box<[T]>` for frozen `Vec` fields to optimize memory.
- Introduce `gen` blocks for `process_iter` implementations to improve iteration.
- Remove unsafe code, update `aho-corasick` dependency, optimize matcher with `tinyvec`.
- Introduce `ProcessTypeError` for `text_process` handling.
- Use `eprintln` for warnings instead of `println`.
- Consolidate conditional matching logic and update FFI function attributes to `unsafe(no_mangle)`.
- Improve struct initializations and Option block handling.

### Features
- Derive `Debug` on `MatchResult` for consistency.
- Add `diagnostic::on_unimplemented` to public traits for better compiler errors.

### Maintenance & Documentation
- Update Rust edition to 2024.
- Add `rust-toolchain.toml` to use nightly toolchains for reproducible builds.
- Remove direct deserialization for core types.
- Improve `SimpleMatcher` and `Matcher` instantiation examples to recommend builder patterns.
- Ensure correct and modern rust idiom implementations across repo.

## 0.7.2 - 2026-02-25

### Refactor & Performance
- Removed explicit ASCII case-insensitivity from `AhoCorasickBuilder` to simplify builder configuration.
- Deferred `String` allocation in `ProcessMatcher`'s `replace_all` and `delete_all` for performance optimization.
- Simplified `TextMatcherTrait` and various internal matcher method implementations.
- Expanded testing suite by separating tests into individual files, adding edge case checks and fixing slice coercion in proptests.

### Maintenance & Documentation
- Switched `aho-corasick-unsafe` dependency from git source to `crates.io`.
- Updated benchmarks with deterministic scenarios for process types.
- Enhanced Java example to use the high-level API and adjusted the environment for macOS.
- Heavily improved documentation across `README.md`, `README_CN.md`, `AGENTS.md` and specific language READMEs.

## 0.7.1 - 2026-02-21

### Security & Safety (Audit Fixes)
- **FFI Panic Safety**: All entry points in `matcher_c` are now wrapped in `catch_unwind` to prevent native crashes when Rust code panics.
- **Memory Robustness**: Fixed brittle raw pointer usage in `reduce_text_process_with_tree` (process matcher) by switching to indexing.
- **ReDoS Protection**: Added pattern length limits (1024 chars) to `RegexMatcher` to mitigate exponential backtracking risks.
- **Invariants**: Added `debug_assert!` checks across `SimpleMatcher` to verify internal consistency in development.

### Java
- **Ergonomics**: Introduced high-level `Matcher` and `SimpleMatcher` classes that implement `AutoCloseable` for automatic native memory management (RAII).

### API (from 0.7.0)
- **Breaking**: `MatchResultTrait::similarity` now returns `Option<f64>` — `None` for exact
  matchers (Simple, Regex) and `Some(score)` for similarity matchers.
- **Breaking**: `MatchTableTrait::word_list` and `exemption_word_list` now return `&[S]`
  instead of `&Vec<S>`.
- Internal `TextMatcherTrait` methods are now marked `#[doc(hidden)]`.

### Performance / Correctness
- Fixed double-checked locking in `get_process_matcher`.
- Re-enabled `overflow-checks` globally; hot-path arithmetic uses `wrapping_add` / `wrapping_mul`.

### Maintenance
- Replaced `lazy_static` with `std::sync::LazyLock`.
- Updated documentation regarding `!Send` iterators and git-dependency limitations.

## 0.6.0 - 2026-02-21

### Added
- Builder API: `SimpleMatcherBuilder`, `MatchTableBuilder`, `MatcherBuilder`.
- `process_iter` — lazy iterator over match results for all four matcher types.
  `RegexMatcher` and `SimMatcher` have truly lazy implementations;
  `SimpleMatcher` wraps `process()` (two-pass AC constraint documented);
  `Matcher` avoids the final `collect()` via `into_values().flatten()`.

## 0.5.9 - 2025-08-23

### Changed
- Update dependencies.

## 0.5.8 - 2025-08-23
- staticmethod for extension_types.py
- Update dependencies.

## 0.5.7 - 2025-03-17

### Flexibility
- Update dependencies.

## 0.5.6 - 2024-11-18

### Performance
- Fix `build_process_type_tree` function, use set instead of list.
- Update several dependencies.

## 0.5.5 - 2024-10-14

### Bug fixes
- Change `XXX(Enum)` to `XXX(str, Enum)` in extension_types.py to fix json dumps issue.

### Flexibility
- Add Python 3.13 support.
- Remove msgspec, only use json in README.md.

## 0.5.4 - 2024-08-23

### Readability
- Fix typo and cargo clippy warnings.
- Add single line benchmark.

## 0.5.3 - 2024-07-26

### Bug fixes
- Fix simple matcher is_match function.

## 0.5.2 - 2024-07-22

### Flexibility
- Remove msgpack, now non-rust users should use json to serialize input of Matcher and SimpleMatcher.
- Refactor Java code.

## 0.5.1 - 2024-07-19

### Performance
- Use FxHash to speed up simple matcher process.

### Flexibility
- Remove unnecessary dependencies.

## 0.5.0 - 2024-07-18

### Changed
- Major internal refactor of SimpleMatcher internals. See git history for details.

## 0.4.6 - 2024-07-15

### Performance
- Optimize SimpleMatcher hot-path performance.

## 0.4.5 - 2024-07-12

### Changed
- Optimize Simple Matcher `process` function when multiple simple_match_type are used.
- Add `dfa` feature to matcher_rs.
- Shrink `VARIANT_NORM` conversion map.

## 0.4.4 - 2024-07-09

### Changed
- Merge ROMANIZE and ROMANIZECHAR process matcher build.
- Add `process` function to matcher_py/c/java.
- Fix simple matcher process function issue.
- Refactor matcher_py file structure, use `rye` to manage matcher_py.
- Delete `println!` in matcher_c.

## 0.4.3 - 2024-07-08

### Changed
- Fix exemption word list wrongly reject entire match, not a single table.
- Add match_id to MatchResult.
- Reverse DFA structure to AhoCorasick structure.
- matcher_c use from_utf8_unchecked instead of from_utf8.
- Build multiple wheels for different python version.
- Update VARIANT_NORM.txt and NORM.txt.
- Fix issues with `runtime_build` feature.

## 0.4.2 - 2024-07-07

### Performance
- Optimize SimpleMatcher construction and search throughput.

## 0.4.1 - 2024-07-06

### Changed
- Rebuild Transformation Rules based on Unicode Standard.

## 0.4.0 - 2024-07-03

### Changed
- Implement NOT logic word-wise inside SimpleMatcher, now you can use `&`(and) and `~`(not) separator to config simple word, eg: `hello&world~helo`.
