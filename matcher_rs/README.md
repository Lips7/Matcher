# Matcher

A high-performance, multi-functional word matcher implemented in Rust.

Designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching. For detailed implementation, see the [Design Document](../DESIGN.md).

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
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [SYMBOL_NORM](./process_map/SYMBOL-NORM.txt), [NORM](./process_map/NORM.txt) and [NUM_NORM](./process_map/NUM-NORM.txt).
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
        exemption_process_type: ProcessType::FanjianDeleteNormalize,
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
│  │  ├─ 1                                2.593 ms      │ 5.474 ms      │ 2.672 ms      │ 2.803 ms      │ 100     │ 100
│  │  ├─ 2                                5.259 ms      │ 6.592 ms      │ 5.438 ms      │ 5.537 ms      │ 100     │ 100
│  │  ├─ 3                                7.982 ms      │ 10.01 ms      │ 8.591 ms      │ 8.7 ms        │ 100     │ 100
│  │  ├─ 4                                10.59 ms      │ 65.93 ms      │ 11.86 ms      │ 12.82 ms      │ 100     │ 100
│  │  ╰─ 5                                13.46 ms      │ 16.05 ms      │ 14.18 ms      │ 14.36 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_process_type   27.6 ms       │ 43.1 ms       │ 28.34 ms      │ 28.83 ms      │ 100     │ 100
│  ├─ build_cn_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         5.332 ms      │ 6.308 ms      │ 5.525 ms      │ 5.597 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               5.394 ms      │ 6.605 ms      │ 5.601 ms      │ 5.618 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        5.33 ms       │ 5.739 ms      │ 5.428 ms      │ 5.467 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       5.485 ms      │ 6.35 ms       │ 5.724 ms      │ 5.791 ms      │ 100     │ 100
│  │  ├─ "none"                           5.439 ms      │ 6.201 ms      │ 5.545 ms      │ 5.612 ms      │ 100     │ 100
│  │  ├─ "normalize"                      5.351 ms      │ 6.041 ms      │ 5.662 ms      │ 5.662 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         6.996 ms      │ 9.993 ms      │ 7.254 ms      │ 7.284 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     7.056 ms      │ 8.977 ms      │ 7.415 ms      │ 7.449 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              520.4 µs      │ 912.2 µs      │ 562.9 µs      │ 568.7 µs      │ 100     │ 100
│     ├─ 1000                             5.184 ms      │ 6.008 ms      │ 5.369 ms      │ 5.415 ms      │ 100     │ 100
│     ├─ 10000                            51.18 ms      │ 61.37 ms      │ 53.76 ms      │ 53.82 ms      │ 93      │ 93
│     ╰─ 50000                            190.9 ms      │ 213.9 ms      │ 196.4 ms      │ 197.6 ms      │ 26      │ 26
├─ build_en                                             │               │               │               │         │
│  ├─ build_en_by_combined_times                        │               │               │               │         │
│  │  ├─ 1                                6.323 ms      │ 7.754 ms      │ 6.504 ms      │ 6.531 ms      │ 100     │ 100
│  │  ├─ 2                                13.82 ms      │ 15.83 ms      │ 14.19 ms      │ 14.23 ms      │ 100     │ 100
│  │  ├─ 3                                20.42 ms      │ 24.58 ms      │ 21.29 ms      │ 21.38 ms      │ 100     │ 100
│  │  ├─ 4                                28.54 ms      │ 31.17 ms      │ 29.12 ms      │ 29.21 ms      │ 100     │ 100
│  │  ╰─ 5                                37.47 ms      │ 40.15 ms      │ 38.64 ms      │ 38.68 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_process_type   16.1 ms       │ 17.82 ms      │ 16.67 ms      │ 16.7 ms       │ 100     │ 100
│  ├─ build_en_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         12.54 ms      │ 14.42 ms      │ 13.19 ms      │ 13.24 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               11.16 ms      │ 12.49 ms      │ 11.45 ms      │ 11.52 ms      │ 100     │ 100
│  │  ├─ "none"                           13.2 ms       │ 14.31 ms      │ 13.57 ms      │ 13.59 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      12.02 ms      │ 13.74 ms      │ 12.52 ms      │ 12.54 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              938.9 µs      │ 1.257 ms      │ 1.007 ms      │ 1.013 ms      │ 100     │ 100
│     ├─ 1000                             13.53 ms      │ 15.2 ms       │ 14.04 ms      │ 14.03 ms      │ 100     │ 100
│     ├─ 10000                            160.5 ms      │ 174.9 ms      │ 164.1 ms      │ 165.2 ms      │ 31      │ 31
│     ╰─ 50000                            689.6 ms      │ 817.3 ms      │ 719 ms        │ 727.6 ms      │ 7       │ 7
├─ search_cn                                            │               │               │               │         │
│  ├─ search_cn_baseline                                │               │               │               │         │
│  │  ├─ 100                              2.907 ms      │ 4.152 ms      │ 2.945 ms      │ 3.033 ms      │ 100     │ 100
│  │  ├─ 1000                             3.081 ms      │ 3.266 ms      │ 3.153 ms      │ 3.162 ms      │ 100     │ 100
│  │  ├─ 10000                            9.386 ms      │ 10.59 ms      │ 9.733 ms      │ 9.708 ms      │ 100     │ 100
│  │  ╰─ 50000                            33.38 ms      │ 42.97 ms      │ 35.56 ms      │ 36.28 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                4.148 ms      │ 4.967 ms      │ 4.181 ms      │ 4.219 ms      │ 100     │ 100
│  │  ├─ 2                                5.601 ms      │ 6.266 ms      │ 5.751 ms      │ 5.773 ms      │ 100     │ 100
│  │  ├─ 3                                6.85 ms       │ 8.021 ms      │ 7.243 ms      │ 7.282 ms      │ 100     │ 100
│  │  ├─ 4                                7.382 ms      │ 8.841 ms      │ 7.734 ms      │ 7.773 ms      │ 100     │ 100
│  │  ╰─ 5                                8.952 ms      │ 12.99 ms      │ 10.04 ms      │ 9.958 ms      │ 100     │ 100
│  ├─ search_cn_by_multiple_process_type  66.7 ms       │ 148.4 ms      │ 75.71 ms      │ 78.7 ms       │ 100     │ 100
│  ├─ search_cn_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         14.13 ms      │ 17.09 ms      │ 15.15 ms      │ 15.17 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               20.14 ms      │ 24.2 ms       │ 21.53 ms      │ 21.72 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        7.07 ms       │ 8.242 ms      │ 7.478 ms      │ 7.474 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       22.36 ms      │ 24.46 ms      │ 23.33 ms      │ 23.32 ms      │ 100     │ 100
│  │  ├─ "none"                           5.852 ms      │ 6.8 ms        │ 6.244 ms      │ 6.208 ms      │ 100     │ 100
│  │  ├─ "normalize"                      14.11 ms      │ 17.09 ms      │ 14.89 ms      │ 14.99 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         55.21 ms      │ 140.7 ms      │ 56.74 ms      │ 58.11 ms      │ 87      │ 87
│  │  ╰─ "pinyinchar"                     57.37 ms      │ 151.5 ms      │ 61.23 ms      │ 65.84 ms      │ 76      │ 76
│  ╰─ search_cn_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              3.16 ms       │ 5.387 ms      │ 3.499 ms      │ 3.64 ms       │ 100     │ 100
│     ├─ 1000                             5.66 ms       │ 7.839 ms      │ 6.457 ms      │ 6.504 ms      │ 100     │ 100
│     ├─ 10000                            22.55 ms      │ 90.1 ms       │ 28.91 ms      │ 29.91 ms      │ 100     │ 100
│     ╰─ 50000                            75.08 ms      │ 122.5 ms      │ 87.05 ms      │ 90.99 ms      │ 55      │ 55
╰─ search_en                                            │               │               │               │         │
   ├─ search_en_baseline                                │               │               │               │         │
   │  ├─ 100                              343.4 µs      │ 593.2 µs      │ 380.9 µs      │ 389.2 µs      │ 100     │ 100
   │  ├─ 1000                             355.1 µs      │ 472.7 µs      │ 389.7 µs      │ 393.1 µs      │ 100     │ 100
   │  ├─ 10000                            1.213 ms      │ 1.554 ms      │ 1.27 ms       │ 1.291 ms      │ 100     │ 100
   │  ╰─ 50000                            1.194 ms      │ 1.342 ms      │ 1.201 ms      │ 1.209 ms      │ 100     │ 100
   ├─ search_en_by_combined_times                       │               │               │               │         │
   │  ├─ 1                                1.698 ms      │ 2.499 ms      │ 1.883 ms      │ 1.914 ms      │ 100     │ 100
   │  ├─ 2                                2.066 ms      │ 3.646 ms      │ 2.321 ms      │ 2.391 ms      │ 100     │ 100
   │  ├─ 3                                2.628 ms      │ 3.176 ms      │ 2.8 ms        │ 2.81 ms       │ 100     │ 100
   │  ├─ 4                                2.879 ms      │ 4.266 ms      │ 3.153 ms      │ 3.259 ms      │ 100     │ 100
   │  ╰─ 5                                2.748 ms      │ 3.31 ms       │ 2.785 ms      │ 2.812 ms      │ 100     │ 100
   ├─ search_en_by_multiple_process_type  9.42 ms       │ 12.25 ms      │ 9.974 ms      │ 10.16 ms      │ 100     │ 100
   ├─ search_en_by_process_type                         │               │               │               │         │
   │  ├─ "delete"                         6.613 ms      │ 8.215 ms      │ 7.027 ms      │ 7.208 ms      │ 100     │ 100
   │  ├─ "delete_normalize"               7.938 ms      │ 9.425 ms      │ 8.116 ms      │ 8.215 ms      │ 100     │ 100
   │  ├─ "none"                           2.648 ms      │ 16.51 ms      │ 2.943 ms      │ 3.417 ms      │ 100     │ 100
   │  ╰─ "normalize"                      4.085 ms      │ 5.228 ms      │ 4.245 ms      │ 4.321 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                 │               │               │               │         │
      ├─ 100                              1.375 ms      │ 1.681 ms      │ 1.458 ms      │ 1.469 ms      │ 100     │ 100
      ├─ 1000                             2.393 ms      │ 2.699 ms      │ 2.447 ms      │ 2.46 ms       │ 100     │ 100
      ├─ 10000                            3.34 ms       │ 4.793 ms      │ 3.578 ms      │ 3.656 ms      │ 100     │ 100
      ╰─ 50000                            5.516 ms      │ 8.122 ms      │ 6.252 ms      │ 6.428 ms      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
