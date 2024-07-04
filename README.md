# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

A high-performance matcher for massive amounts of sensitive words.

Designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching. For detailed implementation, see the [Design Document](./DESIGN.md).

It's helpful for
- **Content Filtering**: Detecting and filtering out offensive or sensitive words.
- **Search Engines**: Improving search results by identifying relevant keywords.
- **Text Analysis**: Extracting specific information from large volumes of text.
- **Spam Detection**: Identifying spam content in emails or messages.
- ···

## Features

- **Multiple Matching Methods**:
  - Simple Word Matching
  - Regex-Based Matching
  - Similarity-Based Matching
- **Text Normalization**:
  - **Fanjian**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `𝜢𝕰𝕃𝙻Ϙ 𝙒ⓞƦℒ𝒟!` -> `hello world!`
  - **PinYin**: Convert Chinese characters to Pinyin for fuzzy matching.
    Example: `西安` -> `/xi//an/`, matches `洗按` -> `/xi//an/`, but not `先` -> `/xian/`
  - **PinYinChar**: Convert Chinese characters to Pinyin.
    Example: `西安` -> `xian`, matches `洗按` and `先` -> `xian`
- **AND OR NOT Word Matching**:
  - Takes into account the number of repetitions of words.
  - Example: `hello&world` matches `hello world` and `world,hello`
  - Example: `无&法&无&天` matches `无无法天` (because `无` is repeated twice), but not `无法天`
  - Example: `hello~helloo~hhello` matches `hello` but not `helloo` and `hhello`
- **Customizable Exemption Lists**: Exclude specific words from matching.
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Usage

Non-Rust users must use **msgpack** for serializing matcher configurations to bytes. **Why msgpack?** It handles backslashes better and is faster than JSON.
  - Example issue with JSON: `It's /\/\y duty` is processed incorrectly.

### Rust Users

See the [Rust README](./matcher_rs/README.md).

### Python Users

See the [Python README](./matcher_py/README.md).

### C, Java and Other Users

We provide dynamic library to link. See the [C README](./matcher_c/README.md) and [Java README](./matcher_java/README.md).

#### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

Then you should find the `libmatcher_c.so`/`libmatcher_c.dylib`/`matcher_c.dll` in the `target/release` directory.

#### Pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Benchmarks

Bench against pairs ([CN_WORD_LIST_100000](./data/word_list/cn/cn_words_100000.txt), [CN_HAYSTACK](./data/text/cn/西游记.txt)) and ([EN_WORD_LIST_100000](./data/word_list/en/en_words_100000.txt), [EN_HAYSTACK](./data/text/en/sherlock.txt)). Word selection is totally random.

The `matcher_rs` library includes benchmarks to measure the performance of the matcher. You can find the benchmarks in the [bench.rs](./benches/bench.rs) file. To run the benchmarks, use the following command:

```shell
cargo bench
```

