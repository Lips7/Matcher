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
│  │  ├─ 1                                2.332 ms      │ 3.161 ms      │ 2.444 ms      │ 2.507 ms      │ 100     │ 100
│  │  ├─ 2                                5.362 ms      │ 5.993 ms      │ 5.439 ms      │ 5.452 ms      │ 100     │ 100
│  │  ├─ 3                                7.815 ms      │ 25.71 ms      │ 8.38 ms       │ 8.809 ms      │ 100     │ 100
│  │  ├─ 4                                10.45 ms      │ 27.96 ms      │ 11.6 ms       │ 11.93 ms      │ 100     │ 100
│  │  ╰─ 5                                13.33 ms      │ 58.14 ms      │ 14.18 ms      │ 14.66 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_process_type   27.01 ms      │ 38.12 ms      │ 28.13 ms      │ 28.21 ms      │ 100     │ 100
│  ├─ build_cn_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         5.216 ms      │ 6.143 ms      │ 5.5 ms        │ 5.52 ms       │ 100     │ 100
│  │  ├─ "delete_normalize"               5.393 ms      │ 5.939 ms      │ 5.611 ms      │ 5.619 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        5.244 ms      │ 5.726 ms      │ 5.458 ms      │ 5.469 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       5.308 ms      │ 6.179 ms      │ 5.492 ms      │ 5.537 ms      │ 100     │ 100
│  │  ├─ "none"                           5.163 ms      │ 5.743 ms      │ 5.355 ms      │ 5.349 ms      │ 100     │ 100
│  │  ├─ "normalize"                      5.361 ms      │ 6.015 ms      │ 5.443 ms      │ 5.466 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         7.162 ms      │ 8.855 ms      │ 7.406 ms      │ 7.447 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     6.84 ms       │ 9.029 ms      │ 7.122 ms      │ 7.284 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              482.6 µs      │ 606 µs        │ 487.9 µs      │ 496.1 µs      │ 100     │ 100
│     ├─ 1000                             5.173 ms      │ 5.876 ms      │ 5.37 ms       │ 5.417 ms      │ 100     │ 100
│     ├─ 10000                            50.2 ms       │ 57.14 ms      │ 51.81 ms      │ 51.92 ms      │ 97      │ 97
│     ╰─ 50000                            189.7 ms      │ 223.6 ms      │ 196.7 ms      │ 198.4 ms      │ 26      │ 26
├─ build_en                                             │               │               │               │         │
│  ├─ build_en_by_combined_times                        │               │               │               │         │
│  │  ├─ 1                                5.934 ms      │ 6.618 ms      │ 6.04 ms       │ 6.102 ms      │ 100     │ 100
│  │  ├─ 2                                12.66 ms      │ 15.04 ms      │ 13.27 ms      │ 13.31 ms      │ 100     │ 100
│  │  ├─ 3                                20.95 ms      │ 23.4 ms       │ 21.64 ms      │ 21.76 ms      │ 100     │ 100
│  │  ├─ 4                                29.79 ms      │ 33.11 ms      │ 30.33 ms      │ 30.43 ms      │ 100     │ 100
│  │  ╰─ 5                                36.81 ms      │ 39.51 ms      │ 37.54 ms      │ 37.68 ms      │ 100     │ 100
│  ├─ build_en_by_multiple_process_type   15.54 ms      │ 17.4 ms       │ 16.11 ms      │ 16.17 ms      │ 100     │ 100
│  ├─ build_en_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         12.92 ms      │ 15.02 ms      │ 13.38 ms      │ 13.44 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               11.88 ms      │ 31.9 ms       │ 12.36 ms      │ 12.73 ms      │ 100     │ 100
│  │  ├─ "none"                           13.89 ms      │ 15.22 ms      │ 14.32 ms      │ 14.37 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      12.22 ms      │ 13.94 ms      │ 12.77 ms      │ 12.81 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              800.8 µs      │ 1.241 ms      │ 873.5 µs      │ 901.4 µs      │ 100     │ 100
│     ├─ 1000                             13.58 ms      │ 15.97 ms      │ 14.02 ms      │ 14.06 ms      │ 100     │ 100
│     ├─ 10000                            159.9 ms      │ 174.4 ms      │ 163.7 ms      │ 163.8 ms      │ 31      │ 31
│     ╰─ 50000                            689.6 ms      │ 740.9 ms      │ 705.7 ms      │ 708.2 ms      │ 8       │ 8
├─ search_cn                                            │               │               │               │         │
│  ├─ search_cn_baseline                                │               │               │               │         │
│  │  ├─ 100                              2.899 ms      │ 3.294 ms      │ 2.936 ms      │ 3.011 ms      │ 100     │ 100
│  │  ├─ 1000                             3.276 ms      │ 3.409 ms      │ 3.289 ms      │ 3.295 ms      │ 100     │ 100
│  │  ├─ 10000                            8.273 ms      │ 9.014 ms      │ 8.425 ms      │ 8.427 ms      │ 100     │ 100
│  │  ╰─ 50000                            26.13 ms      │ 33.49 ms      │ 27.27 ms      │ 28.17 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                3.999 ms      │ 4.47 ms       │ 4.297 ms      │ 4.268 ms      │ 100     │ 100
│  │  ├─ 2                                5.346 ms      │ 6.273 ms      │ 5.389 ms      │ 5.483 ms      │ 100     │ 100
│  │  ├─ 3                                6.169 ms      │ 8.067 ms      │ 6.627 ms      │ 6.629 ms      │ 100     │ 100
│  │  ├─ 4                                7.165 ms      │ 27.83 ms      │ 7.788 ms      │ 8.22 ms       │ 100     │ 100
│  │  ╰─ 5                                8.911 ms      │ 9.855 ms      │ 9.046 ms      │ 9.111 ms      │ 100     │ 100
│  ├─ search_cn_by_multiple_process_type  63.11 ms      │ 87.12 ms      │ 66.4 ms       │ 67.62 ms      │ 100     │ 100
│  ├─ search_cn_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         12.85 ms      │ 17.87 ms      │ 13.94 ms      │ 13.85 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               19.39 ms      │ 24.33 ms      │ 20.23 ms      │ 20.49 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        6.949 ms      │ 8.018 ms      │ 7.078 ms      │ 7.193 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       21.48 ms      │ 23.65 ms      │ 21.93 ms      │ 22.18 ms      │ 100     │ 100
│  │  ├─ "none"                           5.787 ms      │ 6.448 ms      │ 5.851 ms      │ 5.896 ms      │ 100     │ 100
│  │  ├─ "normalize"                      14.37 ms      │ 16.37 ms      │ 14.91 ms      │ 14.97 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         45.12 ms      │ 48.62 ms      │ 46.08 ms      │ 46.35 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     44.72 ms      │ 91.23 ms      │ 45.52 ms      │ 46.49 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              3.028 ms      │ 3.383 ms      │ 3.315 ms      │ 3.295 ms      │ 100     │ 100
│     ├─ 1000                             5.401 ms      │ 5.927 ms      │ 5.547 ms      │ 5.629 ms      │ 100     │ 100
│     ├─ 10000                            16.65 ms      │ 25.13 ms      │ 18.12 ms      │ 18.49 ms      │ 100     │ 100
│     ╰─ 50000                            52.6 ms       │ 66.24 ms      │ 56.53 ms      │ 57.59 ms      │ 87      │ 87
╰─ search_en                                            │               │               │               │         │
   ├─ search_en_baseline                                │               │               │               │         │
   │  ├─ 100                              329.6 µs      │ 475.7 µs      │ 358 µs        │ 361.3 µs      │ 100     │ 100
   │  ├─ 1000                             345.3 µs      │ 425.9 µs      │ 370.5 µs      │ 376.3 µs      │ 100     │ 100
   │  ├─ 10000                            1.003 ms      │ 1.071 ms      │ 1.016 ms      │ 1.018 ms      │ 100     │ 100
   │  ╰─ 50000                            1.005 ms      │ 1.033 ms      │ 1.011 ms      │ 1.012 ms      │ 100     │ 100
   ├─ search_en_by_combined_times                       │               │               │               │         │
   │  ├─ 1                                1.649 ms      │ 1.924 ms      │ 1.731 ms      │ 1.737 ms      │ 100     │ 100
   │  ├─ 2                                1.997 ms      │ 2.213 ms      │ 2.169 ms      │ 2.103 ms      │ 100     │ 100
   │  ├─ 3                                2.637 ms      │ 3.229 ms      │ 2.654 ms      │ 2.676 ms      │ 100     │ 100
   │  ├─ 4                                2.794 ms      │ 3.288 ms      │ 2.994 ms      │ 2.991 ms      │ 100     │ 100
   │  ╰─ 5                                3.148 ms      │ 3.673 ms      │ 3.172 ms      │ 3.193 ms      │ 100     │ 100
   ├─ search_en_by_multiple_process_type  9.085 ms      │ 10.77 ms      │ 9.369 ms      │ 9.466 ms      │ 100     │ 100
   ├─ search_en_by_process_type                         │               │               │               │         │
   │  ├─ "delete"                         6.529 ms      │ 9.715 ms      │ 7.111 ms      │ 7.243 ms      │ 100     │ 100
   │  ├─ "delete_normalize"               8.003 ms      │ 9.271 ms      │ 8.441 ms      │ 8.45 ms       │ 100     │ 100
   │  ├─ "none"                           2.553 ms      │ 2.897 ms      │ 2.569 ms      │ 2.573 ms      │ 100     │ 100
   │  ╰─ "normalize"                      4.064 ms      │ 4.651 ms      │ 4.096 ms      │ 4.122 ms      │ 100     │ 100
   ╰─ search_en_by_simple_word_map_size                 │               │               │               │         │
      ├─ 100                              1.294 ms      │ 1.524 ms      │ 1.393 ms      │ 1.392 ms      │ 100     │ 100
      ├─ 1000                             2.383 ms      │ 2.924 ms      │ 2.408 ms      │ 2.429 ms      │ 100     │ 100
      ├─ 10000                            3.491 ms      │ 4.746 ms      │ 3.578 ms      │ 3.687 ms      │ 100     │ 100
      ╰─ 50000                            5.407 ms      │ 7.827 ms      │ 5.471 ms      │ 5.545 ms      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
