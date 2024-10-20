# Matcher

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

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
│  │  ├─ 1                                2.421 ms      │ 3.108 ms      │ 2.433 ms      │ 2.468 ms      │ 100     │ 100
│  │  ├─ 2                                4.98 ms       │ 5.647 ms      │ 5.047 ms      │ 5.073 ms      │ 100     │ 100
│  │  ├─ 3                                7.651 ms      │ 10.03 ms      │ 7.802 ms      │ 7.947 ms      │ 100     │ 100
│  │  ├─ 4                                10.23 ms      │ 12.06 ms      │ 10.5 ms       │ 10.61 ms      │ 100     │ 100
│  │  ╰─ 5                                12.93 ms      │ 14.1 ms       │ 13.15 ms      │ 13.24 ms      │ 100     │ 100
│  ├─ build_cn_by_multiple_process_type   25.3 ms       │ 59.86 ms      │ 26 ms         │ 26.53 ms      │ 100     │ 100
│  ├─ build_cn_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         5.053 ms      │ 5.439 ms      │ 5.176 ms      │ 5.191 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               4.962 ms      │ 5.768 ms      │ 5.069 ms      │ 5.1 ms        │ 100     │ 100
│  │  ├─ "fanjian"                        5.109 ms      │ 8.929 ms      │ 5.19 ms       │ 5.366 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       4.987 ms      │ 8.449 ms      │ 5.26 ms       │ 5.424 ms      │ 100     │ 100
│  │  ├─ "none"                           5.03 ms       │ 14.95 ms      │ 5.159 ms      │ 5.353 ms      │ 100     │ 100
│  │  ├─ "normalize"                      5.039 ms      │ 5.872 ms      │ 5.214 ms      │ 5.247 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         6.722 ms      │ 14.46 ms      │ 7.347 ms      │ 7.344 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     6.603 ms      │ 9.37 ms       │ 7.147 ms      │ 7.197 ms      │ 100     │ 100
│  ╰─ build_cn_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              471.7 µs      │ 681.7 µs      │ 501.9 µs      │ 512.3 µs      │ 100     │ 100
│     ├─ 1000                             5.186 ms      │ 5.858 ms      │ 5.292 ms      │ 5.321 ms      │ 100     │ 100
│     ├─ 10000                            47.09 ms      │ 51.62 ms      │ 47.4 ms       │ 47.77 ms      │ 100     │ 100
│     ╰─ 50000                            180.3 ms      │ 194.4 ms      │ 185.7 ms      │ 186.1 ms      │ 27      │ 27
├─ build_en                                             │               │               │               │         │
│  ├─ build_en_by_combined_times                        │               │               │               │         │
│  │  ├─ 1                                5.629 ms      │ 6.387 ms      │ 5.733 ms      │ 5.759 ms      │ 100     │ 100
│  │  ├─ 2                                13.33 ms      │ 17.14 ms      │ 13.51 ms      │ 13.55 ms      │ 100     │ 100
│  │  ├─ 3                                19.83 ms      │ 23.14 ms      │ 20.85 ms      │ 20.85 ms      │ 100     │ 100
│  │  ├─ 4                                27.55 ms      │ 30.19 ms      │ 27.73 ms      │ 27.8 ms       │ 100     │ 100
│  │  ╰─ 5                                35.21 ms      │ 37.18 ms      │ 35.55 ms      │ 35.6 ms       │ 100     │ 100
│  ├─ build_en_by_multiple_process_type   15.21 ms      │ 16.72 ms      │ 15.8 ms       │ 15.79 ms      │ 100     │ 100
│  ├─ build_en_by_process_type                          │               │               │               │         │
│  │  ├─ "delete"                         12.63 ms      │ 26.19 ms      │ 13.2 ms       │ 13.32 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               11.76 ms      │ 12.68 ms      │ 11.94 ms      │ 11.95 ms      │ 100     │ 100
│  │  ├─ "none"                           12.21 ms      │ 13.52 ms      │ 12.67 ms      │ 12.71 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      11.45 ms      │ 12.09 ms      │ 11.59 ms      │ 11.61 ms      │ 100     │ 100
│  ╰─ build_en_by_simple_word_map_size                  │               │               │               │         │
│     ├─ 100                              820 µs        │ 1.184 ms      │ 830.6 µs      │ 851.1 µs      │ 100     │ 100
│     ├─ 1000                             13 ms         │ 14.52 ms      │ 13.65 ms      │ 13.62 ms      │ 100     │ 100
│     ├─ 10000                            151.4 ms      │ 169.1 ms      │ 157.5 ms      │ 157.6 ms      │ 32      │ 32
│     ╰─ 50000                            640.3 ms      │ 677.1 ms      │ 655 ms        │ 655.3 ms      │ 8       │ 8
├─ search_cn                                            │               │               │               │         │
│  ├─ search_cn_baseline                                │               │               │               │         │
│  │  ├─ 100                              2.904 ms      │ 7.824 ms      │ 2.927 ms      │ 2.986 ms      │ 100     │ 100
│  │  ├─ 1000                             3.046 ms      │ 3.81 ms       │ 3.066 ms      │ 3.095 ms      │ 100     │ 100
│  │  ├─ 10000                            7.651 ms      │ 8.541 ms      │ 7.77 ms       │ 7.854 ms      │ 100     │ 100
│  │  ╰─ 50000                            26.67 ms      │ 47.51 ms      │ 28.74 ms      │ 30.15 ms      │ 100     │ 100
│  ├─ search_cn_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                3.967 ms      │ 4.308 ms      │ 4.031 ms      │ 4.039 ms      │ 100     │ 100
│  │  ├─ 2                                5.201 ms      │ 5.742 ms      │ 5.246 ms      │ 5.264 ms      │ 100     │ 100
│  │  ├─ 3                                6.405 ms      │ 7.174 ms      │ 6.442 ms      │ 6.47 ms       │ 100     │ 100
│  │  ├─ 4                                7.012 ms      │ 7.671 ms      │ 7.039 ms      │ 7.067 ms      │ 100     │ 100
│  │  ╰─ 5                                8.471 ms      │ 9.027 ms      │ 8.606 ms      │ 8.621 ms      │ 100     │ 100
│  ├─ search_cn_by_multiple_process_type  61.42 ms      │ 92.44 ms      │ 64.06 ms      │ 65.2 ms       │ 100     │ 100
│  ├─ search_cn_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         14.44 ms      │ 15.15 ms      │ 14.59 ms      │ 14.59 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               20.58 ms      │ 21.86 ms      │ 21.19 ms      │ 21.08 ms      │ 100     │ 100
│  │  ├─ "fanjian"                        6.902 ms      │ 7.653 ms      │ 7.232 ms      │ 7.179 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"       21.72 ms      │ 23.12 ms      │ 21.98 ms      │ 22.11 ms      │ 100     │ 100
│  │  ├─ "none"                           5.013 ms      │ 5.628 ms      │ 5.053 ms      │ 5.073 ms      │ 100     │ 100
│  │  ├─ "normalize"                      15.25 ms      │ 16.69 ms      │ 15.44 ms      │ 15.62 ms      │ 100     │ 100
│  │  ├─ "pinyin"                         41.1 ms       │ 45.53 ms      │ 43.78 ms      │ 43.21 ms      │ 100     │ 100
│  │  ╰─ "pinyinchar"                     42.93 ms      │ 48.92 ms      │ 45.06 ms      │ 44.83 ms      │ 100     │ 100
│  ╰─ search_cn_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              3.205 ms      │ 3.498 ms      │ 3.242 ms      │ 3.268 ms      │ 100     │ 100
│     ├─ 1000                             5.057 ms      │ 5.674 ms      │ 5.273 ms      │ 5.277 ms      │ 100     │ 100
│     ├─ 10000                            16.31 ms      │ 19.4 ms       │ 17.24 ms      │ 17.12 ms      │ 100     │ 100
│     ╰─ 50000                            53.87 ms      │ 93.62 ms      │ 58.71 ms      │ 62.27 ms      │ 81      │ 81
├─ search_en                                            │               │               │               │         │
│  ├─ search_en_baseline                                │               │               │               │         │
│  │  ├─ 100                              353.9 µs      │ 471.7 µs      │ 376.6 µs      │ 381.7 µs      │ 100     │ 100
│  │  ├─ 1000                             369 µs        │ 452.2 µs      │ 389.1 µs      │ 393.8 µs      │ 100     │ 100
│  │  ├─ 10000                            1.027 ms      │ 1.06 ms       │ 1.034 ms      │ 1.035 ms      │ 100     │ 100
│  │  ╰─ 50000                            1.004 ms      │ 1.055 ms      │ 1.016 ms      │ 1.018 ms      │ 100     │ 100
│  ├─ search_en_by_combined_times                       │               │               │               │         │
│  │  ├─ 1                                1.788 ms      │ 4.898 ms      │ 1.915 ms      │ 1.94 ms       │ 100     │ 100
│  │  ├─ 2                                2.477 ms      │ 2.747 ms      │ 2.489 ms      │ 2.494 ms      │ 100     │ 100
│  │  ├─ 3                                2.792 ms      │ 3.142 ms      │ 2.805 ms      │ 2.813 ms      │ 100     │ 100
│  │  ├─ 4                                2.691 ms      │ 3.115 ms      │ 2.711 ms      │ 2.717 ms      │ 100     │ 100
│  │  ╰─ 5                                2.786 ms      │ 3.342 ms      │ 2.803 ms      │ 2.824 ms      │ 100     │ 100
│  ├─ search_en_by_multiple_process_type  10.12 ms      │ 11.85 ms      │ 10.76 ms      │ 10.56 ms      │ 100     │ 100
│  ├─ search_en_by_process_type                         │               │               │               │         │
│  │  ├─ "delete"                         7.104 ms      │ 13.92 ms      │ 7.145 ms      │ 7.235 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"               8.588 ms      │ 9.469 ms      │ 8.71 ms       │ 8.848 ms      │ 100     │ 100
│  │  ├─ "none"                           2.436 ms      │ 2.711 ms      │ 2.456 ms      │ 2.466 ms      │ 100     │ 100
│  │  ╰─ "normalize"                      4.047 ms      │ 4.338 ms      │ 4.07 ms       │ 4.076 ms      │ 100     │ 100
│  ╰─ search_en_by_simple_word_map_size                 │               │               │               │         │
│     ├─ 100                              1.355 ms      │ 3.969 ms      │ 1.429 ms      │ 1.483 ms      │ 100     │ 100
│     ├─ 1000                             2.064 ms      │ 2.279 ms      │ 2.077 ms      │ 2.084 ms      │ 100     │ 100
│     ├─ 10000                            3.381 ms      │ 4.793 ms      │ 3.396 ms      │ 3.415 ms      │ 100     │ 100
│     ╰─ 50000                            4.561 ms      │ 6.879 ms      │ 4.659 ms      │ 4.824 ms      │ 100     │ 100
╰─ single_line                                          │               │               │               │         │
   ├─ search_cn_single_line                             │               │               │               │         │
   │  ├─ 100                              252.2 ns      │ 426.8 ns      │ 262.7 ns      │ 271.7 ns      │ 100     │ 1600
   │  ├─ 1000                             309.6 ns      │ 338.2 ns      │ 317.4 ns      │ 317.5 ns      │ 100     │ 1600
   │  ├─ 10000                            540.7 ns      │ 10.04 µs      │ 624.7 ns      │ 725.5 ns      │ 100     │ 100
   │  ╰─ 50000                            1.29 µs       │ 43.45 µs      │ 1.374 µs      │ 1.848 µs      │ 100     │ 100
   ╰─ search_en_single_line                             │               │               │               │         │
      ├─ 100                              56.04 ns      │ 58.64 ns      │ 57.01 ns      │ 56.92 ns      │ 100     │ 12800
      ├─ 1000                             56.69 ns      │ 68.4 ns       │ 57.99 ns      │ 58.17 ns      │ 100     │ 12800
      ├─ 10000                            374.7 ns      │ 5.291 µs      │ 457.7 ns      │ 512.6 ns      │ 100     │ 100
      ╰─ 50000                            457.7 ns      │ 16.99 µs      │ 540.7 ns      │ 701.9 ns      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
