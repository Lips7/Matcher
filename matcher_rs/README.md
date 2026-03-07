# Matcher

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

- **Multiple Matching Methods**:
  - Simple Word Matching
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
│  │  ├─ 1                           17.07 ms      │ 30.18 ms      │ 18.31 ms      │ 19.86 ms      │ 100     │ 100
│  │  ├─ 3                           59.11 ms      │ 83.1 ms       │ 61.67 ms      │ 63.44 ms      │ 79      │ 79
│  │  ╰─ 5                           100.7 ms      │ 143.4 ms      │ 107.7 ms      │ 111.1 ms      │ 45      │ 45
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    58.54 ms      │ 115.5 ms      │ 62.66 ms      │ 67.48 ms      │ 75      │ 75
│  │  ├─ "fanjian"                   61.73 ms      │ 124.5 ms      │ 63.92 ms      │ 67.94 ms      │ 74      │ 74
│  │  ├─ "fanjian_delete_normalize"  57.71 ms      │ 156.7 ms      │ 61.7 ms       │ 68.68 ms      │ 73      │ 73
│  │  ╰─ "none"                      59.17 ms      │ 135.9 ms      │ 61.54 ms      │ 66.82 ms      │ 75      │ 75
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        5.238 ms      │ 8.369 ms      │ 5.608 ms      │ 5.755 ms      │ 100     │ 100
│  │  ├─ 10000                       59.79 ms      │ 108.5 ms      │ 65.57 ms      │ 68.2 ms       │ 74      │ 74
│  │  ╰─ 50000                       398 ms        │ 600.2 ms      │ 459 ms        │ 472.8 ms      │ 11      │ 11
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.27 ms       │ 11.93 ms      │ 9.823 ms      │ 9.889 ms      │ 100     │ 100
│  │  ├─ 3                           34.9 ms       │ 43.92 ms      │ 35.92 ms      │ 36.52 ms      │ 100     │ 100
│  │  ╰─ 5                           64.3 ms       │ 101.2 ms      │ 68.3 ms       │ 72.67 ms      │ 69      │ 69
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    34.72 ms      │ 85 ms         │ 42.63 ms      │ 44.69 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          41.3 ms       │ 45.84 ms      │ 42.01 ms      │ 42.18 ms      │ 100     │ 100
│  │  ╰─ "none"                      34.8 ms       │ 67.55 ms      │ 36.34 ms      │ 37.86 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        3.055 ms      │ 3.559 ms      │ 3.125 ms      │ 3.136 ms      │ 100     │ 100
│     ├─ 10000                       34.92 ms      │ 80.35 ms      │ 35.81 ms      │ 38.65 ms      │ 100     │ 100
│     ╰─ 50000                       275.9 ms      │ 428.8 ms      │ 291.4 ms      │ 310.8 ms      │ 17      │ 17
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.134 ms      │ 4.053 ms      │ 1.206 ms      │ 1.285 ms      │ 100     │ 100
│  │  ├─ 3                           1.073 ms      │ 2.111 ms      │ 1.119 ms      │ 1.189 ms      │ 100     │ 100
│  │  ╰─ 5                           1.154 ms      │ 2.16 ms       │ 1.198 ms      │ 1.254 ms      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    22.76 ms      │ 69.62 ms      │ 27.8 ms       │ 31.16 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   13.03 ms      │ 78.68 ms      │ 17.05 ms      │ 19.45 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  36.95 ms      │ 51.5 ms       │ 38.18 ms      │ 38.9 ms       │ 100     │ 100
│  │  ╰─ "none"                      10.19 ms      │ 14.91 ms      │ 10.59 ms      │ 10.82 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.38 ms       │ 4.618 ms      │ 4.421 ms      │ 4.431 ms      │ 100     │ 100
│  │  ├─ 10000                       10.15 ms      │ 13.02 ms      │ 10.45 ms      │ 10.51 ms      │ 100     │ 100
│  │  ╰─ 50000                       40.04 ms      │ 72.07 ms      │ 43.29 ms      │ 44.1 ms       │ 100     │ 100
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.436 ms      │ 1.734 ms      │ 1.477 ms      │ 1.486 ms      │ 100     │ 100
│  │  ├─ 3                           4.236 ms      │ 5.28 ms       │ 4.399 ms      │ 4.406 ms      │ 100     │ 100
│  │  ╰─ 5                           8.949 ms      │ 22.65 ms      │ 9.208 ms      │ 9.943 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    9.789 ms      │ 11.23 ms      │ 9.939 ms      │ 9.986 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          49.1 ms       │ 93.61 ms      │ 50.27 ms      │ 51.88 ms      │ 97      │ 97
│  │  ╰─ "none"                      4.244 ms      │ 5.434 ms      │ 4.417 ms      │ 4.437 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.119 ms      │ 1.204 ms      │ 1.139 ms      │ 1.141 ms      │ 100     │ 100
│     ├─ 10000                       4.314 ms      │ 5.26 ms       │ 4.431 ms      │ 4.443 ms      │ 100     │ 100
│     ╰─ 50000                       29.74 ms      │ 120.4 ms      │ 46.21 ms      │ 49.3 ms       │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           558.3 µs      │ 1.097 ms      │ 631.6 µs      │ 648.5 µs      │ 100     │ 100
   │  ├─ 3                           556.4 µs      │ 884 µs        │ 606.9 µs      │ 627.3 µs      │ 100     │ 100
   │  ╰─ 5                           517.8 µs      │ 605.9 µs      │ 549.9 µs      │ 554.2 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.67 ms       │ 7.606 ms      │ 6.744 ms      │ 6.776 ms      │ 100     │ 100
   │  ├─ "fanjian"                   2.331 ms      │ 2.511 ms      │ 2.388 ms      │ 2.395 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  12.48 ms      │ 13.32 ms      │ 12.66 ms      │ 12.68 ms      │ 100     │ 100
   │  ╰─ "none"                      768.9 µs      │ 880.9 µs      │ 792.4 µs      │ 800.5 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        785.1 µs      │ 943.9 µs      │ 819.6 µs      │ 823.2 µs      │ 100     │ 100
   │  ├─ 10000                       789.2 µs      │ 888.2 µs      │ 815.3 µs      │ 817.2 µs      │ 100     │ 100
   │  ╰─ 50000                       771.2 µs      │ 881.1 µs      │ 808.4 µs      │ 814.2 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           536 µs        │ 663.6 µs      │ 549.4 µs      │ 553.8 µs      │ 100     │ 100
   │  ├─ 3                           542.2 µs      │ 602.2 µs      │ 550.2 µs      │ 554.4 µs      │ 100     │ 100
   │  ╰─ 5                           540 µs        │ 686.4 µs      │ 548.3 µs      │ 553.6 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.612 ms      │ 3.85 ms       │ 3.686 ms      │ 3.699 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.897 ms      │ 5.209 ms      │ 4.97 ms       │ 4.976 ms      │ 100     │ 100
   │  ╰─ "none"                      507.7 µs      │ 643.2 µs      │ 535.9 µs      │ 538.4 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        532.9 µs      │ 614.7 µs      │ 544 µs        │ 548.5 µs      │ 100     │ 100
      ├─ 10000                       532.6 µs      │ 619.2 µs      │ 557.2 µs      │ 559.3 µs      │ 100     │ 100
      ╰─ 50000                       554.7 µs      │ 10.05 ms      │ 654 µs        │ 1.098 ms      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
