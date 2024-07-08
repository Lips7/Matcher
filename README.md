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
    Example: `蟲艸` -> `虫草`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `𝜢𝕰𝕃𝙻𝝧 𝙒ⓞᵣℒ𝒟!` -> `hello world!`
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
Timer precision: 41 ns
bench                                               fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build_cn                                                       │               │               │               │         │
│  ├─ build_cn_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          2.411 ms      │ 3.148 ms      │ 2.543 ms      │ 2.542 ms      │ 100     │ 100
│  │  ├─ 2                                          5.268 ms      │ 5.628 ms      │ 5.308 ms      │ 5.318 ms      │ 100     │ 100
│  │  ├─ 3                                          7.833 ms      │ 8.757 ms      │ 8.033 ms      │ 8.082 ms      │ 100     │ 100
│  │  ├─ 4                                          10.36 ms      │ 16.95 ms      │ 10.75 ms      │ 10.89 ms      │ 100     │ 100
│  │  ╰─ 5                                          12.91 ms      │ 14 ms         │ 13.14 ms      │ 13.2 ms       │ 100     │ 100
│  ├─ build_cn_by_multiple_simple_match_type        16.67 ms      │ 75.17 ms      │ 17.23 ms      │ 18.19 ms      │ 100     │ 100
│  ├─ build_cn_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "fanjian"                                  5.263 ms      │ 6.504 ms      │ 5.727 ms      │ 5.719 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  5.688 ms      │ 6.144 ms      │ 5.751 ms      │ 5.768 ms      │ 100     │ 100
│  │  ├─ "none"                                     5.229 ms      │ 5.533 ms      │ 5.287 ms      │ 5.295 ms      │ 100     │ 100
│  │  ├─ "normalize"                                5.081 ms      │ 13.27 ms      │ 5.251 ms      │ 5.4 ms        │ 100     │ 100
│  │  ├─ "pinyin"                                   28.37 ms      │ 40.86 ms      │ 29.45 ms      │ 29.54 ms      │ 100     │ 100
│  │  ├─ "pinyinchar"                               15.52 ms      │ 17.1 ms       │ 15.75 ms      │ 15.81 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    5.3 ms        │ 5.666 ms      │ 5.359 ms      │ 5.369 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          5.281 ms      │ 10.33 ms      │ 5.416 ms      │ 5.555 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        468.9 µs      │ 630.7 µs      │ 506.3 µs      │ 509.7 µs      │ 100     │ 100
│     ├─ 1000                                       5.065 ms      │ 6.205 ms      │ 5.152 ms      │ 5.249 ms      │ 100     │ 100
│     ├─ 10000                                      49.95 ms      │ 61.21 ms      │ 51.37 ms      │ 51.78 ms      │ 97      │ 97
│     ╰─ 50000                                      185.9 ms      │ 205.1 ms      │ 190.8 ms      │ 192 ms        │ 27      │ 27
├─ build_en                                                       │               │               │               │         │
│  ├─ build_en_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          5.965 ms      │ 6.952 ms      │ 6.16 ms       │ 6.177 ms      │ 100     │ 100
│  │  ├─ 2                                          13.53 ms      │ 24.89 ms      │ 14.08 ms      │ 14.18 ms      │ 100     │ 100
│  │  ├─ 3                                          21.58 ms      │ 22.98 ms      │ 21.92 ms      │ 21.99 ms      │ 100     │ 100
│  │  ├─ 4                                          29.43 ms      │ 40.54 ms      │ 30.19 ms      │ 30.5 ms       │ 100     │ 100
│  │  ╰─ 5                                          37.01 ms      │ 50.59 ms      │ 37.75 ms      │ 37.96 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_simple_match_type        16.26 ms      │ 20.51 ms      │ 16.34 ms      │ 16.43 ms      │ 100     │ 100
│  ├─ build_en_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "none"                                     12.61 ms      │ 20.9 ms       │ 13.04 ms      │ 13.13 ms      │ 100     │ 100
│  │  ├─ "normalize"                                11.87 ms      │ 13.03 ms      │ 12.33 ms      │ 12.21 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    12.41 ms      │ 13.24 ms      │ 12.78 ms      │ 12.74 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          12.6 ms       │ 23.45 ms      │ 12.96 ms      │ 13.07 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        821.4 µs      │ 1.074 ms      │ 880 µs        │ 886.2 µs      │ 100     │ 100
│     ├─ 1000                                       12.82 ms      │ 26.07 ms      │ 13.3 ms       │ 13.48 ms      │ 100     │ 100
│     ├─ 10000                                      164.8 ms      │ 179 ms        │ 168.7 ms      │ 169.4 ms      │ 30      │ 30
│     ╰─ 50000                                      732.9 ms      │ 769.2 ms      │ 744.8 ms      │ 747.2 ms      │ 7       │ 7
├─ search_cn                                                      │               │               │               │         │
│  ├─ search_cn_baseline                                          │               │               │               │         │
│  │  ├─ 100                                        2.814 ms      │ 3.043 ms      │ 2.97 ms       │ 2.953 ms      │ 100     │ 100
│  │  ├─ 1000                                       2.994 ms      │ 3.434 ms      │ 3.171 ms      │ 3.146 ms      │ 100     │ 100
│  │  ├─ 10000                                      8.954 ms      │ 9.901 ms      │ 9.006 ms      │ 9.053 ms      │ 100     │ 100
│  │  ╰─ 50000                                      31.95 ms      │ 47.99 ms      │ 32.92 ms      │ 33.28 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                                 │               │               │               │         │
│  │  ├─ 1                                          3.954 ms      │ 4.253 ms      │ 4.119 ms      │ 4.112 ms      │ 100     │ 100
│  │  ├─ 2                                          5.733 ms      │ 15.43 ms      │ 6.134 ms      │ 6.209 ms      │ 100     │ 100
│  │  ├─ 3                                          6.917 ms      │ 7.764 ms      │ 6.967 ms      │ 6.985 ms      │ 100     │ 100
│  │  ├─ 4                                          6.596 ms      │ 7.489 ms      │ 6.649 ms      │ 6.666 ms      │ 100     │ 100
│  │  ╰─ 5                                          8.324 ms      │ 9.099 ms      │ 8.371 ms      │ 8.39 ms       │ 100     │ 100
│  ├─ search_cn_by_multiple_simple_match_type       50.94 ms      │ 66.2 ms       │ 51.14 ms      │ 51.88 ms      │ 100     │ 100
│  ├─ search_cn_by_simple_match_type                              │               │               │               │         │
│  │  ├─ "fanjian"                                  8.353 ms      │ 8.783 ms      │ 8.401 ms      │ 8.413 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  19.95 ms      │ 31.51 ms      │ 20.31 ms      │ 20.72 ms      │ 100     │ 100
│  │  ├─ "none"                                     4.908 ms      │ 5.399 ms      │ 5.108 ms      │ 5.115 ms      │ 100     │ 100
│  │  ├─ "normalize"                                9.632 ms      │ 10.78 ms      │ 9.677 ms      │ 9.706 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   62.56 ms      │ 79.62 ms      │ 63.77 ms      │ 65.14 ms      │ 77      │ 77
│  │  ├─ "pinyinchar"                               54.22 ms      │ 67.27 ms      │ 55 ms         │ 55.62 ms      │ 90      │ 90
│  │  ├─ "worddelete_textdelete"                    13.13 ms      │ 13.97 ms      │ 13.17 ms      │ 13.2 ms       │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          17.27 ms      │ 27.16 ms      │ 18.46 ms      │ 18.07 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                           │               │               │               │         │
│     ├─ 100                                        2.914 ms      │ 3.151 ms      │ 2.931 ms      │ 2.937 ms      │ 100     │ 100
│     ├─ 1000                                       5.374 ms      │ 5.699 ms      │ 5.528 ms      │ 5.525 ms      │ 100     │ 100
│     ├─ 10000                                      17.89 ms      │ 27.38 ms      │ 19.08 ms      │ 18.94 ms      │ 100     │ 100
│     ╰─ 50000                                      66.72 ms      │ 81.68 ms      │ 68.4 ms       │ 69.01 ms      │ 73      │ 73
╰─ search_en                                                      │               │               │               │         │
   ├─ search_en_baseline                                          │               │               │               │         │
   │  ├─ 100                                        233.6 µs      │ 378.4 µs      │ 249.8 µs      │ 252.9 µs      │ 100     │ 100
   │  ├─ 1000                                       241.7 µs      │ 355.2 µs      │ 261.3 µs      │ 265.1 µs      │ 100     │ 100
   │  ├─ 10000                                      861.4 µs      │ 997.3 µs      │ 927.5 µs      │ 926.6 µs      │ 100     │ 100
   │  ╰─ 50000                                      864.6 µs      │ 946.3 µs      │ 926.4 µs      │ 927.1 µs      │ 100     │ 100
   ├─ search_en_by_combined_times                                 │               │               │               │         │
   │  ├─ 1                                          1.332 ms      │ 1.55 ms       │ 1.344 ms      │ 1.351 ms      │ 100     │ 100
   │  ├─ 2                                          2.176 ms      │ 2.417 ms      │ 2.187 ms      │ 2.195 ms      │ 100     │ 100
   │  ├─ 3                                          2.273 ms      │ 2.601 ms      │ 2.286 ms      │ 2.293 ms      │ 100     │ 100
   │  ├─ 4                                          2.401 ms      │ 2.991 ms      │ 2.559 ms      │ 2.531 ms      │ 100     │ 100
   │  ╰─ 5                                          2.539 ms      │ 2.982 ms      │ 2.548 ms      │ 2.557 ms      │ 100     │ 100
   ├─ search_en_by_multiple_simple_match_type       9.411 ms      │ 18.13 ms      │ 9.461 ms      │ 9.572 ms      │ 100     │ 100
   ├─ search_en_by_simple_match_type                              │               │               │               │         │
   │  ├─ "none"                                     2.042 ms      │ 2.545 ms      │ 2.121 ms      │ 2.129 ms      │ 100     │ 100
   │  ├─ "normalize"                                2.589 ms      │ 2.773 ms      │ 2.609 ms      │ 2.614 ms      │ 100     │ 100
   │  ├─ "worddelete_textdelete"                    5.161 ms      │ 5.614 ms      │ 5.316 ms      │ 5.324 ms      │ 100     │ 100
   │  ╰─ "worddelete_textdelete_normalize"          5.647 ms      │ 30.43 ms      │ 5.98 ms       │ 6.273 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                           │               │               │               │         │
      ├─ 100                                        984.7 µs      │ 1.166 ms      │ 1.055 ms      │ 1.041 ms      │ 100     │ 100
      ├─ 1000                                       2.066 ms      │ 2.272 ms      │ 2.078 ms      │ 2.086 ms      │ 100     │ 100
      ├─ 10000                                      2.971 ms      │ 4.241 ms      │ 2.988 ms      │ 3.01 ms       │ 100     │ 100
      ╰─ 50000                                      4.268 ms      │ 6.721 ms      │ 4.601 ms      │ 4.705 ms      │ 100     │ 100
