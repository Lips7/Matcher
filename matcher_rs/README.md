# Matcher

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

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
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Usage

### Adding to Your Project

To use `matcher_rs` in your Rust project, run the following command:

```shell
cargo add matcher_rs
```

### Explanation of the configuration

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

Here’s a basic example of how to use the `SimpleMatcher` for text matching:

```rust
use matcher_rs::{text_process, reduce_text_process, ProcessType};

let result = text_process(ProcessType::Delete, "你好，世界！");
let results = reduce_text_process(ProcessType::FanjianDeleteNormalize, "你好，世界！");
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

For more detailed usage examples, please refer to the [test_simple_matcher.rs](./tests/test_simple_matcher.rs) file.

## Feature Flags
* `runtime_build`: By enable runtime_build feature, we could build process matcher at runtime, but with build time increasing.
* `dfa`: By enable dfa feature, we could use dfa to perform simple matching, but with significantly increasing memory consumption.
* `vectorscan`: By enable vectorscan feature, we could use vectorscan to perform simple matching.

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
│  │  ├─ 1                           17.23 ms      │ 23.36 ms      │ 17.74 ms      │ 17.83 ms      │ 100     │ 100
│  │  ├─ 3                           57.55 ms      │ 156.8 ms      │ 59.5 ms       │ 63.02 ms      │ 80      │ 80
│  │  ╰─ 5                           100.2 ms      │ 141.1 ms      │ 102.9 ms      │ 105 ms        │ 48      │ 48
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    57.89 ms      │ 61.59 ms      │ 59.21 ms      │ 59.23 ms      │ 85      │ 85
│  │  ├─ "fanjian"                   59.18 ms      │ 95.35 ms      │ 60.69 ms      │ 61.88 ms      │ 81      │ 81
│  │  ├─ "fanjian_delete_normalize"  56.23 ms      │ 68.83 ms      │ 57.94 ms      │ 58.47 ms      │ 86      │ 86
│  │  ╰─ "none"                      58.5 ms       │ 62.89 ms      │ 59.7 ms       │ 59.79 ms      │ 84      │ 84
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.955 ms      │ 5.788 ms      │ 5.074 ms      │ 5.102 ms      │ 100     │ 100
│  │  ├─ 10000                       57.88 ms      │ 60.72 ms      │ 59.16 ms      │ 59.21 ms      │ 85      │ 85
│  │  ╰─ 50000                       380.3 ms      │ 395.3 ms      │ 385.2 ms      │ 385.9 ms      │ 13      │ 13
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.045 ms      │ 10.11 ms      │ 9.231 ms      │ 9.243 ms      │ 100     │ 100
│  │  ├─ 3                           33.93 ms      │ 43.36 ms      │ 35.04 ms      │ 35.04 ms      │ 100     │ 100
│  │  ╰─ 5                           62.88 ms      │ 65.78 ms      │ 64.15 ms      │ 64.18 ms      │ 78      │ 78
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    33.5 ms       │ 38.23 ms      │ 34.76 ms      │ 35.01 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          40.49 ms      │ 85.58 ms      │ 41.79 ms      │ 42.58 ms      │ 100     │ 100
│  │  ╰─ "none"                      33.79 ms      │ 35.84 ms      │ 34.72 ms      │ 34.75 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        3.056 ms      │ 3.243 ms      │ 3.097 ms      │ 3.107 ms      │ 100     │ 100
│     ├─ 10000                       33.78 ms      │ 37.76 ms      │ 34.51 ms      │ 34.72 ms      │ 100     │ 100
│     ╰─ 50000                       256 ms        │ 278.1 ms      │ 260.5 ms      │ 261.8 ms      │ 20      │ 20
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.081 ms      │ 1.179 ms      │ 1.09 ms       │ 1.094 ms      │ 100     │ 100
│  │  ├─ 3                           960.2 µs      │ 1.086 ms      │ 1.033 ms      │ 1.034 ms      │ 100     │ 100
│  │  ╰─ 5                           1.067 ms      │ 1.135 ms      │ 1.098 ms      │ 1.1 ms        │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    19.92 ms      │ 31.45 ms      │ 20.29 ms      │ 20.43 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   10.8 ms       │ 13.19 ms      │ 11.12 ms      │ 11.13 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  32.68 ms      │ 35.1 ms       │ 33.22 ms      │ 33.26 ms      │ 100     │ 100
│  │  ╰─ "none"                      9.231 ms      │ 11.59 ms      │ 9.505 ms      │ 9.507 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.198 ms      │ 4.397 ms      │ 4.228 ms      │ 4.237 ms      │ 100     │ 100
│  │  ├─ 10000                       9.147 ms      │ 11.42 ms      │ 9.518 ms      │ 9.53 ms       │ 100     │ 100
│  │  ╰─ 50000                       32.61 ms      │ 37.82 ms      │ 34.89 ms      │ 34.96 ms      │ 100     │ 100
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.354 ms      │ 1.512 ms      │ 1.368 ms      │ 1.373 ms      │ 100     │ 100
│  │  ├─ 3                           3.694 ms      │ 4.565 ms      │ 3.813 ms      │ 3.813 ms      │ 100     │ 100
│  │  ╰─ 5                           6.847 ms      │ 10.76 ms      │ 7.051 ms      │ 7.238 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    8.668 ms      │ 12.62 ms      │ 8.856 ms      │ 9.005 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          37.78 ms      │ 41.11 ms      │ 38.72 ms      │ 38.64 ms      │ 100     │ 100
│  │  ╰─ "none"                      3.627 ms      │ 4.72 ms       │ 3.671 ms      │ 3.714 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.041 ms      │ 1.12 ms       │ 1.063 ms      │ 1.064 ms      │ 100     │ 100
│     ├─ 10000                       3.725 ms      │ 4.442 ms      │ 3.787 ms      │ 3.802 ms      │ 100     │ 100
│     ╰─ 50000                       20.92 ms      │ 25.82 ms      │ 22.14 ms      │ 22.18 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           483.8 µs      │ 534.5 µs      │ 490.7 µs      │ 492.7 µs      │ 100     │ 100
   │  ├─ 3                           481.3 µs      │ 519.4 µs      │ 485.6 µs      │ 487.2 µs      │ 100     │ 100
   │  ╰─ 5                           485.9 µs      │ 536.6 µs      │ 493.1 µs      │ 495.4 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.558 ms      │ 8.859 ms      │ 6.599 ms      │ 6.636 ms      │ 100     │ 100
   │  ├─ "fanjian"                   2.093 ms      │ 2.16 ms       │ 2.105 ms      │ 2.11 ms       │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  11.95 ms      │ 12.41 ms      │ 12.03 ms      │ 12.04 ms      │ 100     │ 100
   │  ╰─ "none"                      731 µs        │ 848.4 µs      │ 758.2 µs      │ 763.4 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        737.7 µs      │ 867.7 µs      │ 773.7 µs      │ 779.5 µs      │ 100     │ 100
   │  ├─ 10000                       716.2 µs      │ 811.4 µs      │ 730.3 µs      │ 735.2 µs      │ 100     │ 100
   │  ╰─ 50000                       718.1 µs      │ 2.18 ms       │ 786 µs        │ 966.7 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           474.2 µs      │ 646.2 µs      │ 492.4 µs      │ 500 µs        │ 100     │ 100
   │  ├─ 3                           474.4 µs      │ 696.6 µs      │ 496.1 µs      │ 507.6 µs      │ 100     │ 100
   │  ╰─ 5                           489.3 µs      │ 574.7 µs      │ 497.8 µs      │ 509.7 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.406 ms      │ 3.871 ms      │ 3.491 ms      │ 3.512 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.544 ms      │ 5.141 ms      │ 4.589 ms      │ 4.62 ms       │ 100     │ 100
   │  ╰─ "none"                      484 µs        │ 2.391 ms      │ 503.8 µs      │ 549 µs        │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        482 µs        │ 523.4 µs      │ 489.9 µs      │ 491.8 µs      │ 100     │ 100
      ├─ 10000                       486.4 µs      │ 525.2 µs      │ 491.8 µs      │ 493.6 µs      │ 100     │ 100
      ╰─ 50000                       480.4 µs      │ 554.9 µs      │ 491.7 µs      │ 497.2 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
