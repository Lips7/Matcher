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
│  │  ├─ 1                           19.44 ms      │ 22.83 ms      │ 19.69 ms      │ 19.77 ms      │ 100     │ 100
│  │  ├─ 3                           63.55 ms      │ 68.98 ms      │ 64.29 ms      │ 64.71 ms      │ 78      │ 78
│  │  ╰─ 5                           111.9 ms      │ 201.8 ms      │ 116.6 ms      │ 121.2 ms      │ 42      │ 42
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    63.59 ms      │ 92.53 ms      │ 64.63 ms      │ 65.8 ms       │ 76      │ 76
│  │  ├─ "fanjian"                   63.49 ms      │ 67.24 ms      │ 64.45 ms      │ 64.48 ms      │ 78      │ 78
│  │  ├─ "fanjian_delete_normalize"  64.83 ms      │ 83.9 ms       │ 65.5 ms       │ 66.32 ms      │ 76      │ 76
│  │  ╰─ "none"                      63.56 ms      │ 67.14 ms      │ 64.15 ms      │ 64.28 ms      │ 78      │ 78
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        6.426 ms      │ 7.044 ms      │ 6.485 ms      │ 6.511 ms      │ 100     │ 100
│  │  ├─ 10000                       63.43 ms      │ 66.57 ms      │ 64.21 ms      │ 64.26 ms      │ 78      │ 78
│  │  ╰─ 50000                       252.8 ms      │ 268.1 ms      │ 255.4 ms      │ 256.3 ms      │ 20      │ 20
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           83.59 ms      │ 87.48 ms      │ 84.49 ms      │ 84.52 ms      │ 60      │ 60
│  │  ├─ 3                           187.6 ms      │ 202.6 ms      │ 189.4 ms      │ 190.1 ms      │ 27      │ 27
│  │  ╰─ 5                           342.6 ms      │ 363.1 ms      │ 348.1 ms      │ 349.4 ms      │ 15      │ 15
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    187.7 ms      │ 199.1 ms      │ 189.8 ms      │ 190.3 ms      │ 27      │ 27
│  │  ├─ "delete_normalize"          180.5 ms      │ 189.9 ms      │ 182.5 ms      │ 182.9 ms      │ 28      │ 28
│  │  ╰─ "none"                      187.8 ms      │ 200 ms        │ 189.6 ms      │ 190.6 ms      │ 27      │ 27
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        19.18 ms      │ 20.75 ms      │ 19.48 ms      │ 19.49 ms      │ 100     │ 100
│     ├─ 10000                       188.3 ms      │ 195.4 ms      │ 189.8 ms      │ 190.1 ms      │ 27      │ 27
│     ╰─ 50000                       1.01 s        │ 1.037 s       │ 1.011 s       │ 1.018 s       │ 5       │ 5
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    33 ms         │ 48.42 ms      │ 34.02 ms      │ 34.09 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   23.53 ms      │ 29.04 ms      │ 24.53 ms      │ 24.42 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  50.87 ms      │ 56.38 ms      │ 52.49 ms      │ 52.25 ms      │ 96      │ 96
│  │  ╰─ "none"                      21.51 ms      │ 25.22 ms      │ 21.94 ms      │ 22.17 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.576 ms      │ 5.197 ms      │ 4.646 ms      │ 4.656 ms      │ 100     │ 100
│  │  ├─ 10000                       21.43 ms      │ 29.17 ms      │ 22.51 ms      │ 22.47 ms      │ 100     │ 100
│  │  ╰─ 50000                       88.66 ms      │ 113.2 ms      │ 95.2 ms       │ 95.45 ms      │ 53      │ 53
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.335 ms      │ 1.809 ms      │ 1.348 ms      │ 1.357 ms      │ 100     │ 100
│  │  ├─ 3                           2.163 ms      │ 3.045 ms      │ 2.27 ms       │ 2.263 ms      │ 100     │ 100
│  │  ╰─ 5                           3.011 ms      │ 4.312 ms      │ 3.091 ms      │ 3.141 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    7.182 ms      │ 8.822 ms      │ 7.398 ms      │ 7.376 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          9.531 ms      │ 11.8 ms       │ 9.784 ms      │ 9.799 ms      │ 100     │ 100
│  │  ╰─ "none"                      2.162 ms      │ 2.937 ms      │ 2.175 ms      │ 2.196 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.087 ms      │ 1.37 ms       │ 1.105 ms      │ 1.113 ms      │ 100     │ 100
│     ├─ 10000                       2.169 ms      │ 3.064 ms      │ 2.298 ms      │ 2.29 ms       │ 100     │ 100
│     ╰─ 50000                       6.201 ms      │ 8.515 ms      │ 6.309 ms      │ 6.457 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.118 ms      │ 6.337 ms      │ 6.148 ms      │ 6.154 ms      │ 100     │ 100
   │  ├─ "fanjian"                   1.327 ms      │ 1.404 ms      │ 1.335 ms      │ 1.34 ms       │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  10.18 ms      │ 10.49 ms      │ 10.24 ms      │ 10.25 ms      │ 100     │ 100
   │  ╰─ "none"                      370.7 µs      │ 405.8 µs      │ 378.4 µs      │ 380.4 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        367.6 µs      │ 406.2 µs      │ 374.4 µs      │ 375.4 µs      │ 100     │ 100
   │  ├─ 10000                       378.4 µs      │ 404.2 µs      │ 389.9 µs      │ 388.6 µs      │ 100     │ 100
   │  ╰─ 50000                       378.9 µs      │ 867 µs        │ 394.1 µs      │ 411.1 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           937.5 µs      │ 1.096 ms      │ 949.9 µs      │ 958.5 µs      │ 100     │ 100
   │  ├─ 3                           924.4 µs      │ 949.7 µs      │ 932.7 µs      │ 933.2 µs      │ 100     │ 100
   │  ╰─ 5                           957.8 µs      │ 998.1 µs      │ 966.6 µs      │ 968.9 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    4.623 ms      │ 5.075 ms      │ 4.654 ms      │ 4.679 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          5.859 ms      │ 6.225 ms      │ 5.908 ms      │ 5.938 ms      │ 100     │ 100
   │  ╰─ "none"                      942.2 µs      │ 967 µs        │ 949.4 µs      │ 950.8 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        909.1 µs      │ 1.181 ms      │ 927.2 µs      │ 930.3 µs      │ 100     │ 100
      ├─ 10000                       920.1 µs      │ 971.7 µs      │ 927.7 µs      │ 930 µs        │ 100     │ 100
      ╰─ 50000                       929.9 µs      │ 1.002 ms      │ 938.9 µs      │ 941.2 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