```
Current default simple match type: SimpleMatchType(None)
Current default simple word map size: 1000
Current default combined times: 2
bench                                               fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build_cn                                                       │               │               │               │         │
│  ├─ build_cn_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          2.398 ms      │ 5.506 ms      │ 2.438 ms      │ 2.612 ms      │ 100     │ 100
│  │  ├─ 2                                          5.139 ms      │ 5.798 ms      │ 5.523 ms      │ 5.482 ms      │ 100     │ 100
│  │  ├─ 3                                          8.307 ms      │ 8.735 ms      │ 8.451 ms      │ 8.448 ms      │ 100     │ 100
│  │  ├─ 4                                          10.46 ms      │ 11.72 ms      │ 10.6 ms       │ 10.74 ms      │ 100     │ 100
│  │  ╰─ 5                                          12.97 ms      │ 28.22 ms      │ 13.38 ms      │ 13.68 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_simple_match_type        16.84 ms      │ 56.57 ms      │ 17.8 ms       │ 18.59 ms      │ 100     │ 100
│  ├─ build_cn_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "fanjian"                                  5.262 ms      │ 21.68 ms      │ 5.727 ms      │ 6.024 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  5.625 ms      │ 6.146 ms      │ 5.846 ms      │ 5.864 ms      │ 100     │ 100
│  │  ├─ "none"                                     5.284 ms      │ 17 ms         │ 5.598 ms      │ 5.863 ms      │ 100     │ 100
│  │  ├─ "normalize"                                5.642 ms      │ 6.283 ms      │ 5.87 ms       │ 5.933 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   28.24 ms      │ 35.92 ms      │ 29.12 ms      │ 29.43 ms      │ 100     │ 100
│  │  ├─ "pinyinchar"                               15.62 ms      │ 36.97 ms      │ 16.14 ms      │ 16.78 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    5.428 ms      │ 6.606 ms      │ 5.727 ms      │ 5.764 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          5.723 ms      │ 20.46 ms      │ 5.908 ms      │ 6.168 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        461.4 µs      │ 1.027 ms      │ 498.9 µs      │ 511.4 µs      │ 100     │ 100
│     ├─ 1000                                       5.274 ms      │ 5.932 ms      │ 5.575 ms      │ 5.568 ms      │ 100     │ 100
│     ├─ 10000                                      50.65 ms      │ 85.7 ms       │ 52.37 ms      │ 53.28 ms      │ 94      │ 94
│     ╰─ 50000                                      214.9 ms      │ 252.9 ms      │ 224 ms        │ 225.7 ms      │ 23      │ 23
├─ build_en                                                       │               │               │               │         │
│  ├─ build_en_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          6.251 ms      │ 6.978 ms      │ 6.601 ms      │ 6.63 ms       │ 100     │ 100
│  │  ├─ 2                                          13.71 ms      │ 15.32 ms      │ 14.5 ms       │ 14.5 ms       │ 100     │ 100
│  │  ├─ 3                                          20.4 ms       │ 37.57 ms      │ 21.9 ms       │ 22.08 ms      │ 100     │ 100
│  │  ├─ 4                                          27.99 ms      │ 31.3 ms       │ 28.8 ms       │ 29 ms         │ 100     │ 100
│  │  ╰─ 5                                          37.21 ms      │ 78.67 ms      │ 38.8 ms       │ 40.66 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_simple_match_type        16.65 ms      │ 18.83 ms      │ 17.14 ms      │ 17.33 ms      │ 100     │ 100
│  ├─ build_en_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "none"                                     13.35 ms      │ 15.41 ms      │ 14.03 ms      │ 14.11 ms      │ 100     │ 100
│  │  ├─ "normalize"                                15.87 ms      │ 17.84 ms      │ 16.44 ms      │ 16.46 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    13.32 ms      │ 15.45 ms      │ 14.12 ms      │ 14.12 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          16.65 ms      │ 21.88 ms      │ 17.32 ms      │ 17.41 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        876.5 µs      │ 1.111 ms      │ 934.6 µs      │ 941.1 µs      │ 100     │ 100
│     ├─ 1000                                       13.19 ms      │ 36.92 ms      │ 14.04 ms      │ 14.37 ms      │ 100     │ 100
│     ├─ 10000                                      170.8 ms      │ 211.5 ms      │ 177.6 ms      │ 179.3 ms      │ 28      │ 28
│     ╰─ 50000                                      779.8 ms      │ 915.5 ms      │ 802.1 ms      │ 822.1 ms      │ 7       │ 7
├─ search_cn                                                      │               │               │               │         │
│  ├─ search_cn_baseline                                          │               │               │               │         │
│  │  ├─ 100                                        2.83 ms       │ 4.104 ms      │ 3.015 ms      │ 3.018 ms      │ 100     │ 100
│  │  ├─ 1000                                       2.941 ms      │ 3.232 ms      │ 3.008 ms      │ 3.059 ms      │ 100     │ 100
│  │  ├─ 10000                                      8.549 ms      │ 9.309 ms      │ 8.735 ms      │ 8.74 ms       │ 100     │ 100
│  │  ╰─ 50000                                      30.02 ms      │ 39.24 ms      │ 33.18 ms      │ 33.3 ms       │ 100     │ 100
│  ├─ search_cn_by_combined_times                                 │               │               │               │         │
│  │  ├─ 1                                          3.75 ms       │ 18.02 ms      │ 4.037 ms      │ 4.337 ms      │ 100     │ 100
│  │  ├─ 2                                          5.272 ms      │ 24.82 ms      │ 5.5 ms        │ 5.879 ms      │ 100     │ 100
│  │  ├─ 3                                          6.739 ms      │ 22.92 ms      │ 7.218 ms      │ 7.585 ms      │ 100     │ 100
│  │  ├─ 4                                          6.781 ms      │ 8.221 ms      │ 7.052 ms      │ 7.151 ms      │ 100     │ 100
│  │  ╰─ 5                                          8.21 ms       │ 9.886 ms      │ 8.644 ms      │ 8.67 ms       │ 100     │ 100
│  ├─ search_cn_by_multiple_simple_match_type       53.18 ms      │ 101.5 ms      │ 58.52 ms      │ 59.38 ms      │ 100     │ 100
│  ├─ search_cn_by_simple_match_type                              │               │               │               │         │
│  │  ├─ "fanjian"                                  6.764 ms      │ 8.779 ms      │ 7.278 ms      │ 7.317 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  20.56 ms      │ 38.22 ms      │ 21.63 ms      │ 22.18 ms      │ 100     │ 100
│  │  ├─ "none"                                     4.949 ms      │ 7.812 ms      │ 5.118 ms      │ 5.437 ms      │ 100     │ 100
│  │  ├─ "normalize"                                12.15 ms      │ 26.63 ms      │ 12.84 ms      │ 12.99 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   62.18 ms      │ 95.55 ms      │ 66.06 ms      │ 67.79 ms      │ 74      │ 74
│  │  ├─ "pinyinchar"                               55.58 ms      │ 121.5 ms      │ 57.91 ms      │ 59.71 ms      │ 84      │ 84
│  │  ├─ "worddelete_textdelete"                    13.68 ms      │ 14.9 ms       │ 14.1 ms       │ 14.21 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          19.73 ms      │ 37.62 ms      │ 20.3 ms       │ 20.84 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                           │               │               │               │         │
│     ├─ 100                                        2.918 ms      │ 3.351 ms      │ 3.142 ms      │ 3.1 ms        │ 100     │ 100
│     ├─ 1000                                       5.678 ms      │ 6.097 ms      │ 5.747 ms      │ 5.761 ms      │ 100     │ 100
│     ├─ 10000                                      19.97 ms      │ 35.1 ms       │ 22.2 ms       │ 23.24 ms      │ 100     │ 100
│     ╰─ 50000                                      69.94 ms      │ 124 ms        │ 79.35 ms      │ 81.99 ms      │ 61      │ 61
╰─ search_en                                                      │               │               │               │         │
   ├─ search_en_baseline                                          │               │               │               │         │
   │  ├─ 100                                        223.2 µs      │ 446.1 µs      │ 248.8 µs      │ 255 µs        │ 100     │ 100
   │  ├─ 1000                                       243.2 µs      │ 335.4 µs      │ 270.9 µs      │ 272.3 µs      │ 100     │ 100
   │  ├─ 10000                                      882.9 µs      │ 1.048 ms      │ 951.7 µs      │ 954.5 µs      │ 100     │ 100
   │  ╰─ 50000                                      898.1 µs      │ 1.065 ms      │ 964.5 µs      │ 969.5 µs      │ 100     │ 100
   ├─ search_en_by_combined_times                                 │               │               │               │         │
   │  ├─ 1                                          1.313 ms      │ 1.912 ms      │ 1.414 ms      │ 1.426 ms      │ 100     │ 100
   │  ├─ 2                                          1.634 ms      │ 1.895 ms      │ 1.766 ms      │ 1.742 ms      │ 100     │ 100
   │  ├─ 3                                          2.266 ms      │ 2.802 ms      │ 2.377 ms      │ 2.388 ms      │ 100     │ 100
   │  ├─ 4                                          2.382 ms      │ 3.813 ms      │ 2.574 ms      │ 2.569 ms      │ 100     │ 100
   │  ╰─ 5                                          2.384 ms      │ 3.436 ms      │ 2.444 ms      │ 2.534 ms      │ 100     │ 100
   ├─ search_en_by_multiple_simple_match_type       10.17 ms      │ 32.13 ms      │ 10.54 ms      │ 11.11 ms      │ 100     │ 100
   ├─ search_en_by_simple_match_type                              │               │               │               │         │
   │  ├─ "none"                                     2.257 ms      │ 3.474 ms      │ 2.321 ms      │ 2.362 ms      │ 100     │ 100
   │  ├─ "normalize"                                3.894 ms      │ 4.299 ms      │ 3.989 ms      │ 4.008 ms      │ 100     │ 100
   │  ├─ "worddelete_textdelete"                    5.925 ms      │ 7.733 ms      │ 6.069 ms      │ 6.113 ms      │ 100     │ 100
   │  ╰─ "worddelete_textdelete_normalize"          7.342 ms      │ 10.04 ms      │ 7.658 ms      │ 7.848 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                           │               │               │               │         │
      ├─ 100                                        988 µs        │ 1.469 ms      │ 1.095 ms      │ 1.118 ms      │ 100     │ 100
      ├─ 1000                                       2.028 ms      │ 15.76 ms      │ 2.188 ms      │ 2.475 ms      │ 100     │ 100
      ├─ 10000                                      2.9 ms        │ 6.907 ms      │ 3.118 ms      │ 3.311 ms      │ 100     │ 100
      ╰─ 50000                                      4.049 ms      │ 6.268 ms      │ 4.33 ms       │ 4.356 ms      │ 100     │ 100
```

