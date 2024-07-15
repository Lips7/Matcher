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
│  │  ├─ 1                                          2.445 ms      │ 3.004 ms      │ 2.516 ms      │ 2.536 ms      │ 100     │ 100
│  │  ├─ 2                                          5.24 ms       │ 5.606 ms      │ 5.294 ms      │ 5.3 ms        │ 100     │ 100
│  │  ├─ 3                                          8.127 ms      │ 9.515 ms      │ 8.398 ms      │ 8.456 ms      │ 100     │ 100
│  │  ├─ 4                                          10.51 ms      │ 50.54 ms      │ 11.27 ms      │ 11.74 ms      │ 100     │ 100
│  │  ╰─ 5                                          13.22 ms      │ 25.06 ms      │ 13.65 ms      │ 13.88 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_simple_match_type        27.99 ms      │ 38.42 ms      │ 28.58 ms      │ 28.74 ms      │ 100     │ 100
│  ├─ build_cn_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "fanjian"                                  5.313 ms      │ 5.726 ms      │ 5.445 ms      │ 5.464 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  5.528 ms      │ 5.912 ms      │ 5.607 ms      │ 5.612 ms      │ 100     │ 100
│  │  ├─ "none"                                     5.28 ms       │ 5.844 ms      │ 5.515 ms      │ 5.503 ms      │ 100     │ 100
│  │  ├─ "normalize"                                5.453 ms      │ 5.965 ms      │ 5.653 ms      │ 5.667 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   16.39 ms      │ 27.83 ms      │ 16.81 ms      │ 17.01 ms      │ 100     │ 100
│  │  ├─ "pinyinchar"                               16.25 ms      │ 18.55 ms      │ 16.75 ms      │ 16.86 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    5.383 ms      │ 9.107 ms      │ 5.529 ms      │ 5.572 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          5.488 ms      │ 5.976 ms      │ 5.675 ms      │ 5.672 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        487.4 µs      │ 774 µs        │ 535 µs        │ 537.1 µs      │ 100     │ 100
│     ├─ 1000                                       5.203 ms      │ 6.004 ms      │ 5.31 ms       │ 5.363 ms      │ 100     │ 100
│     ├─ 10000                                      50.44 ms      │ 65.39 ms      │ 51.55 ms      │ 52.07 ms      │ 97      │ 97
│     ╰─ 50000                                      194 ms        │ 212.4 ms      │ 201 ms        │ 201 ms        │ 25      │ 25
├─ build_en                                                       │               │               │               │         │
│  ├─ build_en_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          5.496 ms      │ 27.82 ms      │ 5.798 ms      │ 6.405 ms      │ 100     │ 100
│  │  ├─ 2                                          12.63 ms      │ 14.09 ms      │ 13.29 ms      │ 13.25 ms      │ 100     │ 100
│  │  ├─ 3                                          21.94 ms      │ 23.56 ms      │ 22.2 ms       │ 22.3 ms       │ 100     │ 100
│  │  ├─ 4                                          29.54 ms      │ 73.17 ms      │ 30.67 ms      │ 31.6 ms       │ 100     │ 100
│  │  ╰─ 5                                          38.82 ms      │ 90.39 ms      │ 39.5 ms       │ 40.09 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_simple_match_type        16.46 ms      │ 18.91 ms      │ 17.06 ms      │ 17.17 ms      │ 100     │ 100
│  ├─ build_en_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "none"                                     13.43 ms      │ 25.77 ms      │ 13.97 ms      │ 14.12 ms      │ 100     │ 100
│  │  ├─ "normalize"                                11.52 ms      │ 13.47 ms      │ 12.39 ms      │ 12.36 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    12.53 ms      │ 13.46 ms      │ 13.03 ms      │ 13.02 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          11.91 ms      │ 54.05 ms      │ 12.59 ms      │ 13.07 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        942.8 µs      │ 1.234 ms      │ 978.4 µs      │ 999.1 µs      │ 100     │ 100
│     ├─ 1000                                       12.08 ms      │ 13.42 ms      │ 12.7 ms       │ 12.65 ms      │ 100     │ 100
│     ├─ 10000                                      173.4 ms      │ 228.4 ms      │ 178.9 ms      │ 182.9 ms      │ 28      │ 28
│     ╰─ 50000                                      749.1 ms      │ 797.2 ms      │ 764.6 ms      │ 768.4 ms      │ 7       │ 7
├─ search_cn                                                      │               │               │               │         │
│  ├─ search_cn_baseline                                          │               │               │               │         │
│  │  ├─ 100                                        3.019 ms      │ 3.274 ms      │ 3.037 ms      │ 3.045 ms      │ 100     │ 100
│  │  ├─ 1000                                       2.958 ms      │ 3.402 ms      │ 2.992 ms      │ 3.011 ms      │ 100     │ 100
│  │  ├─ 10000                                      9.016 ms      │ 10.35 ms      │ 9.186 ms      │ 9.25 ms       │ 100     │ 100
│  │  ╰─ 50000                                      32.66 ms      │ 50.9 ms       │ 33.31 ms      │ 33.75 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                                 │               │               │               │         │
│  │  ├─ 1                                          4.082 ms      │ 4.815 ms      │ 4.146 ms      │ 4.247 ms      │ 100     │ 100
│  │  ├─ 2                                          5.25 ms       │ 6.151 ms      │ 5.614 ms      │ 5.578 ms      │ 100     │ 100
│  │  ├─ 3                                          6.923 ms      │ 49.44 ms      │ 7.129 ms      │ 7.772 ms      │ 100     │ 100
│  │  ├─ 4                                          7.52 ms       │ 8.945 ms      │ 8.005 ms      │ 8.005 ms      │ 100     │ 100
│  │  ╰─ 5                                          7.892 ms      │ 9.423 ms      │ 8.139 ms      │ 8.32 ms       │ 100     │ 100
│  ├─ search_cn_by_multiple_simple_match_type       71.63 ms      │ 92.02 ms      │ 75.63 ms      │ 76.22 ms      │ 100     │ 100
│  ├─ search_cn_by_simple_match_type                              │               │               │               │         │
│  │  ├─ "fanjian"                                  7.002 ms      │ 7.41 ms       │ 7.182 ms      │ 7.187 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  17.77 ms      │ 28.42 ms      │ 18.42 ms      │ 18.61 ms      │ 100     │ 100
│  │  ├─ "none"                                     5.39 ms       │ 5.743 ms      │ 5.514 ms      │ 5.526 ms      │ 100     │ 100
│  │  ├─ "normalize"                                10.78 ms      │ 43.1 ms       │ 11.01 ms      │ 11.47 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   53.95 ms      │ 69.5 ms       │ 54.88 ms      │ 55.33 ms      │ 91      │ 91
│  │  ├─ "pinyinchar"                               62.93 ms      │ 74.38 ms      │ 63.95 ms      │ 64.9 ms       │ 78      │ 78
│  │  ├─ "worddelete_textdelete"                    13.98 ms      │ 24.26 ms      │ 14.75 ms      │ 14.9 ms       │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          17.1 ms       │ 22.19 ms      │ 18.14 ms      │ 18.09 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                           │               │               │               │         │
│     ├─ 100                                        2.964 ms      │ 3.463 ms      │ 3.031 ms      │ 3.055 ms      │ 100     │ 100
│     ├─ 1000                                       5.459 ms      │ 5.778 ms      │ 5.494 ms      │ 5.512 ms      │ 100     │ 100
│     ├─ 10000                                      19.03 ms      │ 21.74 ms      │ 19.42 ms      │ 19.55 ms      │ 100     │ 100
│     ╰─ 50000                                      74.22 ms      │ 87.68 ms      │ 76.62 ms      │ 77.09 ms      │ 65      │ 65
╰─ search_en                                                      │               │               │               │         │
   ├─ search_en_baseline                                          │               │               │               │         │
   │  ├─ 100                                        231.5 µs      │ 363.1 µs      │ 252.2 µs      │ 257.4 µs      │ 100     │ 100
   │  ├─ 1000                                       250.8 µs      │ 381.1 µs      │ 277.6 µs      │ 281.6 µs      │ 100     │ 100
   │  ├─ 10000                                      869.7 µs      │ 1.041 ms      │ 932.4 µs      │ 936.6 µs      │ 100     │ 100
   │  ╰─ 50000                                      925.5 µs      │ 972.9 µs      │ 930.2 µs      │ 933.2 µs      │ 100     │ 100
   ├─ search_en_by_combined_times                                 │               │               │               │         │
   │  ├─ 1                                          1.307 ms      │ 1.568 ms      │ 1.404 ms      │ 1.383 ms      │ 100     │ 100
   │  ├─ 2                                          1.648 ms      │ 1.914 ms      │ 1.722 ms      │ 1.74 ms       │ 100     │ 100
   │  ├─ 3                                          2.299 ms      │ 2.662 ms      │ 2.47 ms       │ 2.438 ms      │ 100     │ 100
   │  ├─ 4                                          2.339 ms      │ 2.949 ms      │ 2.4 ms        │ 2.43 ms       │ 100     │ 100
   │  ╰─ 5                                          2.436 ms      │ 3.159 ms      │ 2.631 ms      │ 2.616 ms      │ 100     │ 100
   ├─ search_en_by_multiple_simple_match_type       12.74 ms      │ 18.66 ms      │ 12.82 ms      │ 12.97 ms      │ 100     │ 100
   ├─ search_en_by_simple_match_type                              │               │               │               │         │
   │  ├─ "none"                                     1.691 ms      │ 14.03 ms      │ 1.812 ms      │ 2.207 ms      │ 100     │ 100
   │  ├─ "normalize"                                2.829 ms      │ 4.028 ms      │ 3.045 ms      │ 3.071 ms      │ 100     │ 100
   │  ├─ "worddelete_textdelete"                    5.648 ms      │ 35.35 ms      │ 6.115 ms      │ 6.561 ms      │ 100     │ 100
   │  ╰─ "worddelete_textdelete_normalize"          6.221 ms      │ 7.296 ms      │ 6.641 ms      │ 6.655 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                           │               │               │               │         │
      ├─ 100                                        1.008 ms      │ 1.192 ms      │ 1.076 ms      │ 1.079 ms      │ 100     │ 100
      ├─ 1000                                       2.197 ms      │ 2.384 ms      │ 2.22 ms       │ 2.224 ms      │ 100     │ 100
      ├─ 10000                                      3.211 ms      │ 4.464 ms      │ 3.23 ms       │ 3.244 ms      │ 100     │ 100
      ╰─ 50000                                      4.971 ms      │ 7.22 ms       │ 5.065 ms      │ 5.081 ms      │ 100     │ 100
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
- [x] Optimize simple matcher when multiple simple match types are used.
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
- [x] Merge multiple aho-corasick matcher into one when multiple simple match types are used.

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
- [ ] Customize str conv map.
- [x] Add Matcher process function to py, c and java.
- [ ] For simple matcher, is it possible to use regex-automata to replace aho-corasick? and support regex.

### Readability
- [x] More precise and convenient MatchTable.
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] More detailed simple match type explanation.
- [ ] More detailed [DESIGN](./DESIGN.md).
- [ ] Write a Chinese README.