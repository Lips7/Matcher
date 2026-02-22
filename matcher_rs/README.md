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
│  │  ├─ 1                           19.97 ms      │ 40.85 ms      │ 20.84 ms      │ 21.25 ms      │ 100     │ 100
│  │  ├─ 3                           64.28 ms      │ 113.8 ms      │ 66.83 ms      │ 67.73 ms      │ 74      │ 74
│  │  ╰─ 5                           112.9 ms      │ 181.7 ms      │ 121.5 ms      │ 123.7 ms      │ 41      │ 41
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    66.12 ms      │ 150.6 ms      │ 69.77 ms      │ 71.85 ms      │ 70      │ 70
│  │  ├─ "fanjian"                   67.25 ms      │ 187.4 ms      │ 71.57 ms      │ 74.36 ms      │ 68      │ 68
│  │  ├─ "fanjian_delete_normalize"  67.51 ms      │ 189 ms        │ 73.74 ms      │ 76.97 ms      │ 65      │ 65
│  │  ╰─ "none"                      66.99 ms      │ 171.4 ms      │ 70.2 ms       │ 73.42 ms      │ 69      │ 69
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        6.467 ms      │ 8.219 ms      │ 6.822 ms      │ 6.877 ms      │ 100     │ 100
│  │  ├─ 10000                       65.87 ms      │ 87.88 ms      │ 68.91 ms      │ 69.91 ms      │ 72      │ 72
│  │  ╰─ 50000                       267.5 ms      │ 356.4 ms      │ 298.4 ms      │ 300.8 ms      │ 17      │ 17
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           85.29 ms      │ 174.2 ms      │ 92.48 ms      │ 103.3 ms      │ 49      │ 49
│  │  ├─ 3                           301.5 ms      │ 443 ms        │ 348.4 ms      │ 347.5 ms      │ 15      │ 15
│  │  ╰─ 5                           480.3 ms      │ 1.009 s       │ 523.8 ms      │ 591.4 ms      │ 9       │ 9
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    273.8 ms      │ 453.8 ms      │ 304.8 ms      │ 320.4 ms      │ 17      │ 17
│  │  ├─ "delete_normalize"          436.1 ms      │ 1.099 s       │ 548.2 ms      │ 658.1 ms      │ 8       │ 8
│  │  ╰─ "none"                      371.5 ms      │ 1.205 s       │ 464.7 ms      │ 564.6 ms      │ 9       │ 9
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        39.93 ms      │ 71.26 ms      │ 50.83 ms      │ 52.6 ms       │ 96      │ 96
│     ├─ 10000                       421.1 ms      │ 598.2 ms      │ 494.3 ms      │ 495.2 ms      │ 11      │ 11
│     ╰─ 50000                       2.887 s       │ 4.306 s       │ 3.597 s       │ 3.597 s       │ 2       │ 2
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    79.34 ms      │ 268 ms        │ 112.8 ms      │ 122.5 ms      │ 41      │ 41
│  │  ├─ "fanjian"                   123 ms        │ 672.4 ms      │ 140.7 ms      │ 202.7 ms      │ 25      │ 25
│  │  ├─ "fanjian_delete_normalize"  220.4 ms      │ 849.2 ms      │ 243.3 ms      │ 289.4 ms      │ 18      │ 18
│  │  ╰─ "none"                      102.8 ms      │ 191.6 ms      │ 115.1 ms      │ 119.1 ms      │ 42      │ 42
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        13.44 ms      │ 33.61 ms      │ 14.68 ms      │ 15.1 ms       │ 100     │ 100
│  │  ├─ 10000                       81.41 ms      │ 209.1 ms      │ 103.6 ms      │ 106.3 ms      │ 47      │ 47
│  │  ╰─ 50000                       597.9 ms      │ 664 ms        │ 638 ms        │ 634.8 ms      │ 8       │ 8
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           4.054 ms      │ 11.28 ms      │ 4.455 ms      │ 4.621 ms      │ 100     │ 100
│  │  ├─ 3                           8.297 ms      │ 17.46 ms      │ 9.862 ms      │ 10.57 ms      │ 100     │ 100
│  │  ╰─ 5                           13.66 ms      │ 48.42 ms      │ 15.7 ms       │ 17.26 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    28.48 ms      │ 39.67 ms      │ 31.45 ms      │ 31.91 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          38.03 ms      │ 67.89 ms      │ 43.07 ms      │ 44.65 ms      │ 100     │ 100
│  │  ╰─ "none"                      8.024 ms      │ 15.69 ms      │ 9.758 ms      │ 9.952 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        3.044 ms      │ 4.581 ms      │ 3.499 ms      │ 3.521 ms      │ 100     │ 100
│     ├─ 10000                       6.828 ms      │ 14.19 ms      │ 8.425 ms      │ 8.501 ms      │ 100     │ 100
│     ╰─ 50000                       30.9 ms       │ 48.74 ms      │ 36.23 ms      │ 36.71 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    14.08 ms      │ 21.22 ms      │ 15.92 ms      │ 16.02 ms      │ 100     │ 100
   │  ├─ "fanjian"                   3.584 ms      │ 6.19 ms       │ 4.209 ms      │ 4.293 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  20.11 ms      │ 26.94 ms      │ 24.23 ms      │ 24.17 ms      │ 100     │ 100
   │  ╰─ "none"                      852.2 µs      │ 1.228 ms      │ 892.6 µs      │ 912.8 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        837.9 µs      │ 1.322 ms      │ 883.9 µs      │ 912.2 µs      │ 100     │ 100
   │  ├─ 10000                       843.7 µs      │ 1.025 ms      │ 881.4 µs      │ 887.4 µs      │ 100     │ 100
   │  ╰─ 50000                       855 µs        │ 1.366 ms      │ 1.045 ms      │ 1.058 ms      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           2.489 ms      │ 3.514 ms      │ 2.667 ms      │ 2.716 ms      │ 100     │ 100
   │  ├─ 3                           2.529 ms      │ 3.35 ms       │ 2.77 ms       │ 2.813 ms      │ 100     │ 100
   │  ╰─ 5                           2.505 ms      │ 3.576 ms      │ 2.833 ms      │ 2.86 ms       │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    11.34 ms      │ 18.48 ms      │ 12.77 ms      │ 12.92 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          13.79 ms      │ 18.76 ms      │ 17.01 ms      │ 16.55 ms      │ 100     │ 100
   │  ╰─ "none"                      2.082 ms      │ 2.587 ms      │ 2.159 ms      │ 2.187 ms      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        2.069 ms      │ 3.578 ms      │ 2.163 ms      │ 2.227 ms      │ 100     │ 100
      ├─ 10000                       1.799 ms      │ 2.456 ms      │ 1.874 ms      │ 1.91 ms       │ 100     │ 100
      ╰─ 50000                       1.761 ms      │ 2.384 ms      │ 1.816 ms      │ 1.849 ms      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