```

## Roadmap

### Performance
- [x] ~~Cache middle results during different SimpleMatchType reduce_process_text function calling. (failed, too slow)~~
- [x] Try more aho-corasick library to improve performance and reduce memory usage.
  - [x] ~~https://github.com/daac-tools/crawdad (produce char-wise index, not byte-wise index, it's not acceptable)~~
  - [x] https://github.com/daac-tools/daachorse (use it when Fanjian, PinYin or PinYinChar transformation is performed)
  - [x] ~~Test char-wise HashMap transformation for Chinese Characters. (Too slow)~~
- [x] Make aho-corasick unsafe.
  - [x] See https://github.com/Lips7/aho-corasick.
- [ ] Optimize NOT logic word-wise.
- [x] Optimize regex matcher using RegexSet.
- [ ] Optimize simple matcher when multiple simple match types are used.
  1. Consider if there are multiple simple match types
   * None
   * Fanjian
   * FanjianDelete
   * FanjianDeleteNormalize
   * FanjianNormalize
  2. We can construct a chain of transformations,
   * None -> Fanjian -> Delete -> Normalize
   * &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;\ -> Normalize.
  3. Calcuate all possible transformations, and cache the results, so that instead calculating 8 times (Fanjian, Fanjian + Delete, Fanjian + Delete + Normalize, Fanjian + Normalize), we only need to calculate 4 times.
- [ ] Optimize process matcher when perform reduce text processing.
  1. Consider we have to perform FanjianDeleteNormalize, we need to perform Fanjian first, then Delete, then Normalize, 3 kinds of Process Matcher are needed to perform replacement or delete, the text has to be scanned 3 times.
  2. What if we only construct only 1 Process Matcher which's patterns contains all the Fanjian, Delete and Normalize 3 kinds of patterns? We could scan the text only once to get all the positions that should be perform replacement or delete.
  3. We need to take care of the byte index will change after replacement or delete, so we need to take the offset changes into account.

### Flexibility
- [x] Cache get_process_matcher results globally, instead of caching result inside SimpleMatcher.
- [x] Expose reduce_process_text to Python.
- [x] Add a new function that can handle single simple match type.
  - [x] `text_process` now is available.
- [x] Add fuzzy matcher, https://github.com/lotabout/fuzzy-matcher.
  - [x] Use `rapidfuzz` instead.
- [x] Make SimpleMatcher and Matcher serializable.
  - [x] Make aho-corasick serializable.
  - [x] See https://github.com/Lips7/aho-corasick.
- [x] Implement NOT logic word-wise.
- [x] Support stable rust.
- [ ] Support iterator.
- [ ] A real java package.
- [x] Multiple Python version wheel build.

### Readability
- [x] More precise and convenient MatchTable.
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] More detailed simple match type explanation.
- [ ] More detailed [DESIGN](./DESIGN.md).