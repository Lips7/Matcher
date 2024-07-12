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
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](./str_conv/FANJIAN.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [SYMBOL_NORM](./str_conv/SYMBOL-NORM.txt), [NORM](./str_conv/NORM.txt) and [NUM_NORM](./str_conv/NUM-NORM.txt).
  * `ℋЀ⒈㈠Õ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](./str_conv/PINYIN.txt).
  * `你好` -> ` ni  hao `
  * `西安` -> ` xi  an `
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN](./str_conv/PINYIN.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

`Delete` is technologically a combination of `TextDelete` and `WordDelete`, we implement different delete methods for text and word. 'Cause we believe special characters are parts of the word, users put them in words deliberately, but not for text. For `text_process` and `reduce_text_process` functions, users should use `TextDelete` instead of `WordDelete`.
* `WordDelete`: Delete all patterns in `WHITE_SPACE`.
* `TextDelete`: Delete all patterns in [TEXT_DELETE](./str_conv/TEXT-DELETE.txt).

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

let mut smt_word_map = HashMap::new();
let mut simple_word_map = HashMap::new();

simple_word_map.insert(1, "你好");
simple_word_map.insert(2, "世界");

smt_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);

let matcher = SimpleMatcher::new(&smt_word_map);
let text = "你好，世界！";
let results = matcher.process(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Feature Flags
* `prebuilt`: By enable prebuilt feature, we could boost  process matcher build time, but with package size increasing.
* `runtime_build`: By enable runtime_build feature, we could build process matcher at runtime, but with build time increasing.
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

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).