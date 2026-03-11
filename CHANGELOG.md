# Changelog

## 0.10.3 - 2026-03-11

### Performance
- Hot/cold struct split, pre-computed masks, TLS consolidation for reduced per-call overhead in `SimpleMatcher`.
- Skip unused text variants during process-tree traversal, avoiding redundant transformations.
- Cache PinYin trim metadata to eliminate repeated recomputation.
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
- Byte-level `PinYin`/`Delete` iterators and `ascii_lut` fast-path, eliminating UTF-8 decoding overhead on ASCII-heavy input.
- `portable_simd` SIMD helpers (`skip_ascii_simd`, `simd_ascii_delete_mask`, `skip_non_digit_ascii_simd`) for 16-byte parallel probing in `SingleChar` skip loops.

### Features
- Exhaustive property-based and unit tests for `Fanjian`, `Delete`, `Normalize`, and `PinYin` process types.
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
- A bunch of changes and I don't want to explain one by one.

## 0.4.6 - 2024-07-15

### Performance
- Optimize performance.

## 0.4.5 - 2024-07-12

### Changed
- Optimize Simple Matcher `process` function when multiple simple_match_type are used.
- add `dfa` feature to matcher_rs.
- shrink `FANJIAN` conversion map.

## 0.4.4 - 2024-07-09

### Changed
- Merge PINYIN and PINYINCHAR process matcher build.
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
- Update FANJIAN.txt and NORM.txt.
- Fix issues with `runtime_build` feature.

## 0.4.2 - 2024-07-07

### Changed
- Optimize performance.

## 0.4.1 - 2024-07-06

### Changed
- Rebuild Transformation Rules based on Unicode Standard.

## 0.4.0 - 2024-07-03

### Changed
- Implement NOT logic word-wise inside SimpleMatcher, now you can use `&`(and) and `~`(not) separator to config simple word, eg: `hello&world~helo`.
