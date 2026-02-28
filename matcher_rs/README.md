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
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           15.56 ms      │ 32.94 ms      │ 15.93 ms      │ 16.25 ms      │ 100     │ 100
│  │  ├─ 3                           52.38 ms      │ 79.45 ms      │ 53.66 ms      │ 54.41 ms      │ 92      │ 92
│  │  ╰─ 5                           91.64 ms      │ 123.8 ms      │ 95.42 ms      │ 97.54 ms      │ 52      │ 52
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    52.58 ms      │ 57.62 ms      │ 53.25 ms      │ 53.59 ms      │ 94      │ 94
│  │  ├─ "fanjian"                   52.66 ms      │ 59.06 ms      │ 53.55 ms      │ 53.92 ms      │ 93      │ 93
│  │  ├─ "fanjian_delete_normalize"  52.7 ms       │ 68.99 ms      │ 53.63 ms      │ 54.21 ms      │ 93      │ 93
│  │  ╰─ "none"                      53.08 ms      │ 75.71 ms      │ 54.14 ms      │ 54.81 ms      │ 92      │ 92
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        5.003 ms      │ 7.175 ms      │ 5.135 ms      │ 5.173 ms      │ 100     │ 100
│  │  ├─ 10000                       52.79 ms      │ 60.83 ms      │ 53.37 ms      │ 53.67 ms      │ 94      │ 94
│  │  ╰─ 50000                       219 ms        │ 245.5 ms      │ 221.3 ms      │ 223.8 ms      │ 23      │ 23
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           59.81 ms      │ 87.29 ms      │ 60.75 ms      │ 61.56 ms      │ 82      │ 82
│  │  ├─ 3                           124.3 ms      │ 143.9 ms      │ 125.7 ms      │ 127.5 ms      │ 40      │ 40
│  │  ╰─ 5                           213.1 ms      │ 230.8 ms      │ 215.2 ms      │ 217 ms        │ 24      │ 24
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    124.1 ms      │ 137.1 ms      │ 125.3 ms      │ 125.8 ms      │ 40      │ 40
│  │  ├─ "delete_normalize"          116.6 ms      │ 128.9 ms      │ 117.8 ms      │ 118.2 ms      │ 43      │ 43
│  │  ╰─ "none"                      124.6 ms      │ 134.8 ms      │ 125.8 ms      │ 126.6 ms      │ 40      │ 40
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        12.46 ms      │ 13.42 ms      │ 12.64 ms      │ 12.67 ms      │ 100     │ 100
│     ├─ 10000                       125.1 ms      │ 145.8 ms      │ 127 ms        │ 128.9 ms      │ 39      │ 39
│     ╰─ 50000                       605.6 ms      │ 630 ms        │ 608.4 ms      │ 613 ms        │ 9       │ 9
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    33.46 ms      │ 57.76 ms      │ 34.41 ms      │ 35.58 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   23.5 ms       │ 33.59 ms      │ 24.62 ms      │ 24.88 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  51.02 ms      │ 63.77 ms      │ 52.45 ms      │ 52.86 ms      │ 95      │ 95
│  │  ╰─ "none"                      21.45 ms      │ 30.05 ms      │ 22.52 ms      │ 22.72 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.596 ms      │ 5.097 ms      │ 4.692 ms      │ 4.691 ms      │ 100     │ 100
│  │  ├─ 10000                       21.38 ms      │ 25.9 ms       │ 22.01 ms      │ 22.15 ms      │ 100     │ 100
│  │  ╰─ 50000                       93.97 ms      │ 120.3 ms      │ 98.59 ms      │ 101.1 ms      │ 50      │ 50
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.333 ms      │ 1.487 ms      │ 1.356 ms      │ 1.362 ms      │ 100     │ 100
│  │  ├─ 3                           2.155 ms      │ 2.879 ms      │ 2.186 ms      │ 2.203 ms      │ 100     │ 100
│  │  ╰─ 5                           2.951 ms      │ 4.338 ms      │ 3.072 ms      │ 3.103 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    7.028 ms      │ 10.57 ms      │ 7.308 ms      │ 7.42 ms       │ 100     │ 100
│  │  ├─ "delete_normalize"          9.231 ms      │ 10.99 ms      │ 9.406 ms      │ 9.445 ms      │ 100     │ 100
│  │  ╰─ "none"                      2.152 ms      │ 2.877 ms      │ 2.191 ms      │ 2.212 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.102 ms      │ 1.306 ms      │ 1.119 ms      │ 1.124 ms      │ 100     │ 100
│     ├─ 10000                       2.162 ms      │ 3.023 ms      │ 2.252 ms      │ 2.254 ms      │ 100     │ 100
│     ╰─ 50000                       6.184 ms      │ 8.324 ms      │ 6.301 ms      │ 6.42 ms       │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.159 ms      │ 6.512 ms      │ 6.208 ms      │ 6.228 ms      │ 100     │ 100
   │  ├─ "fanjian"                   1.319 ms      │ 1.558 ms      │ 1.342 ms      │ 1.354 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  10.21 ms      │ 13.12 ms      │ 10.26 ms      │ 10.3 ms       │ 100     │ 100
   │  ╰─ "none"                      372.9 µs      │ 484.4 µs      │ 380 µs        │ 383.5 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        370 µs        │ 398.9 µs      │ 377.8 µs      │ 379 µs        │ 100     │ 100
   │  ├─ 10000                       375.1 µs      │ 418.6 µs      │ 388.3 µs      │ 389.2 µs      │ 100     │ 100
   │  ╰─ 50000                       369.4 µs      │ 432.8 µs      │ 378.2 µs      │ 380.8 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           929.7 µs      │ 1.029 ms      │ 942.4 µs      │ 946.5 µs      │ 100     │ 100
   │  ├─ 3                           914.6 µs      │ 991.2 µs      │ 934.2 µs      │ 938.2 µs      │ 100     │ 100
   │  ╰─ 5                           938.6 µs      │ 988.8 µs      │ 947.4 µs      │ 950.9 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    4.603 ms      │ 5.581 ms      │ 4.674 ms      │ 4.703 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          5.816 ms      │ 6.26 ms       │ 5.881 ms      │ 5.894 ms      │ 100     │ 100
   │  ╰─ "none"                      908.1 µs      │ 961.4 µs      │ 920.8 µs      │ 926.2 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        910.4 µs      │ 1.584 ms      │ 940.5 µs      │ 961.4 µs      │ 100     │ 100
      ├─ 10000                       920.8 µs      │ 1.013 ms      │ 934 µs        │ 938.4 µs      │ 100     │ 100
      ╰─ 50000                       912.4 µs      │ 1.009 ms      │ 926.3 µs      │ 932.2 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
