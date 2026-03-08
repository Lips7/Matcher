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
Run on MacBook Air M4 24GB
Current default simple match type: ProcessType(None)
Current default simple word map size: 10000
Current default combined times: 1
Timer precision: 41 ns
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           17.14 ms      │ 33.09 ms      │ 17.7 ms       │ 18.22 ms      │ 100     │ 100
│  │  ├─ 2                           38 ms         │ 44.64 ms      │ 39.5 ms       │ 39.95 ms      │ 100     │ 100
│  │  ├─ 3                           56.93 ms      │ 65.17 ms      │ 58.64 ms      │ 58.96 ms      │ 85      │ 85
│  │  ╰─ 4                           77.92 ms      │ 92.81 ms      │ 79.28 ms      │ 79.59 ms      │ 63      │ 63
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    16.93 ms      │ 18.92 ms      │ 17.57 ms      │ 17.61 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   17.35 ms      │ 18.61 ms      │ 17.8 ms       │ 17.81 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  14.91 ms      │ 23.01 ms      │ 15.4 ms       │ 15.48 ms      │ 100     │ 100
│  │  ╰─ "none"                      17.15 ms      │ 18.05 ms      │ 17.73 ms      │ 17.72 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        1.39 ms       │ 1.526 ms      │ 1.459 ms      │ 1.457 ms      │ 100     │ 100
│  │  ├─ 10000                       17.19 ms      │ 20 ms         │ 17.71 ms      │ 17.76 ms      │ 100     │ 100
│  │  ├─ 50000                       102.6 ms      │ 112.9 ms      │ 104.8 ms      │ 105.4 ms      │ 48      │ 48
│  │  ╰─ 100000                      236.4 ms      │ 282 ms        │ 242.7 ms      │ 246.2 ms      │ 21      │ 21
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.065 ms      │ 9.619 ms      │ 9.257 ms      │ 9.28 ms       │ 100     │ 100
│  │  ├─ 2                           19.42 ms      │ 39.11 ms      │ 19.95 ms      │ 20.26 ms      │ 100     │ 100
│  │  ├─ 3                           33.48 ms      │ 36.07 ms      │ 34.56 ms      │ 34.58 ms      │ 100     │ 100
│  │  ╰─ 4                           45.74 ms      │ 48.97 ms      │ 46.85 ms      │ 46.9 ms       │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    8.9 ms        │ 12.38 ms      │ 9.206 ms      │ 9.264 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          10.41 ms      │ 11.93 ms      │ 10.81 ms      │ 10.81 ms      │ 100     │ 100
│  │  ╰─ "none"                      9.052 ms      │ 10.31 ms      │ 9.314 ms      │ 9.347 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        747.8 µs      │ 1.113 ms      │ 818.6 µs      │ 821.8 µs      │ 100     │ 100
│     ├─ 10000                       9.09 ms       │ 9.651 ms      │ 9.348 ms      │ 9.335 ms      │ 100     │ 100
│     ├─ 50000                       59.04 ms      │ 66.08 ms      │ 61.08 ms      │ 61.29 ms      │ 82      │ 82
│     ╰─ 100000                      172.4 ms      │ 216.5 ms      │ 176.3 ms      │ 178.1 ms      │ 29      │ 29
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.025 ms      │ 1.479 ms      │ 1.089 ms      │ 1.09 ms       │ 100     │ 100
│  │  ├─ 2                           997.2 µs      │ 1.113 ms      │ 1.081 ms      │ 1.078 ms      │ 100     │ 100
│  │  ├─ 3                           1.032 ms      │ 1.076 ms      │ 1.04 ms       │ 1.041 ms      │ 100     │ 100
│  │  ╰─ 4                           989.1 µs      │ 1.135 ms      │ 1.075 ms      │ 1.074 ms      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    14.48 ms      │ 24.98 ms      │ 14.8 ms       │ 14.97 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   6.834 ms      │ 7.764 ms      │ 6.923 ms      │ 6.999 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  23.09 ms      │ 24.84 ms      │ 23.5 ms       │ 23.52 ms      │ 100     │ 100
│  │  ╰─ "none"                      5.316 ms      │ 6.008 ms      │ 5.505 ms      │ 5.504 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        3.954 ms      │ 4.292 ms      │ 4.231 ms      │ 4.211 ms      │ 100     │ 100
│  │  ├─ 10000                       5.323 ms      │ 5.891 ms      │ 5.612 ms      │ 5.581 ms      │ 100     │ 100
│  │  ├─ 50000                       11.68 ms      │ 14.76 ms      │ 12.27 ms      │ 12.26 ms      │ 100     │ 100
│  │  ╰─ 100000                      19.51 ms      │ 26.68 ms      │ 21.38 ms      │ 21.5 ms       │ 100     │ 100
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.3 ms        │ 1.538 ms      │ 1.376 ms      │ 1.376 ms      │ 100     │ 100
│  │  ├─ 2                           1.547 ms      │ 1.948 ms      │ 1.616 ms      │ 1.623 ms      │ 100     │ 100
│  │  ├─ 3                           3.642 ms      │ 4.564 ms      │ 3.776 ms      │ 3.778 ms      │ 100     │ 100
│  │  ╰─ 4                           4.503 ms      │ 5.702 ms      │ 4.553 ms      │ 4.592 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    4.975 ms      │ 5.313 ms      │ 5.033 ms      │ 5.046 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          20.84 ms      │ 23.95 ms      │ 21.27 ms      │ 21.32 ms      │ 100     │ 100
│  │  ╰─ "none"                      1.387 ms      │ 1.559 ms      │ 1.397 ms      │ 1.404 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        538.5 µs      │ 618.2 µs      │ 549 µs        │ 552.6 µs      │ 100     │ 100
│     ├─ 10000                       1.341 ms      │ 1.566 ms      │ 1.397 ms      │ 1.401 ms      │ 100     │ 100
│     ├─ 50000                       6.323 ms      │ 8.025 ms      │ 6.496 ms      │ 6.497 ms      │ 100     │ 100
│     ╰─ 100000                      14.97 ms      │ 23.4 ms       │ 15.73 ms      │ 16.11 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           460.4 µs      │ 682.3 µs      │ 518.7 µs      │ 521.1 µs      │ 100     │ 100
   │  ├─ 2                           466.2 µs      │ 546.7 µs      │ 515.9 µs      │ 505.8 µs      │ 100     │ 100
   │  ├─ 3                           477 µs        │ 587.5 µs      │ 483.8 µs      │ 486.8 µs      │ 100     │ 100
   │  ╰─ 4                           488.4 µs      │ 682.3 µs      │ 512.6 µs      │ 521.4 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.505 ms      │ 6.891 ms      │ 6.606 ms      │ 6.625 ms      │ 100     │ 100
   │  ├─ "fanjian"                   2.08 ms       │ 2.171 ms      │ 2.1 ms        │ 2.103 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  11.82 ms      │ 12.49 ms      │ 11.99 ms      │ 12.02 ms      │ 100     │ 100
   │  ╰─ "none"                      682.5 µs      │ 777.7 µs      │ 705.3 µs      │ 709.7 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        691.5 µs      │ 813 µs        │ 701.7 µs      │ 709.2 µs      │ 100     │ 100
   │  ├─ 10000                       676.5 µs      │ 790.1 µs      │ 709.8 µs      │ 712.2 µs      │ 100     │ 100
   │  ├─ 50000                       682.2 µs      │ 799.4 µs      │ 708.8 µs      │ 713.2 µs      │ 100     │ 100
   │  ╰─ 100000                      663.8 µs      │ 876.3 µs      │ 724.8 µs      │ 727.5 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           454.5 µs      │ 508.4 µs      │ 468.5 µs      │ 469.8 µs      │ 100     │ 100
   │  ├─ 2                           438.3 µs      │ 590.1 µs      │ 477.3 µs      │ 486.4 µs      │ 100     │ 100
   │  ├─ 3                           459.8 µs      │ 592.2 µs      │ 479.8 µs      │ 484.3 µs      │ 100     │ 100
   │  ╰─ 4                           494.2 µs      │ 564.3 µs      │ 501.9 µs      │ 511.1 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.494 ms      │ 4.278 ms      │ 3.614 ms      │ 3.629 ms      │ 100     │ 100
   │  ├─ "delete_normalize"          4.569 ms      │ 5.506 ms      │ 4.654 ms      │ 4.666 ms      │ 100     │ 100
   │  ╰─ "none"                      451.1 µs      │ 559.2 µs      │ 497.7 µs      │ 492.6 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        447.8 µs      │ 656.3 µs      │ 480.2 µs      │ 490.5 µs      │ 100     │ 100
      ├─ 10000                       465.1 µs      │ 860.5 µs      │ 483.2 µs      │ 493.4 µs      │ 100     │ 100
      ├─ 50000                       462.4 µs      │ 511.4 µs      │ 467.3 µs      │ 471.2 µs      │ 100     │ 100
      ╰─ 100000                      458 µs        │ 535.9 µs      │ 477 µs        │ 479.5 µs      │ 100     │ 100
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
