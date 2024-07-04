# Matcher

A high-performance, multi-functional word matcher implemented in Rust.

Designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching. For detailed implementation, see the [Design Document](../DESIGN.md).

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

### Adding to Your Project

To use `matcher_rs` in your Rust project, run the following command:

```shell
cargo add matcher_rs
```

### Explanation of the configuration

* `Matcher`'s configuration is defined by the `MatchTableMap = HashMap<u32, Vec<MatchTable>>` type, the key of `MatchTableMap` is called `match_id`, for each `match_id`, the `table_id` inside **should but isn't required to be unique**.
* `SimpleMatcher`'s configuration is defined by the `SimpleMatchTableMap = HashMap<SimpleMatchType, HashMap<u32, &'a str>>` type, the value `HashMap<u32, &'a str>`'s key is called `word_id`, **`word_id` is required to be globally unique**.

#### MatchTable

* `table_id`: The unique ID of the match table.
* `match_table_type`: The type of the match table.
* `word_list`: The word list of the match table.
* `exemption_simple_match_type`: The type of the exemption simple match.
* `exemption_word_list`: The exemption word list of the match table.

For each match table, word matching is performed over the `word_list`, and exemption word matching is performed over the `exemption_word_list`. If the exemption word matching result is True, the word matching result will be False.

#### MatchTableType

* `Simple`: Supports simple multiple patterns matching with text normalization defined by `simple_match_type`.
  * We offer transformation methods for text normalization, including `Fanjian`, `Normalize`, `PinYin` ···.
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
  * `DamerauLevenshtein`: Supports similar text matching based on Damerau-Levenshtein distance.
  * `Indel`: Supports similar text matching based on Indel distance.
  * `Jaro`: Supports similar text matching based on Jaro distance.
  * `JaroWinkler`: Supports similar text matching based on Jaro-Winkler distance.

#### SimpleMatchType

* `None`: No transformation.
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](./str_conv_map/FANJIAN.txt) and [UNICODE](./str_conv_map/UNICODE.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [UPPER_LOWER](./str_conv_map/UPPER-LOWER.txt), [EN_VARIATION](./str_conv_map/EN-VARIATION.txt), [NUM_NORM](./str_conv_map/NUM-NORM.txt) and [CHAR](./str_conv_map/CHAR.txt).
  * `ℋЀ⒈㈠ϕ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](./str_conv_map/PINYIN.txt).
  * `你好` -> `␀ni␀␀hao␀`
  * `西安` -> `␀xi␀␀an␀`
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN_CHAR](./str_conv_map/PINYIN-CHAR.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

`Delete` is technologically a combination of `TextDelete` and `WordDelete`, we implement different delete methods for text and word. 'Cause we believe `CN_SPECIAL` and `EN_SPECIAL` are parts of the word, but not for text. For `text_process` and `reduce_text_process` functions, users should use `TextDelete` instead of `WordDelete`.
* `WordDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](./str_conv_map/PUNCTUATION-SPECIAL.txt).
* `TextDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](./str_conv_map/PUNCTUATION-SPECIAL.txt), [CN_SPECIAL](./str_conv_map/CN-SPECIAL.txt), [EN_SPECIAL](./str_conv_map/EN-SPECIAL.txt).

### Basic Example

Here’s a basic example of how to use the `Matcher` struct for text matching:

```rust
use matcher_rs::{text_process, reduce_text_process, SimpleMatchType};

let result = text_process(SimpleMatchType::TextDelete, "你好，世界！");
let result = reduce_text_process(SimpleMatchType::FanjianDeleteNormalize, "你好，世界！");
```

```rust
use std::collections::HashMap;
use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, SimpleMatchType};

let match_table_map: MatchTableMap = HashMap::from_iter(vec![
    (1, vec![MatchTable {
        table_id: 1,
        match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::FanjianDeleteNormalize},
        word_list: vec!["example", "test"],
        exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        exemption_word_list: vec![],
    }]),
]);
let matcher = Matcher::new(&match_table_map);
let text = "This is an example text.";
let results = matcher.word_match(text);
```

```rust
use std::collections::HashMap;
use matcher_rs::{SimpleMatchType, SimpleMatcher};

let mut simple_match_type_word_map = HashMap::default();
let mut simple_word_map = HashMap::default();

simple_word_map.insert(1, "你好");
simple_word_map.insert(2, "世界");

simple_match_type_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);

let matcher = SimpleMatcher::new(&simple_match_type_word_map);
let text = "你好，世界！";
let results = matcher.process(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Feature Flags
* `prebuilt`: By enable prebuilt feature, we could boost matcher and simple_matcher build time, but with package size increasing.
* `runtime_build`: By enable runtime_build feature, we could build matcher and simple_matcher at runtime, but with build time increasing.
* `serde`: By enable serde feature, we could serialize and deserialize matcher and simple_matcher. With serde feature, AhoCorasick's prefilter is disabled, because I don't know how to serialize it correctly, which will lead to performance regression when the patterns size is small (say, less than 100).

Default feature is `prebuilt`, `prebuilt` and `runtime_build` can't be enabled at same time. If you want to make `Matcher` and `SimpleMatcher` serializable, you should enable `serde` feature.

## Benchmarks

Bench against pairs ([CN_WORD_LIST_100000](../data/word_list/cn/cn_words_100000.txt), [CN_HAYSTACK](../data/text/cn/西游记.txt)) and ([EN_WORD_LIST_100000](../data/word_list/en/en_words_100000.txt), [EN_HAYSTACK](../data/text/en/sherlock.txt)). Word selection is totally random.

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

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).