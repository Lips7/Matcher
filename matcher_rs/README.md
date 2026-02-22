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

> [!IMPORTANT]
> **Git Dependency Limitation**: This crate currently depends on a git-based version of `aho-corasick-unsafe` (a fork of `aho-corasick`). As a result, projects depending on `matcher_rs` must also use a git dependency or the `matcher_rs` dependency will fail to resolve in some package registry environments.

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
Current default simple word map size: 10000
Current default combined times: 3
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           29.37 ms      │ 45.62 ms      │ 37.29 ms      │ 37.58 ms      │ 100     │ 100
│  │  ├─ 3                           87.9 ms       │ 188.5 ms      │ 120.9 ms      │ 133.6 ms      │ 38      │ 38
│  │  ╰─ 5                           165.2 ms      │ 321.4 ms      │ 216.2 ms      │ 213.7 ms      │ 24      │ 24
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    80.33 ms      │ 134.8 ms      │ 86.57 ms      │ 92.15 ms      │ 55      │ 55
│  │  ├─ "fanjian"                   79.97 ms      │ 91.93 ms      │ 84.56 ms      │ 84.98 ms      │ 59      │ 59
│  │  ├─ "fanjian_delete_normalize"  82.53 ms      │ 101.6 ms      │ 87.69 ms      │ 88.56 ms      │ 57      │ 57
│  │  ╰─ "none"                      81.4 ms       │ 95.44 ms      │ 87.8 ms       │ 87.73 ms      │ 58      │ 58
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        7.87 ms       │ 9.942 ms      │ 8.612 ms      │ 8.589 ms      │ 100     │ 100
│  │  ├─ 10000                       83.53 ms      │ 111.6 ms      │ 90.93 ms      │ 92.29 ms      │ 55      │ 55
│  │  ╰─ 50000                       356.7 ms      │ 395.9 ms      │ 375.9 ms      │ 374.8 ms      │ 14      │ 14
│  ╰─ en_by_process_type                           │               │               │               │         │
│     ├─ "delete"                    253.5 ms      │ 665.5 ms      │ 279.8 ms      │ 308.7 ms      │ 17      │ 17
│     ├─ "fanjian"                   263.9 ms      │ 665.6 ms      │ 407.4 ms      │ 429.1 ms      │ 12      │ 12
│     ├─ "fanjian_delete_normalize"  230.5 ms      │ 836.7 ms      │ 275 ms        │ 341.5 ms      │ 17      │ 17
│     ╰─ "none"                      354.6 ms      │ 704.8 ms      │ 385.8 ms      │ 445.9 ms      │ 12      │ 12
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    44.99 ms      │ 112.5 ms      │ 61.08 ms      │ 62.1 ms       │ 81      │ 81
│  │  ├─ "fanjian"                   32.44 ms      │ 47.51 ms      │ 37.93 ms      │ 38.14 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  58.44 ms      │ 83.77 ms      │ 61.99 ms      │ 63.44 ms      │ 79      │ 79
│  │  ╰─ "none"                      24.43 ms      │ 78.96 ms      │ 29.47 ms      │ 34.6 ms       │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        8.427 ms      │ 15.72 ms      │ 11.22 ms      │ 11.27 ms      │ 100     │ 100
│  │  ├─ 10000                       36.99 ms      │ 147.4 ms      │ 51.33 ms      │ 55.26 ms      │ 91      │ 91
│  │  ╰─ 50000                       193.2 ms      │ 503.6 ms      │ 306.8 ms      │ 313.8 ms      │ 17      │ 17
│  ╰─ en_by_process_type                           │               │               │               │         │
│     ├─ "delete"                    14.44 ms      │ 30.19 ms      │ 19.01 ms      │ 18.81 ms      │ 100     │ 100
│     ├─ "fanjian"                   4.739 ms      │ 7.294 ms      │ 5.582 ms      │ 5.707 ms      │ 100     │ 100
│     ├─ "fanjian_delete_normalize"  18.1 ms       │ 25.92 ms      │ 22.29 ms      │ 22.21 ms      │ 100     │ 100
│     ╰─ "none"                      3.373 ms      │ 6.808 ms      │ 4.038 ms      │ 4.151 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    9.555 ms      │ 11.85 ms      │ 9.859 ms      │ 10.24 ms      │ 100     │ 100
   │  ├─ "fanjian"                   2.062 ms      │ 2.754 ms      │ 2.322 ms      │ 2.332 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  14.48 ms      │ 25.73 ms      │ 16.2 ms       │ 16.31 ms      │ 100     │ 100
   │  ╰─ "none"                      528.9 µs      │ 598.9 µs      │ 548.5 µs      │ 549.1 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        522.1 µs      │ 923.7 µs      │ 535.5 µs      │ 542.4 µs      │ 100     │ 100
   │  ├─ 10000                       488.2 µs      │ 593.2 µs      │ 507.1 µs      │ 509.6 µs      │ 100     │ 100
   │  ╰─ 50000                       483.6 µs      │ 850.5 µs      │ 500.5 µs      │ 506.6 µs      │ 100     │ 100
   ╰─ en_by_process_type                           │               │               │               │         │
      ├─ "delete"                    5.914 ms      │ 6.662 ms      │ 6.11 ms       │ 6.14 ms       │ 100     │ 100
      ├─ "fanjian"                   1.804 ms      │ 2.191 ms      │ 1.859 ms      │ 1.87 ms       │ 100     │ 100
      ├─ "fanjian_delete_normalize"  7.744 ms      │ 8.883 ms      │ 8.279 ms      │ 8.2 ms        │ 100     │ 100
      ╰─ "none"                      1.12 ms       │ 1.28 ms       │ 1.146 ms      │ 1.172 ms      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
