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
* `runtime_build`: Enable building the process matcher at runtime (increases build time).
* `dfa`: Use a Deterministic Finite Automaton (DFA) for matching. Offers better search speed but significantly higher memory consumption.
* `vectorscan`: Use Intel's Vectorscan (a fork of Hyperscan) for SIMD-accelerated matching. Offers the best performance but requires the Vectorscan library to be installed on the system.

### Feature Comparison & Recommendation

| Feature | Engine | Search Speed | Memory Usage | External Dependency | Best For |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Default** | Aho-Corasick (NFA) | Good | **Lowest** | None | General purpose, memory-constrained environments. |
| `dfa` | Aho-Corasick (DFA) | **Fast** | Highest | None | Speed-critical apps where external dependencies are a no-go. |
| `vectorscan` | Vectorscan (SIMD) | **Fastest** | Moderate | **Required** | High-throughput production systems requiring max performance. |

## Benchmarks

Benchmarked on **MacBook Air M4 (24GB RAM)**.
Test data: [CN_WORD_LIST_100000](../data/word_list/cn/cn_words_100000.txt) against [CN_HAYSTACK](../data/text/cn/西游记.txt) and [EN_WORD_LIST_100000](../data/word_list/en/en_words_100000.txt) against [EN_HAYSTACK](../data/text/en/sherlock.txt).

### DFA

