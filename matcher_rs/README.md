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

* `Matcher`'s configuration is built using `MatcherBuilder` and `MatchTableBuilder`.
* `SimpleMatcher`'s configuration is built using `SimpleMatcherBuilder`. For each `SimpleMatcher`, the added `word_id` is required to be globally unique.

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
use matcher_rs::{MatcherBuilder, MatchTableBuilder, MatchTableType, ProcessType};

let table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::FanjianDeleteNormalize })
    .add_words(["example", "test"])
    .build();

let matcher = MatcherBuilder::new()
    .add_table(1, table)
    .build();

let text = "This is an example text.";
let results = matcher.word_match(text);
```

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::Fanjian, 1, "你好")
    .add_word(ProcessType::Fanjian, 2, "世界")
    .build();

let text = "你好，世界！";
let results = matcher.process(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Feature Flags
* `runtime_build`: By enable runtime_build feature, we could build process matcher at runtime, but with build time increasing.
* `dfa`: By enable dfa feature, we could use dfa to perform simple matching, but with significantly increasing memory consumption.

Default feature is `dfa`.

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
Timer precision: 41 ns
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           7.761 ms      │ 11.14 ms      │ 8.053 ms      │ 8.153 ms      │ 100     │ 100
│  │  ├─ 3                           25.6 ms       │ 59.3 ms       │ 28.03 ms      │ 29.63 ms      │ 100     │ 100
│  │  ╰─ 5                           44.68 ms      │ 74.26 ms      │ 47.95 ms      │ 49.66 ms      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    25.37 ms      │ 45.72 ms      │ 26.11 ms      │ 26.57 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   25.69 ms      │ 55.01 ms      │ 27.2 ms       │ 27.64 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  25.96 ms      │ 48.89 ms      │ 27.3 ms       │ 27.88 ms      │ 100     │ 100
│  │  ╰─ "none"                      25.94 ms      │ 62.33 ms      │ 28.24 ms      │ 29.9 ms       │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        2.261 ms      │ 3.293 ms      │ 2.311 ms      │ 2.36 ms       │ 100     │ 100
│  │  ├─ 10000                       25.48 ms      │ 28.64 ms      │ 25.91 ms      │ 25.96 ms      │ 100     │ 100
│  │  ╰─ 50000                       105.3 ms      │ 152.1 ms      │ 109.2 ms      │ 111.9 ms      │ 45      │ 45
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.651 ms      │ 10.92 ms      │ 9.956 ms      │ 9.973 ms      │ 100     │ 100
│  │  ├─ 3                           25.42 ms      │ 40.48 ms      │ 26.35 ms      │ 26.62 ms      │ 100     │ 100
│  │  ╰─ 5                           43.95 ms      │ 73.28 ms      │ 46.61 ms      │ 48.27 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    24.87 ms      │ 31.21 ms      │ 25.66 ms      │ 25.9 ms       │ 100     │ 100
│  │  ├─ "delete_normalize"          25.72 ms      │ 52.05 ms      │ 26.59 ms      │ 27.05 ms      │ 100     │ 100
│  │  ╰─ "none"                      24.98 ms      │ 41.02 ms      │ 25.74 ms      │ 26.04 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        2.443 ms      │ 3.13 ms       │ 2.56 ms       │ 2.575 ms      │ 100     │ 100
│     ├─ 10000                       25.07 ms      │ 45.75 ms      │ 25.94 ms      │ 26.23 ms      │ 100     │ 100
│     ╰─ 50000                       120.6 ms      │ 237.2 ms      │ 126.1 ms      │ 133.9 ms      │ 38      │ 38
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           899.4 µs      │ 1.303 ms      │ 917.6 µs      │ 925.8 µs      │ 100     │ 100
│  │  ├─ 3                           902.7 µs      │ 991.6 µs      │ 912.8 µs      │ 917.2 µs      │ 100     │ 100
│  │  ╰─ 5                           909.7 µs      │ 958.2 µs      │ 922.7 µs      │ 924.6 µs      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    25.76 ms      │ 40.26 ms      │ 26.65 ms      │ 26.79 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   15.52 ms      │ 19.32 ms      │ 15.94 ms      │ 16.19 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  43.16 ms      │ 55.93 ms      │ 44.51 ms      │ 44.71 ms      │ 100     │ 100
│  │  ╰─ "none"                      13.68 ms      │ 18.05 ms      │ 14.15 ms      │ 14.43 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        3.798 ms      │ 9.419 ms      │ 3.89 ms       │ 4.016 ms      │ 100     │ 100
│  │  ├─ 10000                       13.68 ms      │ 17.25 ms      │ 14.54 ms      │ 14.48 ms      │ 100     │ 100
│  │  ╰─ 50000                       60.75 ms      │ 73.84 ms      │ 64.75 ms      │ 64.65 ms      │ 78      │ 78
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.141 ms      │ 1.301 ms      │ 1.153 ms      │ 1.156 ms      │ 100     │ 100
│  │  ├─ 3                           1.69 ms       │ 2.435 ms      │ 1.698 ms      │ 1.725 ms      │ 100     │ 100
│  │  ╰─ 5                           2.284 ms      │ 3.392 ms      │ 2.416 ms      │ 2.401 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    5.894 ms      │ 8.537 ms      │ 6.129 ms      │ 6.124 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          8.132 ms      │ 9.821 ms      │ 8.37 ms       │ 8.326 ms      │ 100     │ 100
│  │  ╰─ "none"                      1.7 ms        │ 2.716 ms      │ 1.734 ms      │ 1.772 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        908.2 µs      │ 978.2 µs      │ 919.4 µs      │ 922.2 µs      │ 100     │ 100
│     ├─ 10000                       1.686 ms      │ 2.45 ms       │ 1.701 ms      │ 1.721 ms      │ 100     │ 100
│     ╰─ 50000                       4.239 ms      │ 6.585 ms      │ 4.658 ms      │ 4.702 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           512.2 µs      │ 597.5 µs      │ 539 µs        │ 537.5 µs      │ 100     │ 100
   │  ├─ 3                           528.5 µs      │ 571.4 µs      │ 536.3 µs      │ 537 µs        │ 100     │ 100
   │  ╰─ 5                           537.4 µs      │ 572.6 µs      │ 541.3 µs      │ 543.4 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    5.408 ms      │ 5.531 ms      │ 5.438 ms      │ 5.444 ms      │ 100     │ 100
   │  ├─ "fanjian"                   1.281 ms      │ 1.343 ms      │ 1.294 ms      │ 1.296 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  9.387 ms      │ 9.652 ms      │ 9.454 ms      │ 9.467 ms      │ 100     │ 100
   │  ╰─ "none"                      259.7 µs      │ 300.2 µs      │ 263.1 µs      │ 265.5 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        258.7 µs      │ 287 µs        │ 260.8 µs      │ 263 µs        │ 100     │ 100
   │  ├─ 10000                       259.9 µs      │ 299.3 µs      │ 262.3 µs      │ 264.1 µs      │ 100     │ 100
   │  ╰─ 50000                       259.1 µs      │ 295.5 µs      │ 261.5 µs      │ 264.1 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           533.4 µs      │ 587.9 µs      │ 545 µs        │ 547.2 µs      │ 100     │ 100
   │  ├─ 3                           542.5 µs      │ 577.2 µs      │ 550.5 µs      │ 552.2 µs      │ 100     │ 100
   │  ╰─ 5                           541.8 µs      │ 567.7 µs      │ 549 µs        │ 549.9 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.523 ms      │ 3.692 ms      │ 3.566 ms      │ 3.573 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.84 ms       │ 5.108 ms      │ 4.881 ms      │ 4.89 ms       │ 100     │ 100
   │  ╰─ "none"                      543.2 µs      │ 594.7 µs      │ 548.1 µs      │ 549.2 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        537.6 µs      │ 706.7 µs      │ 549.9 µs      │ 556.8 µs      │ 100     │ 100
      ├─ 10000                       543.1 µs      │ 585.2 µs      │ 551.4 µs      │ 553.5 µs      │ 100     │ 100
      ╰─ 50000                       513.9 µs      │ 613.4 µs      │ 546.4 µs      │ 549.3 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
