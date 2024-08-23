# Matcher

A high-performance matcher designed to solve **LOGICAL and TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

- **Multiple Matching Methods**:
  - Simple Word Matching
  - Regex-Based Matching
  - Similarity-Based Matching
- **Text Transformation**:
  - **Fanjian**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
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

### Adding to Your Project

To use `matcher_rs` in your Rust project, run the following command:

```shell
cargo add matcher_rs
```

### Explanation of the configuration

* `Matcher`'s configuration is defined by the `MatchTableMap = HashMap<u32, Vec<MatchTable>>` type, the key of `MatchTableMap` is called `match_id`, **for each `match_id`, the `table_id` inside is required to be unique**.
* `SimpleMatcher`'s configuration is defined by the `SimpleTable = HashMap<ProcessType, HashMap<u32, &str>>` type, the value `HashMap<u32, &str>`'s key is called `word_id`, **`word_id` is required to be globally unique**.

#### MatchTable

* `table_id`: The unique ID of the match table.
* `match_table_type`: The type of the match table.
* `word_list`: The word list of the match table.
* `exemption_process_type`: The type of the exemption simple match.
* `exemption_word_list`: The exemption word list of the match table.

For each match table, word matching is performed over the `word_list`, and exemption word matching is performed over the `exemption_word_list`. If the exemption word matching result is True, the word matching result will be False.

#### MatchTableType

* `Simple`: Supports simple multiple patterns matching with text normalization defined by `process_type`.
  * It can handle combination patterns and repeated times sensitive matching, delimited by `&` and `~`, such as `hello&world&hello` will match `hellohelloworld` and `worldhellohello`, but not `helloworld` due to the repeated times of `hello`.
* `Regex`: Supports regex patterns matching.
  * `SimilarChar`: Supports similar character matching using regex.
    * `["hello,hallo,hollo,hi", "word,world,wrd,🌍", "!,?,~"]` will match `helloworld!`, `hollowrd?`, `hi🌍~` ··· any combinations of the words split by `,` in the list.
  * `Acrostic`: Supports acrostic matching using regex **(currently only supports Chinese and simple English sentences)**.
    * `["h,e,l,l,o", "你,好"]` will match `hope, endures, love, lasts, onward.` and `你的笑容温暖, 好心情常伴。`.
  * `Regex`: Supports regex matching.
    * `["h[aeiou]llo", "w[aeiou]rd"]` will match `hello`, `world`, `hillo`, `wurld` ··· any text that matches the regex in the list.
* `Similar`: Supports similar text matching based on distance and threshold.
  * `Levenshtein`: Supports similar text matching based on Levenshtein distance.

#### ProcessType