## Roadmap
- [x] Cache get_process_matcher results globally, instead cache result inside SimpleMatcher.
- [x] Expose reduce_process_text to Python.
- [x] ~~Cache middle results during different SimpleMatchType reduce_process_text function calling. (failed, too slow)~~
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] Try more aho_corasick library to improve performance and reduce memory usage.
  - [x] ~~https://github.com/daac-tools/crawdad (produce char-wise index, not byte-wise index, it's not acceptable)~~
  - [x] https://github.com/daac-tools/daachorse (use it when Fanjian, PinYin or PinYinChar transformation is performed)
  - [ ] Test char-wise HashMap transformation for Chinese Characters.
- [x] Add a new function that can handle single simple match type.
  - [x] `text_process` now is available.
- [x] More detailed simple match type explanation.
- [x] Add fuzzy matcher, https://github.com/lotabout/fuzzy-matcher.
  - [x] Use `rapidfuzz` instead.
- [x] More precise and convenient MatchTable.
- [x] Make SimpleMatcher and Matcher serializable.
  - [x] Make aho-corasick serializable.
  - [x] See https://github.com/Lips7/aho-corasick.
- [x] Implement NOT logic word-wise.
- [ ] More detailed [DESIGN](./DESIGN.md).
- [x] Support stable rust.
- [x] Unsafe aho-corasick crate implement.
  - [x] Faster and faster!
  - [x] See https://github.com/Lips7/aho-corasick.
- [ ] Support iterator.
- [ ] Optimize NOT logic word-wise.