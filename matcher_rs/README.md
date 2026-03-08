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
│  │  ├─ 1                           17.65 ms      │ 21.51 ms      │ 18.12 ms      │ 18.35 ms      │ 100     │ 100
│  │  ├─ 3                           59.09 ms      │ 71.6 ms       │ 62.42 ms      │ 62.55 ms      │ 80      │ 80
│  │  ╰─ 5                           101.3 ms      │ 137.1 ms      │ 104 ms        │ 106.4 ms      │ 47      │ 47
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    58.86 ms      │ 97.93 ms      │ 60.5 ms       │ 61.84 ms      │ 81      │ 81
│  │  ├─ "fanjian"                   60.95 ms      │ 116.8 ms      │ 64.1 ms       │ 66.7 ms       │ 75      │ 75
│  │  ├─ "fanjian_delete_normalize"  58.01 ms      │ 121.6 ms      │ 61.39 ms      │ 63.49 ms      │ 79      │ 79
│  │  ╰─ "none"                      58.74 ms      │ 93.08 ms      │ 60.29 ms      │ 61.52 ms      │ 82      │ 82
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        5.03 ms       │ 5.655 ms      │ 5.19 ms       │ 5.227 ms      │ 100     │ 100
│  │  ├─ 10000                       60 ms         │ 106.5 ms      │ 61.35 ms      │ 63.15 ms      │ 80      │ 80
│  │  ╰─ 50000                       396.3 ms      │ 496.4 ms      │ 415.8 ms      │ 424.5 ms      │ 12      │ 12
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.157 ms      │ 9.944 ms      │ 9.445 ms      │ 9.464 ms      │ 100     │ 100
│  │  ├─ 3                           35.41 ms      │ 109.4 ms      │ 36.85 ms      │ 38.91 ms      │ 100     │ 100
│  │  ╰─ 5                           65.3 ms       │ 127.7 ms      │ 68.23 ms      │ 70.57 ms      │ 71      │ 71
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    34.16 ms      │ 64.97 ms      │ 35.52 ms      │ 36.15 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          41.85 ms      │ 87.95 ms      │ 43.72 ms      │ 45.15 ms      │ 100     │ 100
│  │  ╰─ "none"                      34.53 ms      │ 49.64 ms      │ 35.51 ms      │ 35.98 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        3.05 ms       │ 3.539 ms      │ 3.108 ms      │ 3.125 ms      │ 100     │ 100
│     ├─ 10000                       34.59 ms      │ 63.37 ms      │ 35.45 ms      │ 36 ms         │ 100     │ 100
│     ╰─ 50000                       263.7 ms      │ 306.7 ms      │ 273.7 ms      │ 276.4 ms      │ 19      │ 19
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.103 ms      │ 1.499 ms      │ 1.116 ms      │ 1.127 ms      │ 100     │ 100
│  │  ├─ 3                           1.038 ms      │ 1.106 ms      │ 1.06 ms       │ 1.062 ms      │ 100     │ 100
│  │  ╰─ 5                           1.101 ms      │ 1.156 ms      │ 1.116 ms      │ 1.119 ms      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    20.1 ms       │ 33.7 ms       │ 20.5 ms       │ 20.88 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   10.92 ms      │ 20.5 ms       │ 11.38 ms      │ 11.7 ms       │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  33.28 ms      │ 49.17 ms      │ 33.84 ms      │ 34.48 ms      │ 100     │ 100
│  │  ╰─ "none"                      9.444 ms      │ 12.08 ms      │ 9.77 ms       │ 9.812 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.3 ms        │ 8.517 ms      │ 4.364 ms      │ 4.472 ms      │ 100     │ 100
│  │  ├─ 10000                       9.448 ms      │ 11.77 ms      │ 9.769 ms      │ 9.811 ms      │ 100     │ 100
│  │  ╰─ 50000                       37.73 ms      │ 59.56 ms      │ 39.94 ms      │ 40.7 ms       │ 100     │ 100
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.399 ms      │ 1.639 ms      │ 1.423 ms      │ 1.432 ms      │ 100     │ 100
│  │  ├─ 3                           3.707 ms      │ 4.765 ms      │ 3.855 ms      │ 3.866 ms      │ 100     │ 100
│  │  ╰─ 5                           6.947 ms      │ 8.973 ms      │ 7.144 ms      │ 7.193 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    8.776 ms      │ 15.61 ms      │ 9.036 ms      │ 9.268 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          39.11 ms      │ 54.57 ms      │ 39.92 ms      │ 40.28 ms      │ 100     │ 100
│  │  ╰─ "none"                      3.812 ms      │ 4.732 ms      │ 3.944 ms      │ 3.983 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        1.112 ms      │ 1.244 ms      │ 1.13 ms       │ 1.135 ms      │ 100     │ 100
│     ├─ 10000                       3.719 ms      │ 4.83 ms       │ 3.85 ms       │ 3.873 ms      │ 100     │ 100
│     ╰─ 50000                       23.86 ms      │ 35.51 ms      │ 24.84 ms      │ 25.27 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           504.5 µs      │ 668.5 µs      │ 526.1 µs      │ 538.2 µs      │ 100     │ 100
   │  ├─ 3                           504.9 µs      │ 574.3 µs      │ 521.9 µs      │ 524.7 µs      │ 100     │ 100
   │  ╰─ 5                           513.3 µs      │ 571.5 µs      │ 523.2 µs      │ 526.8 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.644 ms      │ 6.912 ms      │ 6.7 ms        │ 6.707 ms      │ 100     │ 100
   │  ├─ "fanjian"                   2.102 ms      │ 2.544 ms      │ 2.151 ms      │ 2.159 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  12 ms         │ 12.31 ms      │ 12.1 ms       │ 12.11 ms      │ 100     │ 100
   │  ╰─ "none"                      744.8 µs      │ 814.2 µs      │ 762.7 µs      │ 763.4 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        734.4 µs      │ 829.7 µs      │ 764.1 µs      │ 768.6 µs      │ 100     │ 100
   │  ├─ 10000                       734.1 µs      │ 827.6 µs      │ 764.3 µs      │ 768 µs        │ 100     │ 100
   │  ╰─ 50000                       737.7 µs      │ 869.4 µs      │ 769.2 µs      │ 778 µs        │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           512.9 µs      │ 575.2 µs      │ 524.6 µs      │ 528.9 µs      │ 100     │ 100
   │  ├─ 3                           510.2 µs      │ 575.2 µs      │ 520.6 µs      │ 527.6 µs      │ 100     │ 100
   │  ╰─ 5                           511.5 µs      │ 566.1 µs      │ 517.4 µs      │ 522.5 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.519 ms      │ 3.766 ms      │ 3.57 ms       │ 3.575 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.543 ms      │ 4.794 ms      │ 4.602 ms      │ 4.609 ms      │ 100     │ 100
   │  ╰─ "none"                      499.5 µs      │ 746.5 µs      │ 523 µs        │ 532.2 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        511.4 µs      │ 659 µs        │ 546.2 µs      │ 544.3 µs      │ 100     │ 100
      ├─ 10000                       507.9 µs      │ 691.6 µs      │ 526.4 µs      │ 535.5 µs      │ 100     │ 100
      ╰─ 50000                       507.5 µs      │ 679.9 µs      │ 519.9 µs      │ 531.6 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
