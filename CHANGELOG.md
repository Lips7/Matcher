# Changelog

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
