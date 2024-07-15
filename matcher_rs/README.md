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
* `prebuilt`: By enable prebuilt feature, we could boost process matcher build time, but with package size increasing.
* `runtime_build`: By enable runtime_build feature, we could build process matcher at runtime, but with build time increasing.
* `serde`: By enable serde feature, we could serialize and deserialize matcher and simple_matcher. With serde feature, AhoCorasick's prefilter is disabled, because I don't know how to serialize it correctly, which will lead to performance regression when the patterns size is small (say, less than 100).
* `dfa`: By enable dfa feature, we could use dfa to perform simple matching, but with significantly incresaing memory consumption.

Default feature is `prebuilt` and `dfa`, `prebuilt` and `runtime_build` can't be enabled at same time. If you want to make `Matcher` and `SimpleMatcher` serializable, you should enable `serde` feature.

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

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
