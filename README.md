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
    Example: `西安` -> ` xi  an `, matches `洗按` -> ` xi  an `, but not `先` -> ` xian `
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
│  │  ├─ 1                                          2.468 ms      │ 3.355 ms      │ 2.506 ms      │ 2.536 ms      │ 100     │ 100
│  │  ├─ 2                                          5.303 ms      │ 5.765 ms      │ 5.402 ms      │ 5.41 ms       │ 100     │ 100
│  │  ├─ 3                                          7.912 ms      │ 10.16 ms      │ 7.986 ms      │ 8.081 ms      │ 100     │ 100
│  │  ├─ 4                                          10.59 ms      │ 11.31 ms      │ 10.73 ms      │ 10.75 ms      │ 100     │ 100
│  │  ╰─ 5                                          13.03 ms      │ 14.1 ms       │ 13.13 ms      │ 13.21 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_simple_match_type        26.63 ms      │ 40.81 ms      │ 26.99 ms      │ 27.23 ms      │ 100     │ 100
│  ├─ build_cn_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "fanjian"                                  5.296 ms      │ 6.12 ms       │ 5.348 ms      │ 5.398 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  5.43 ms       │ 5.937 ms      │ 5.47 ms       │ 5.491 ms      │ 100     │ 100
│  │  ├─ "none"                                     5.268 ms      │ 5.667 ms      │ 5.375 ms      │ 5.379 ms      │ 100     │ 100
│  │  ├─ "normalize"                                5.373 ms      │ 5.827 ms      │ 5.423 ms      │ 5.437 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   16.02 ms      │ 24.52 ms      │ 16.15 ms      │ 16.34 ms      │ 100     │ 100
│  │  ├─ "pinyinchar"                               15.81 ms      │ 41.81 ms      │ 16.29 ms      │ 16.99 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    5.291 ms      │ 6.192 ms      │ 5.409 ms      │ 5.556 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          5.38 ms       │ 6.311 ms      │ 5.897 ms      │ 5.866 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        501.2 µs      │ 838.9 µs      │ 545.2 µs      │ 559.5 µs      │ 100     │ 100
│     ├─ 1000                                       5.383 ms      │ 18.63 ms      │ 5.669 ms      │ 5.88 ms       │ 100     │ 100
│     ├─ 10000                                      49.97 ms      │ 99.73 ms      │ 53.03 ms      │ 54.13 ms      │ 93      │ 93
│     ╰─ 50000                                      194.1 ms      │ 366.2 ms      │ 204.9 ms      │ 212.6 ms      │ 24      │ 24
├─ build_en                                                       │               │               │               │         │
│  ├─ build_en_by_combined_times                                  │               │               │               │         │
│  │  ├─ 1                                          5.43 ms       │ 6.427 ms      │ 5.84 ms       │ 5.907 ms      │ 100     │ 100
│  │  ├─ 2                                          12.9 ms       │ 21.5 ms       │ 13.6 ms       │ 13.83 ms      │ 100     │ 100
│  │  ├─ 3                                          21.99 ms      │ 24.19 ms      │ 22.89 ms      │ 22.8 ms       │ 100     │ 100
│  │  ├─ 4                                          29.3 ms       │ 50.2 ms       │ 30.84 ms      │ 31.27 ms      │ 100     │ 100
│  │  ╰─ 5                                          38.12 ms      │ 40.88 ms      │ 38.44 ms      │ 38.58 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_simple_match_type        16.43 ms      │ 19 ms         │ 16.79 ms      │ 16.95 ms      │ 100     │ 100
│  ├─ build_en_by_simple_match_type                               │               │               │               │         │
│  │  ├─ "none"                                     13.97 ms      │ 15.1 ms       │ 14.56 ms      │ 14.58 ms      │ 100     │ 100
│  │  ├─ "normalize"                                12.35 ms      │ 17.97 ms      │ 13.05 ms      │ 13.13 ms      │ 100     │ 100
│  │  ├─ "worddelete_textdelete"                    13.5 ms       │ 14.87 ms      │ 13.96 ms      │ 13.97 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          11.83 ms      │ 13.31 ms      │ 12.46 ms      │ 12.54 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                            │               │               │               │         │
│     ├─ 100                                        848.1 µs      │ 1.286 ms      │ 925.4 µs      │ 929 µs        │ 100     │ 100
│     ├─ 1000                                       12.57 ms      │ 16.46 ms      │ 13.38 ms      │ 13.38 ms      │ 100     │ 100
│     ├─ 10000                                      178.1 ms      │ 192.3 ms      │ 182.2 ms      │ 183.7 ms      │ 28      │ 28
│     ╰─ 50000                                      743.3 ms      │ 884.1 ms      │ 752.2 ms      │ 776.2 ms      │ 7       │ 7
├─ search_cn                                                      │               │               │               │         │
│  ├─ search_cn_baseline                                          │               │               │               │         │
│  │  ├─ 100                                        2.907 ms      │ 11.87 ms      │ 3.068 ms      │ 3.359 ms      │ 100     │ 100
│  │  ├─ 1000                                       2.99 ms       │ 3.422 ms      │ 3.006 ms      │ 3.033 ms      │ 100     │ 100
│  │  ├─ 10000                                      5.197 ms      │ 5.801 ms      │ 5.269 ms      │ 5.294 ms      │ 100     │ 100
│  │  ╰─ 50000                                      12.44 ms      │ 16.52 ms      │ 14.2 ms       │ 13.89 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                                 │               │               │               │         │
│  │  ├─ 1                                          3.702 ms      │ 4.091 ms      │ 3.728 ms      │ 3.749 ms      │ 100     │ 100
│  │  ├─ 2                                          4.442 ms      │ 4.826 ms      │ 4.458 ms      │ 4.467 ms      │ 100     │ 100
│  │  ├─ 3                                          5.054 ms      │ 5.595 ms      │ 5.078 ms      │ 5.093 ms      │ 100     │ 100
│  │  ├─ 4                                          6.136 ms      │ 6.777 ms      │ 6.159 ms      │ 6.177 ms      │ 100     │ 100
│  │  ╰─ 5                                          6.235 ms      │ 11.38 ms      │ 6.396 ms      │ 6.51 ms       │ 100     │ 100
│  ├─ search_cn_by_multiple_simple_match_type       64.81 ms      │ 80.83 ms      │ 66.49 ms      │ 66.75 ms      │ 100     │ 100
│  ├─ search_cn_by_simple_match_type                              │               │               │               │         │
│  │  ├─ "fanjian"                                  6.781 ms      │ 7.486 ms      │ 6.841 ms      │ 6.927 ms      │ 100     │ 100
│  │  ├─ "fanjian_worddelete_textdelete_normalize"  21.47 ms      │ 45.61 ms      │ 21.82 ms      │ 22.33 ms      │ 100     │ 100
│  │  ├─ "none"                                     4.684 ms      │ 5.198 ms      │ 4.705 ms      │ 4.731 ms      │ 100     │ 100
│  │  ├─ "normalize"                                14.62 ms      │ 15.81 ms      │ 15.5 ms       │ 15.28 ms      │ 100     │ 100
│  │  ├─ "pinyin"                                   57.98 ms      │ 63.66 ms      │ 60.31 ms      │ 59.92 ms      │ 84      │ 84
│  │  ├─ "pinyinchar"                               63.8 ms       │ 74.02 ms      │ 65.47 ms      │ 66.22 ms      │ 76      │ 76
│  │  ├─ "worddelete_textdelete"                    13.2 ms       │ 14.62 ms      │ 13.43 ms      │ 13.65 ms      │ 100     │ 100
│  │  ╰─ "worddelete_textdelete_normalize"          18.97 ms      │ 21.06 ms      │ 19.73 ms      │ 19.83 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                           │               │               │               │         │
│     ├─ 100                                        3.031 ms      │ 3.491 ms      │ 3.082 ms      │ 3.104 ms      │ 100     │ 100
│     ├─ 1000                                       4.793 ms      │ 5.205 ms      │ 4.997 ms      │ 5.001 ms      │ 100     │ 100
│     ├─ 10000                                      10.12 ms      │ 12.74 ms      │ 10.7 ms       │ 10.66 ms      │ 100     │ 100
│     ╰─ 50000                                      21.12 ms      │ 27.96 ms      │ 21.77 ms      │ 23.13 ms      │ 100     │ 100
╰─ search_en                                                      │               │               │               │         │
   ├─ search_en_baseline                                          │               │               │               │         │
   │  ├─ 100                                        328.3 µs      │ 1.576 ms      │ 343.1 µs      │ 364.5 µs      │ 100     │ 100
   │  ├─ 1000                                       343.6 µs      │ 472.4 µs      │ 369.9 µs      │ 369.1 µs      │ 100     │ 100
   │  ├─ 10000                                      1.169 ms      │ 1.248 ms      │ 1.197 ms      │ 1.199 ms      │ 100     │ 100
   │  ╰─ 50000                                      1.193 ms      │ 1.304 ms      │ 1.199 ms      │ 1.205 ms      │ 100     │ 100
   ├─ search_en_by_combined_times                                 │               │               │               │         │
   │  ├─ 1                                          1.682 ms      │ 4.053 ms      │ 1.692 ms      │ 1.727 ms      │ 100     │ 100
   │  ├─ 2                                          2.481 ms      │ 2.682 ms      │ 2.502 ms      │ 2.506 ms      │ 100     │ 100
   │  ├─ 3                                          2.585 ms      │ 2.979 ms      │ 2.678 ms      │ 2.69 ms       │ 100     │ 100
   │  ├─ 4                                          2.654 ms      │ 3.265 ms      │ 2.761 ms      │ 2.764 ms      │ 100     │ 100
   │  ╰─ 5                                          2.74 ms       │ 3.242 ms      │ 2.752 ms      │ 2.761 ms      │ 100     │ 100
   ├─ search_en_by_multiple_simple_match_type       9.173 ms      │ 10.27 ms      │ 9.351 ms      │ 9.481 ms      │ 100     │ 100
   ├─ search_en_by_simple_match_type                              │               │               │               │         │
   │  ├─ "none"                                     1.99 ms       │ 2.286 ms      │ 2.006 ms      │ 2.049 ms      │ 100     │ 100
   │  ├─ "normalize"                                3.992 ms      │ 4.064 ms      │ 4.009 ms      │ 4.012 ms      │ 100     │ 100
   │  ├─ "worddelete_textdelete"                    6.198 ms      │ 7.005 ms      │ 6.225 ms      │ 6.253 ms      │ 100     │ 100
   │  ╰─ "worddelete_textdelete_normalize"          10.51 ms      │ 32.63 ms      │ 11.1 ms       │ 11.41 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                           │               │               │               │         │
      ├─ 100                                        1.384 ms      │ 1.616 ms      │ 1.458 ms      │ 1.471 ms      │ 100     │ 100
      ├─ 1000                                       2.395 ms      │ 2.587 ms      │ 2.427 ms      │ 2.432 ms      │ 100     │ 100
      ├─ 10000                                      3.091 ms      │ 4.291 ms      │ 3.113 ms      │ 3.127 ms      │ 100     │ 100
      ╰─ 50000                                      3.668 ms      │ 5.738 ms      │ 3.831 ms      │ 3.853 ms      │ 100     │ 100
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
- [x] ~~Optimize process matcher when perform reduce text processing.~~
  1. Consider we have to perform FanjianDeleteNormalize, we need to perform Fanjian first, then Delete, then Normalize, 3 kinds of Process Matcher are needed to perform replacement or delete, the text has to be scanned 3 times.
  2. What if we only construct only 1 Process Matcher which's patterns contains all the Fanjian, Delete and Normalize 3 kinds of patterns? We could scan the text only once to get all the positions that should be perform replacement or delete.
  3. We need to take care of the byte index will change after replacement or delete, so we need to take the offset changes into account.
- [x] Merge multiple aho-corasick matcher into one when multiple simple match types are used.
- [x] When `dfa` feature is disabled, use daachorse to perform text processing.
  - [x] Do not use it for simple process, too slow to build.

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
- [ ] Customize str conversion map.
- [x] Add Matcher process function to py, c and java.
- [ ] For simple matcher, is it possible to use regex-automata to replace aho-corasick? and support regex.

### Readability
- [x] More precise and convenient MatchTable.
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] More detailed simple match type explanation.
- [ ] More detailed [DESIGN](./DESIGN.md).
- [ ] Write a Chinese README.