* `None`: No transformation.
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](./process_map/FANJIAN.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces. Based on [TEXT_DELETE](./process_map/TEXT-DELETE.txt) and `WHITE_SPACE`.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [NORM](./process_map/NORM.txt) and [NUM_NORM](./process_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠Õ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](./process_map/PINYIN.txt).
  * `你好` -> ` ni  hao `
  * `西安` -> ` xi  an `
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN](./process_map/PINYIN.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

### Basic Example

Here’s a basic example of how to use the `Matcher` struct for text matching:

```rust
use matcher_rs::{text_process, reduce_text_process, ProcessType};

let result = text_process(ProcessType::Delete, "你好，世界！");
let result = reduce_text_process(ProcessType::FanjianDeleteNormalize, "你好，世界！");
```

```rust
use std::collections::HashMap;
use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, ProcessType};

let match_table_map: MatchTableMap = HashMap::from_iter(vec![
    (1, vec![MatchTable {
        table_id: 1,
        match_table_type: MatchTableType::Simple { process_type: ProcessType::FanjianDeleteNormalize},
        word_list: vec!["example", "test"],
        exemption_process_type: ProcessType::None,
        exemption_word_list: vec![],
    }]),
]);
let matcher = Matcher::new(&match_table_map);
let text = "This is an example text.";
let results = matcher.word_match(text);
```

```rust
use std::collections::HashMap;
use matcher_rs::{ProcessType, SimpleMatcher};

let mut simple_table = HashMap::new();
let mut simple_word_map = HashMap::new();

simple_word_map.insert(1, "你好");
simple_word_map.insert(2, "世界");

simple_table.insert(ProcessType::Fanjian, simple_word_map);

let matcher = SimpleMatcher::new(&simple_table);
let text = "你好，世界！";
let results = matcher.process(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Feature Flags
* `runtime_build`: By enable runtime_build feature, we could build process matcher at runtime, but with build time increasing.
* `serde`: By enable serde feature, we could serialize and deserialize matcher and simple_matcher. With serde feature, AhoCorasick's prefilter is disabled, because I don't know how to serialize it correctly, which will lead to performance regression when the patterns size is small (say, less than 100).
* `dfa`: By enable dfa feature, we could use dfa to perform simple matching, but with significantly increasing memory consumption.

Default feature is `dfa`. If you want to make `Matcher` and `SimpleMatcher` serializable, you should enable `serde` feature.

## Benchmarks

Bench against pairs ([CN_WORD_LIST_100000](../data/word_list/cn/cn_words_100000.txt), [CN_HAYSTACK](../data/text/cn/西游记.txt)) and ([EN_WORD_LIST_100000](../data/word_list/en/en_words_100000.txt), [EN_HAYSTACK](../data/text/en/sherlock.txt)). Word selection is totally random.

The `matcher_rs` library includes benchmarks to measure the performance of the matcher. You can find the benchmarks in the [bench.rs](./benches/bench.rs) file. To run the benchmarks, use the following command:

```shell
cargo bench
```

```
Current default simple match type: ProcessType(None)
Current default simple word map size: 1000
Current default combined times: 2
Timer precision: 41 ns
bench                                     fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build_cn                                             │               │               │               │         │
│  ├─ build_cn_by_combined_times                        │               │               │               │         │
│  │  ├─ 1                                2.5 ms        │ 9.171 ms      │ 2.621 ms      │ 2.975 ms      │ 100     │ 100
│  │  ├─ 2                                5.088 ms      │ 6.116 ms      │ 5.407 ms      │ 5.447 ms      │ 100     │ 100
│  │  ├─ 3                                7.761 ms      │ 8.842 ms      │ 7.904 ms      │ 7.954 ms      │ 100     │ 100
│  │  ├─ 4                                10.27 ms      │ 12.08 ms      │ 10.86 ms      │ 10.87 ms      │ 100     │ 100
│  │  ╰─ 5                                12.83 ms      │ 13.96 ms      │ 13.27 ms      │ 13.34 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_process_type   25.63 ms      │ 49.57 ms      │ 26.19 ms      │ 27.25 ms      │ 100     │ 100
│  ├─ build_cn_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         5.162 ms      │ 6.166 ms      │ 5.458 ms      │ 5.521 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               5.359 ms      │ 6.164 ms      │ 5.599 ms      │ 5.7 ms        │ 100     │ 100
│  │  ├─ "fanjian"                        5.18 ms       │ 18.05 ms      │ 5.364 ms      │ 5.686 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       5.342 ms      │ 5.605 ms      │ 5.413 ms      │ 5.427 ms      │ 100     │ 100
│  │  ├─ "none"                           5.206 ms      │ 6.014 ms      │ 5.404 ms      │ 5.466 ms      │ 100     │ 100
│  │  ├─ "normalize"                      5.136 ms      │ 6.022 ms      │ 5.313 ms      │ 5.413 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         7.15 ms       │ 10.4 ms       │ 7.749 ms      │ 7.776 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     6.56 ms       │ 8.648 ms      │ 6.875 ms      │ 6.9 ms        │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              462.1 µs      │ 640.2 µs      │ 497.6 µs      │ 503.9 µs      │ 100     │ 100
│     ├─ 1000                             5.205 ms      │ 6.055 ms      │ 5.444 ms      │ 5.511 ms      │ 100     │ 100
│     ├─ 10000                            49.3 ms       │ 75.97 ms      │ 51.22 ms      │ 51.94 ms      │ 97      │ 97
│     ╰─ 50000                            185.7 ms      │ 207.6 ms      │ 194.1 ms      │ 194.3 ms      │ 26      │ 26
├─ build_en                                             │               │               │               │         │
│  ├─ build_en_by_combined_times                        │               │               │               │         │
│  │  ├─ 1                                5.982 ms      │ 7.846 ms      │ 6.418 ms      │ 6.451 ms      │ 100     │ 100
│  │  ├─ 2                                12.64 ms      │ 14.05 ms      │ 13.41 ms      │ 13.37 ms      │ 100     │ 100
│  │  ├─ 3                                20.83 ms      │ 72.35 ms      │ 21.57 ms      │ 23.43 ms      │ 100     │ 100
│  │  ├─ 4                                28.75 ms      │ 31.95 ms      │ 29.36 ms      │ 29.54 ms      │ 100     │ 100
│  │  ╰─ 5                                37.31 ms      │ 62.69 ms      │ 37.61 ms      │ 38.02 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_process_type   15.42 ms      │ 29.2 ms       │ 16.09 ms      │ 16.42 ms      │ 100     │ 100
│  ├─ build_en_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         12.93 ms      │ 14.65 ms      │ 13.55 ms      │ 13.59 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               11.15 ms      │ 12.97 ms      │ 11.31 ms      │ 11.38 ms      │ 100     │ 100
│  │  ├─ "none"                           12.94 ms      │ 14.11 ms      │ 13.49 ms      │ 13.51 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      11.36 ms      │ 12.83 ms      │ 12.15 ms      │ 12.11 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              901.1 µs      │ 1.268 ms      │ 977.2 µs      │ 1.004 ms      │ 100     │ 100
│     ├─ 1000                             12.13 ms      │ 32.91 ms      │ 12.91 ms      │ 13.15 ms      │ 100     │ 100
│     ├─ 10000                            159.3 ms      │ 193 ms        │ 165 ms        │ 166.5 ms      │ 31      │ 31
│     ╰─ 50000                            712 ms        │ 857.7 ms      │ 716.7 ms      │ 739.5 ms      │ 7       │ 7
├─ search_cn                                            │               │               │               │         │
│  ├─ search_cn_baseline                                │               │               │               │         │
│  │  ├─ 100                              2.927 ms      │ 4.723 ms      │ 3.239 ms      │ 3.251 ms      │ 100     │ 100
│  │  ├─ 1000                             3.084 ms      │ 3.915 ms      │ 3.406 ms      │ 3.426 ms      │ 100     │ 100
│  │  ├─ 10000                            8.098 ms      │ 9.623 ms      │ 8.314 ms      │ 8.372 ms      │ 100     │ 100
│  │  ╰─ 50000                            27.34 ms      │ 40.26 ms      │ 29.6 ms       │ 30.57 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                4 ms          │ 4.618 ms      │ 4.304 ms      │ 4.296 ms      │ 100     │ 100
│  │  ├─ 2                                5.097 ms      │ 5.676 ms      │ 5.446 ms      │ 5.422 ms      │ 100     │ 100
│  │  ├─ 3                                6.164 ms      │ 6.73 ms       │ 6.192 ms      │ 6.29 ms       │ 100     │ 100
│  │  ├─ 4                                6.948 ms      │ 8.172 ms      │ 7.438 ms      │ 7.314 ms      │ 100     │ 100
│  │  ╰─ 5                                9.285 ms      │ 9.946 ms      │ 9.777 ms      │ 9.766 ms      │ 100     │ 100
│  ├─ search_cn_by_multiple_process_type  61.99 ms      │ 94.96 ms      │ 65.04 ms      │ 65.7 ms       │ 100     │ 100
│  ├─ search_cn_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         14.19 ms      │ 15.32 ms      │ 15.19 ms      │ 14.95 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               21.86 ms      │ 26.01 ms      │ 21.91 ms      │ 21.99 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        7.295 ms      │ 7.861 ms      │ 7.337 ms      │ 7.372 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       23.07 ms      │ 25.89 ms      │ 24.36 ms      │ 24.27 ms      │ 100     │ 100
│  │  ├─ "none"                           5.173 ms      │ 5.502 ms      │ 5.207 ms      │ 5.214 ms      │ 100     │ 100
│  │  ├─ "normalize"                      14.36 ms      │ 15.34 ms      │ 14.48 ms      │ 14.49 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         42.33 ms      │ 43.75 ms      │ 42.43 ms      │ 42.46 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     42.16 ms      │ 43.93 ms      │ 42.32 ms      │ 42.38 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              3.002 ms      │ 3.243 ms      │ 3.017 ms      │ 3.026 ms      │ 100     │ 100
│     ├─ 1000                             5.248 ms      │ 5.677 ms      │ 5.421 ms      │ 5.426 ms      │ 100     │ 100
│     ├─ 10000                            15.51 ms      │ 18.43 ms      │ 15.7 ms       │ 15.79 ms      │ 100     │ 100
│     ╰─ 50000                            52.89 ms      │ 64.13 ms      │ 55.85 ms      │ 55.99 ms      │ 90      │ 90
├─ search_en                                            │               │               │               │         │
│  ├─ search_en_baseline                                │               │               │               │         │
│  │  ├─ 100                              350.2 µs      │ 547.6 µs      │ 376.5 µs      │ 382.5 µs      │ 100     │ 100
│  │  ├─ 1000                             360.4 µs      │ 463.8 µs      │ 386 µs        │ 388.3 µs      │ 100     │ 100
│  │  ├─ 10000                            1.014 ms      │ 1.045 ms      │ 1.02 ms       │ 1.022 ms      │ 100     │ 100
│  │  ╰─ 50000                            1.015 ms      │ 1.051 ms      │ 1.02 ms       │ 1.021 ms      │ 100     │ 100
│  ├─ search_en_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                1.787 ms      │ 2.475 ms      │ 1.808 ms      │ 1.831 ms      │ 100     │ 100
│  │  ├─ 2                                2.519 ms      │ 2.772 ms      │ 2.528 ms      │ 2.535 ms      │ 100     │ 100
│  │  ├─ 3                                2.58 ms       │ 2.926 ms      │ 2.6 ms        │ 2.609 ms      │ 100     │ 100
│  │  ├─ 4                                2.816 ms      │ 3.299 ms      │ 2.827 ms      │ 2.837 ms      │ 100     │ 100
│  │  ╰─ 5                                2.753 ms      │ 3.387 ms      │ 2.768 ms      │ 2.778 ms      │ 100     │ 100
│  ├─ search_en_by_multiple_process_type  10.65 ms      │ 11.94 ms      │ 10.68 ms      │ 10.72 ms      │ 100     │ 100
│  ├─ search_en_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         7.012 ms      │ 7.4 ms        │ 7.106 ms      │ 7.112 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               8.678 ms      │ 9.234 ms      │ 8.787 ms      │ 8.785 ms      │ 100     │ 100
│  │  ├─ "none"                           2.085 ms      │ 2.373 ms      │ 2.222 ms      │ 2.223 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      3.919 ms      │ 4.235 ms      │ 4.179 ms      │ 4.175 ms      │ 100     │ 100
│  ╰─ search_en_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              1.361 ms      │ 1.625 ms      │ 1.447 ms      │ 1.438 ms      │ 100     │ 100
│     ├─ 1000                             2.446 ms      │ 2.802 ms      │ 2.617 ms      │ 2.583 ms      │ 100     │ 100
│     ├─ 10000                            3.166 ms      │ 4.672 ms      │ 3.281 ms      │ 3.298 ms      │ 100     │ 100
│     ╰─ 50000                            5.981 ms      │ 8.647 ms      │ 6.054 ms      │ 6.101 ms      │ 100     │ 100
╰─ single_line                                          │               │               │               │         │
   ├─ search_cn_single_line                             │               │               │               │         │
   │  ├─ 100                              259.7 ns      │ 275.3 ns      │ 267.5 ns      │ 267.7 ns      │ 100     │ 1600
   │  ├─ 1000                             314.3 ns      │ 335.2 ns      │ 319.6 ns      │ 320.1 ns      │ 100     │ 1600
   │  ├─ 10000                            499.3 ns      │ 12.24 µs      │ 582.3 ns      │ 711.4 ns      │ 100     │ 100
   │  ╰─ 50000                            1.249 µs      │ 26.66 µs      │ 1.333 µs      │ 1.673 µs      │ 100     │ 100
   ╰─ search_en_single_line                             │               │               │               │         │
      ├─ 100                              56.28 ns      │ 61.17 ns      │ 56.93 ns      │ 57.85 ns      │ 100     │ 12800
      ├─ 1000                             60.18 ns      │ 61.82 ns      │ 60.84 ns      │ 60.74 ns      │ 100     │ 12800
      ├─ 10000                            332.3 ns      │ 5.249 µs      │ 416.3 ns      │ 477.6 ns      │ 100     │ 100
      ╰─ 50000                            457.3 ns      │ 15.2 µs       │ 540.3 ns      │ 706.8 ns      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