```
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

### DFA + Vectorscan

```
Current default simple match type: ProcessType(None)
Current default simple word map size: 10000
Current default combined times: 1
Timer precision: 41 ns
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           52.93 ms      │ 61.62 ms      │ 54.26 ms      │ 54.38 ms      │ 92      │ 92
│  │  ├─ 2                           115.5 ms      │ 128.8 ms      │ 119 ms        │ 119 ms        │ 43      │ 43
│  │  ├─ 3                           194.3 ms      │ 214.9 ms      │ 198.6 ms      │ 198.9 ms      │ 26      │ 26
│  │  ╰─ 4                           280 ms        │ 293 ms        │ 286.1 ms      │ 285.8 ms      │ 18      │ 18
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    52.83 ms      │ 55.98 ms      │ 53.96 ms      │ 54.03 ms      │ 93      │ 93
│  │  ├─ "fanjian"                   52.97 ms      │ 63.59 ms      │ 54.07 ms      │ 54.27 ms      │ 93      │ 93
│  │  ├─ "fanjian_delete_normalize"  53.92 ms      │ 65.74 ms      │ 54.78 ms      │ 54.87 ms      │ 92      │ 92
│  │  ╰─ "none"                      52.53 ms      │ 54.78 ms      │ 53.54 ms      │ 53.62 ms      │ 94      │ 94
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        6.504 ms      │ 7.057 ms      │ 6.695 ms      │ 6.688 ms      │ 100     │ 100
│  │  ├─ 10000                       52.39 ms      │ 63.5 ms       │ 53.75 ms      │ 53.88 ms      │ 93      │ 93
│  │  ├─ 50000                       368.7 ms      │ 382.2 ms      │ 376.1 ms      │ 375.7 ms      │ 14      │ 14
│  │  ╰─ 100000                      797.4 ms      │ 827.8 ms      │ 806.5 ms      │ 807.3 ms      │ 7       │ 7
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           51.8 ms       │ 71.68 ms      │ 53.09 ms      │ 53.44 ms      │ 94      │ 94
│  │  ├─ 2                           108.1 ms      │ 118.8 ms      │ 110.6 ms      │ 110.9 ms      │ 46      │ 46
│  │  ├─ 3                           178.4 ms      │ 187.5 ms      │ 181.5 ms      │ 181.7 ms      │ 28      │ 28
│  │  ╰─ 4                           250.8 ms      │ 265.7 ms      │ 256.2 ms      │ 255.7 ms      │ 20      │ 20
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    51.93 ms      │ 55.44 ms      │ 53.15 ms      │ 53.15 ms      │ 95      │ 95
│  │  ├─ "delete_normalize"          53.45 ms      │ 56.32 ms      │ 54.59 ms      │ 54.49 ms      │ 92      │ 92
│  │  ╰─ "none"                      51.78 ms      │ 62.97 ms      │ 52.89 ms      │ 53.08 ms      │ 95      │ 95
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        6.364 ms      │ 6.958 ms      │ 6.439 ms      │ 6.496 ms      │ 100     │ 100
│     ├─ 10000                       51.57 ms      │ 54.27 ms      │ 52.75 ms      │ 52.66 ms      │ 95      │ 95
│     ├─ 50000                       332.3 ms      │ 349.4 ms      │ 338.5 ms      │ 338 ms        │ 15      │ 15
│     ╰─ 100000                      787.8 ms      │ 818 ms        │ 795 ms        │ 798.1 ms      │ 7       │ 7
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           839.1 µs      │ 1.728 ms      │ 853.6 µs      │ 865.5 µs      │ 100     │ 100
│  │  ├─ 2                           800.1 µs      │ 1.8 ms        │ 806.9 µs      │ 818.9 µs      │ 100     │ 100
│  │  ├─ 3                           610.1 µs      │ 1.903 ms      │ 620.1 µs      │ 634.8 µs      │ 100     │ 100
│  │  ╰─ 4                           786.4 µs      │ 842.1 µs      │ 795.9 µs      │ 798.3 µs      │ 100     │ 100
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    16.8 ms       │ 28.34 ms      │ 17.02 ms      │ 17.13 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   7.427 ms      │ 8.447 ms      │ 7.525 ms      │ 7.551 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  11.03 ms      │ 11.28 ms      │ 11.09 ms      │ 11.09 ms      │ 100     │ 100
│  │  ╰─ "none"                      6.13 ms       │ 6.719 ms      │ 6.196 ms      │ 6.238 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        2.163 ms      │ 2.39 ms       │ 2.178 ms      │ 2.188 ms      │ 100     │ 100
│  │  ├─ 10000                       6.145 ms      │ 7.242 ms      │ 6.277 ms      │ 6.264 ms      │ 100     │ 100
│  │  ├─ 50000                       14.38 ms      │ 19.03 ms      │ 14.87 ms      │ 14.85 ms      │ 100     │ 100
│  │  ╰─ 100000                      21.74 ms      │ 32.28 ms      │ 23.04 ms      │ 23.18 ms      │ 100     │ 100
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.125 ms      │ 1.736 ms      │ 1.135 ms      │ 1.149 ms      │ 100     │ 100
│  │  ├─ 2                           1.566 ms      │ 2.786 ms      │ 1.58 ms       │ 1.598 ms      │ 100     │ 100
│  │  ├─ 3                           4.541 ms      │ 8.529 ms      │ 4.603 ms      │ 4.738 ms      │ 100     │ 100
│  │  ╰─ 4                           5.662 ms      │ 6.968 ms      │ 5.859 ms      │ 5.854 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    4.68 ms       │ 5.313 ms      │ 4.717 ms      │ 4.737 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          32.51 ms      │ 33.7 ms       │ 32.93 ms      │ 32.87 ms      │ 100     │ 100
│  │  ╰─ "none"                      1.123 ms      │ 1.719 ms      │ 1.143 ms      │ 1.154 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        657.8 µs      │ 770.1 µs      │ 668.1 µs      │ 671.1 µs      │ 100     │ 100
│     ├─ 10000                       1.111 ms      │ 1.698 ms      │ 1.132 ms      │ 1.146 ms      │ 100     │ 100
│     ├─ 50000                       8.098 ms      │ 11.49 ms      │ 8.328 ms      │ 8.325 ms      │ 100     │ 100
│     ╰─ 100000                      21.58 ms      │ 28.81 ms      │ 22.61 ms      │ 22.52 ms      │ 100     │ 100
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           597.4 µs      │ 1.242 ms      │ 608.1 µs      │ 618.5 µs      │ 100     │ 100
   │  ├─ 2                           610 µs        │ 1.465 ms      │ 615.1 µs      │ 624.9 µs      │ 100     │ 100
   │  ├─ 3                           606 µs        │ 1.673 ms      │ 619.5 µs      │ 632.3 µs      │ 100     │ 100
   │  ╰─ 4                           619.7 µs      │ 1.941 ms      │ 632.2 µs      │ 645.9 µs      │ 100     │ 100
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    6.792 ms      │ 7.58 ms       │ 6.826 ms      │ 6.84 ms       │ 100     │ 100
   │  ├─ "fanjian"                   2.176 ms      │ 2.853 ms      │ 2.189 ms      │ 2.202 ms      │ 100     │ 100
   │  ├─ "fanjian_delete_normalize"  12.18 ms      │ 15.28 ms      │ 12.25 ms      │ 12.3 ms       │ 100     │ 100
   │  ╰─ "none"                      896.9 µs      │ 1.584 ms      │ 902.8 µs      │ 911.6 µs      │ 100     │ 100
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        886.7 µs      │ 1.031 ms      │ 891.4 µs      │ 897.1 µs      │ 100     │ 100
   │  ├─ 10000                       898.9 µs      │ 1.608 ms      │ 903.4 µs      │ 913.4 µs      │ 100     │ 100
   │  ├─ 50000                       898.4 µs      │ 3.559 ms      │ 902.8 µs      │ 931.8 µs      │ 100     │ 100
   │  ╰─ 100000                      904 µs        │ 6.196 ms      │ 909.7 µs      │ 965.4 µs      │ 100     │ 100
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           581.4 µs      │ 1.246 ms      │ 585.9 µs      │ 594.8 µs      │ 100     │ 100
   │  ├─ 2                           582.6 µs      │ 1.454 ms      │ 585.9 µs      │ 595.8 µs      │ 100     │ 100
   │  ├─ 3                           605.7 µs      │ 1.684 ms      │ 609.7 µs      │ 622.8 µs      │ 100     │ 100
   │  ╰─ 4                           627.4 µs      │ 1.925 ms      │ 631.8 µs      │ 647.4 µs      │ 100     │ 100
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.46 ms       │ 4.293 ms      │ 3.475 ms      │ 3.49 ms       │ 100     │ 100
   │  ├─ "delete_normalize"          4.569 ms      │ 4.817 ms      │ 4.623 ms      │ 4.633 ms      │ 100     │ 100
   │  ╰─ "none"                      596.9 µs      │ 1.262 ms      │ 602.5 µs      │ 612.3 µs      │ 100     │ 100
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        592.8 µs      │ 700.8 µs      │ 603.6 µs      │ 606.4 µs      │ 100     │ 100
      ├─ 10000                       601.6 µs      │ 1.264 ms      │ 609.4 µs      │ 616.9 µs      │ 100     │ 100
      ├─ 50000                       581.4 µs      │ 3.223 ms      │ 594.9 µs      │ 625.2 µs      │ 100     │ 100
      ╰─ 100000                      596.8 µs      │ 5.792 ms      │ 601.1 µs      │ 655.3 µs      │ 100     │ 100
```


## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
