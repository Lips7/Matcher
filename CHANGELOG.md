# Changelog

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
