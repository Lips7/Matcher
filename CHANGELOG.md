# Changelog

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
