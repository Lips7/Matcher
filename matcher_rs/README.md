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
│  │  ├─ 1                           911.7 µs      │ 994.3 µs      │ 926.5 µs      │ 928.5 µs      │ 100     │ 100
│  │  ├─ 3                           872.7 µs      │ 921.4 µs      │ 885.1 µs      │ 886.4 µs      │ 100     │ 100
│  │  ╰─ 5                           920.6 µs      │ 965.4 µs      │ 929.1 µs      │ 931 µs        │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    26.28 ms      │ 39.51 ms      │ 27.02 ms      │ 27.46 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   16.97 ms      │ 21.16 ms      │ 17.88 ms      │ 18.22 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  44.53 ms      │ 60.24 ms      │ 45.75 ms      │ 46.01 ms      │ 100     │ 100
│  │  ╰─ "none"                      15.03 ms      │ 19.09 ms      │ 15.78 ms      │ 16.15 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.036 ms      │ 4.434 ms      │ 4.123 ms      │ 4.136 ms      │ 100     │ 100
│  │  ├─ 10000                       15.05 ms      │ 18.62 ms      │ 15.92 ms      │ 16.3 ms       │ 100     │ 100
│  │  ╰─ 50000                       59.69 ms      │ 86.06 ms      │ 63.51 ms      │ 64.48 ms      │ 78      │ 78
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.184 ms      │ 1.39 ms       │ 1.192 ms      │ 1.196 ms      │ 100     │ 100
│  │  ├─ 3                           1.903 ms      │ 2.806 ms      │ 1.914 ms      │ 1.935 ms      │ 100     │ 100
│  │  ╰─ 5                           2.502 ms      │ 3.607 ms      │ 2.525 ms      │ 2.576 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    6.39 ms       │ 7.956 ms      │ 6.61 ms       │ 6.572 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          8.563 ms      │ 10.17 ms      │ 8.776 ms      │ 8.739 ms      │ 100     │ 100
│  │  ╰─ "none"                      1.894 ms      │ 2.615 ms      │ 1.912 ms      │ 1.95 ms       │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        920.2 µs      │ 970.8 µs      │ 930.8 µs      │ 932.5 µs      │ 100     │ 100
│     ├─ 10000                       1.899 ms      │ 2.687 ms      │ 2.002 ms      │ 1.999 ms      │ 100     │ 100
│     ╰─ 50000                       5.063 ms      │ 7.859 ms      │ 5.553 ms      │ 5.817 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           550.7 µs      │ 644.4 µs      │ 560.6 µs      │ 564.9 µs      │ 100     │ 100
   │  ├─ 3                           554.8 µs      │ 601.7 µs      │ 558.4 µs      │ 559.9 µs      │ 100     │ 100
   │  ╰─ 5                           551.1 µs      │ 586.7 µs      │ 558.4 µs      │ 559.6 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    5.453 ms      │ 7.724 ms      │ 5.485 ms      │ 5.545 ms      │ 100     │ 100
   │  ├─ "fanjian"                   1.283 ms      │ 1.338 ms      │ 1.292 ms      │ 1.293 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  9.465 ms      │ 9.695 ms      │ 9.508 ms      │ 9.515 ms      │ 100     │ 100
   │  ╰─ "none"                      259.5 µs      │ 291.3 µs      │ 262.2 µs      │ 264.1 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        257.2 µs      │ 287.5 µs      │ 260.1 µs      │ 262 µs        │ 100     │ 100
   │  ├─ 10000                       258.7 µs      │ 296.5 µs      │ 261.5 µs      │ 262.9 µs      │ 100     │ 100
   │  ╰─ 50000                       258.3 µs      │ 303.3 µs      │ 260.7 µs      │ 262.3 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           545.9 µs      │ 608.9 µs      │ 557.1 µs      │ 561.2 µs      │ 100     │ 100
   │  ├─ 3                           543.3 µs      │ 579.7 µs      │ 547.7 µs      │ 549.2 µs      │ 100     │ 100
   │  ╰─ 5                           545.3 µs      │ 582.5 µs      │ 553.3 µs      │ 553.6 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.575 ms      │ 3.938 ms      │ 3.608 ms      │ 3.633 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.884 ms      │ 5.035 ms      │ 4.917 ms      │ 4.922 ms      │ 100     │ 100
   │  ╰─ "none"                      548 µs        │ 602.1 µs      │ 553.8 µs      │ 555.9 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        543.5 µs      │ 606.9 µs      │ 551.6 µs      │ 553.4 µs      │ 100     │ 100
      ├─ 10000                       548 µs        │ 580.9 µs      │ 553.6 µs      │ 555.4 µs      │ 100     │ 100
      ╰─ 50000                       551.6 µs      │ 621.2 µs      │ 566.9 µs      │ 567.9 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
