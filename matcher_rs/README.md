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
│  │  ├─ 1                           17.55 ms      │ 29.89 ms      │ 18.07 ms      │ 18.41 ms      │ 100     │ 100
│  │  ├─ 3                           57.63 ms      │ 161.3 ms      │ 59.08 ms      │ 61.61 ms      │ 82      │ 82
│  │  ╰─ 5                           101.1 ms      │ 292.3 ms      │ 104.6 ms      │ 111.6 ms      │ 47      │ 47
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    58.25 ms      │ 73.38 ms      │ 60.09 ms      │ 60.98 ms      │ 82      │ 82
│  │  ├─ "fanjian"                   58.74 ms      │ 72.04 ms      │ 60.1 ms       │ 61.07 ms      │ 82      │ 82
│  │  ├─ "fanjian_delete_normalize"  59.17 ms      │ 187.4 ms      │ 60.65 ms      │ 63.67 ms      │ 79      │ 79
│  │  ╰─ "none"                      58.55 ms      │ 70.59 ms      │ 60.01 ms      │ 60.89 ms      │ 83      │ 83
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        5.409 ms      │ 9.509 ms      │ 5.52 ms       │ 5.632 ms      │ 100     │ 100
│  │  ├─ 10000                       58.58 ms      │ 113.5 ms      │ 61.26 ms      │ 63.2 ms       │ 80      │ 80
│  │  ╰─ 50000                       252 ms        │ 408.4 ms      │ 261.9 ms      │ 273.9 ms      │ 19      │ 19
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           66.34 ms      │ 179 ms        │ 68.76 ms      │ 74.65 ms      │ 67      │ 67
│  │  ├─ 3                           144.2 ms      │ 275.9 ms      │ 149.6 ms      │ 161.7 ms      │ 31      │ 31
│  │  ╰─ 5                           246.1 ms      │ 264.9 ms      │ 248.5 ms      │ 251.2 ms      │ 20      │ 20
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    143.5 ms      │ 291.9 ms      │ 149.1 ms      │ 160 ms        │ 32      │ 32
│  │  ├─ "delete_normalize"          133.1 ms      │ 149.8 ms      │ 135.4 ms      │ 138.6 ms      │ 37      │ 37
│  │  ╰─ "none"                      142.6 ms      │ 157.8 ms      │ 144.3 ms      │ 145.6 ms      │ 35      │ 35
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        12.88 ms      │ 14.2 ms       │ 13.2 ms       │ 13.25 ms      │ 100     │ 100
│     ├─ 10000                       144.6 ms      │ 184.4 ms      │ 148.3 ms      │ 150.9 ms      │ 34      │ 34
│     ╰─ 50000                       719.4 ms      │ 895.2 ms      │ 760.5 ms      │ 772.1 ms      │ 7       │ 7
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.063 ms      │ 1.203 ms      │ 1.08 ms       │ 1.087 ms      │ 100     │ 100
│  │  ├─ 3                           1.059 ms      │ 1.147 ms      │ 1.076 ms      │ 1.079 ms      │ 100     │ 100
│  │  ╰─ 5                           1.066 ms      │ 1.152 ms      │ 1.09 ms       │ 1.093 ms      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    27.72 ms      │ 41.91 ms      │ 28.84 ms      │ 29.35 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   18.27 ms      │ 32.56 ms      │ 19.08 ms      │ 19.51 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  46.26 ms      │ 62.02 ms      │ 47.74 ms      │ 48.44 ms      │ 100     │ 100
│  │  ╰─ "none"                      16.23 ms      │ 25.45 ms      │ 17.66 ms      │ 18.2 ms       │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.043 ms      │ 5.396 ms      │ 4.145 ms      │ 4.214 ms      │ 100     │ 100
│  │  ├─ 10000                       16.21 ms      │ 29.69 ms      │ 17.21 ms      │ 17.59 ms      │ 100     │ 100
│  │  ╰─ 50000                       73.82 ms      │ 99.08 ms      │ 78.3 ms       │ 79.59 ms      │ 63      │ 63
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.291 ms      │ 1.461 ms      │ 1.313 ms      │ 1.318 ms      │ 100     │ 100
│  │  ├─ 3                           1.903 ms      │ 2.698 ms      │ 1.938 ms      │ 1.971 ms      │ 100     │ 100
│  │  ╰─ 5                           2.461 ms      │ 3.752 ms      │ 2.649 ms      │ 2.613 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    6.257 ms      │ 7.893 ms      │ 6.38 ms       │ 6.419 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          8.506 ms      │ 16.98 ms      │ 8.736 ms      │ 8.926 ms      │ 100     │ 100
│  │  ╰─ "none"                      1.915 ms      │ 3.54 ms       │ 1.972 ms      │ 2.048 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.058 ms      │ 1.244 ms      │ 1.081 ms      │ 1.086 ms      │ 100     │ 100
│     ├─ 10000                       1.912 ms      │ 2.699 ms      │ 1.955 ms      │ 1.984 ms      │ 100     │ 100
│     ╰─ 50000                       4.652 ms      │ 7.093 ms      │ 5.15 ms       │ 5.155 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           736.7 µs      │ 865.3 µs      │ 761.8 µs      │ 766.9 µs      │ 100     │ 100
   │  ├─ 3                           749.9 µs      │ 818.6 µs      │ 770.8 µs      │ 774.1 µs      │ 100     │ 100
   │  ╰─ 5                           737.8 µs      │ 801.4 µs      │ 755.4 µs      │ 755.8 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    5.502 ms      │ 5.986 ms      │ 5.551 ms      │ 5.57 ms       │ 100     │ 100
   │  ├─ "fanjian"                   1.33 ms       │ 1.398 ms      │ 1.348 ms      │ 1.352 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  9.571 ms      │ 13.16 ms      │ 9.693 ms      │ 9.823 ms      │ 100     │ 100
   │  ╰─ "none"                      311.4 µs      │ 344 µs        │ 318 µs        │ 319.9 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        307.4 µs      │ 379.1 µs      │ 319.1 µs      │ 322.9 µs      │ 100     │ 100
   │  ├─ 10000                       308 µs        │ 350.2 µs      │ 318.3 µs      │ 321.2 µs      │ 100     │ 100
   │  ╰─ 50000                       315.7 µs      │ 1.691 ms      │ 333.2 µs      │ 481.2 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           725 µs        │ 810.2 µs      │ 741.7 µs      │ 744.1 µs      │ 100     │ 100
   │  ├─ 3                           738.3 µs      │ 828.4 µs      │ 758.2 µs      │ 764.3 µs      │ 100     │ 100
   │  ╰─ 5                           727 µs        │ 787.1 µs      │ 739.6 µs      │ 742.7 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.816 ms      │ 4.719 ms      │ 3.869 ms      │ 3.89 ms       │ 100     │ 100
   │  ├─ "delete_normalize"          5.224 ms      │ 5.965 ms      │ 5.38 ms       │ 5.416 ms      │ 100     │ 100
   │  ╰─ "none"                      728.2 µs      │ 776.1 µs      │ 745.9 µs      │ 746.8 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        729.3 µs      │ 818.7 µs      │ 743.9 µs      │ 748.7 µs      │ 100     │ 100
      ├─ 10000                       743.1 µs      │ 828.3 µs      │ 763.1 µs      │ 769.6 µs      │ 100     │ 100
      ╰─ 50000                       731.1 µs      │ 783.2 µs      │ 743.4 µs      │ 747.2 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
