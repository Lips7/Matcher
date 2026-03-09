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
Current default simple word map size: 10000
Current default combined times: 1
Timer precision: 41 ns
bench                                fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ build                                           │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           17.41 ms      │ 23.02 ms      │ 17.85 ms      │ 17.98 ms      │ 100     │ 100
│  │  ├─ 2                           38.41 ms      │ 69.96 ms      │ 39.57 ms      │ 40.85 ms      │ 100     │ 100
│  │  ├─ 3                           56.97 ms      │ 84.34 ms      │ 58.83 ms      │ 59.59 ms      │ 85      │ 85
│  │  ╰─ 4                           77.96 ms      │ 121.6 ms      │ 79.89 ms      │ 81.71 ms      │ 62      │ 62
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    17.67 ms      │ 24.67 ms      │ 18.09 ms      │ 18.22 ms      │ 100     │ 100
│  │  ├─ "fanjian"                   17.63 ms      │ 18.75 ms      │ 17.99 ms      │ 18.03 ms      │ 100     │ 100
│  │  ├─ "fanjian_delete_normalize"  15.1 ms       │ 28.12 ms      │ 15.59 ms      │ 15.88 ms      │ 100     │ 100
│  │  ╰─ "none"                      17.62 ms      │ 19.18 ms      │ 18 ms         │ 18.05 ms      │ 100     │ 100
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        26.43 ms      │ 27.79 ms      │ 26.88 ms      │ 26.89 ms      │ 100     │ 100
│  │  ├─ 10000                       43.38 ms      │ 72.39 ms      │ 43.97 ms      │ 44.84 ms      │ 100     │ 100
│  │  ├─ 50000                       130.7 ms      │ 169.9 ms      │ 134.7 ms      │ 137.1 ms      │ 37      │ 37
│  │  ╰─ 100000                      270.4 ms      │ 325.3 ms      │ 280 ms        │ 284.3 ms      │ 18      │ 18
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           9.222 ms      │ 10.55 ms      │ 9.552 ms      │ 9.561 ms      │ 100     │ 100
│  │  ├─ 2                           19.8 ms       │ 24.46 ms      │ 20.3 ms       │ 20.41 ms      │ 100     │ 100
│  │  ├─ 3                           33.92 ms      │ 55.61 ms      │ 34.91 ms      │ 35.29 ms      │ 100     │ 100
│  │  ╰─ 4                           45.54 ms      │ 49.61 ms      │ 47.04 ms      │ 47.15 ms      │ 100     │ 100
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    9.247 ms      │ 9.797 ms      │ 9.435 ms      │ 9.436 ms      │ 100     │ 100
│  │  ├─ "delete_normalize"          10.58 ms      │ 12.62 ms      │ 10.98 ms      │ 10.99 ms      │ 100     │ 100
│  │  ╰─ "none"                      9.243 ms      │ 9.956 ms      │ 9.488 ms      │ 9.485 ms      │ 100     │ 100
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        10.22 ms      │ 18.33 ms      │ 10.36 ms      │ 10.57 ms      │ 100     │ 100
│     ├─ 10000                       18.9 ms       │ 23.16 ms      │ 19.28 ms      │ 19.38 ms      │ 100     │ 100
│     ├─ 50000                       71.55 ms      │ 99.63 ms      │ 72.86 ms      │ 73.58 ms      │ 68      │ 68
│     ╰─ 100000                      190.6 ms      │ 243.9 ms      │ 198.4 ms      │ 202.1 ms      │ 25      │ 25
├─ is_match_match                                  │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.563 ms      │ 3.219 ms      │ 1.624 ms      │ 1.643 ms      │ 100     │ 100
│  │  │                              1.725 GB/s    │ 838.1 MB/s    │ 1.66 GB/s     │ 1.641 GB/s    │         │
│  │  ├─ 2                           6.889 ms      │ 9.648 ms      │ 7.247 ms      │ 7.38 ms       │ 100     │ 100
│  │  │                              391.6 MB/s    │ 279.6 MB/s    │ 372.2 MB/s    │ 365.5 MB/s    │         │
│  │  ├─ 3                           8.971 ms      │ 24.23 ms      │ 9.32 ms       │ 9.693 ms      │ 100     │ 100
│  │  │                              300.7 MB/s    │ 111.3 MB/s    │ 289.4 MB/s    │ 278.3 MB/s    │         │
│  │  ╰─ 4                           10.07 ms      │ 15.41 ms      │ 10.66 ms      │ 10.85 ms      │ 100     │ 100
│  │                                 267.7 MB/s    │ 175 MB/s      │ 252.9 MB/s    │ 248.5 MB/s    │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    8.054 ms      │ 8.722 ms      │ 8.14 ms       │ 8.174 ms      │ 100     │ 100
│  │  │                              334.9 MB/s    │ 309.3 MB/s    │ 331.4 MB/s    │ 330 MB/s      │         │
│  │  ├─ "fanjian"                   3.501 ms      │ 3.743 ms      │ 3.531 ms      │ 3.536 ms      │ 100     │ 100
│  │  │                              770.5 MB/s    │ 720.6 MB/s    │ 763.9 MB/s    │ 762.8 MB/s    │         │
│  │  ├─ "fanjian_delete_normalize"  10.82 ms      │ 14.41 ms      │ 10.98 ms      │ 11.06 ms      │ 100     │ 100
│  │  │                              249.2 MB/s    │ 187.1 MB/s    │ 245.5 MB/s    │ 243.8 MB/s    │         │
│  │  ╰─ "none"                      1.501 ms      │ 1.706 ms      │ 1.608 ms      │ 1.614 ms      │ 100     │ 100
│  │                                 1.797 GB/s    │ 1.58 GB/s     │ 1.676 GB/s    │ 1.67 GB/s     │         │
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        1.995 ms      │ 2.26 ms       │ 2.113 ms      │ 2.11 ms       │ 100     │ 100
│  │  │                              1.352 GB/s    │ 1.193 GB/s    │ 1.276 GB/s    │ 1.278 GB/s    │         │
│  │  ├─ 10000                       1.504 ms      │ 1.671 ms      │ 1.614 ms      │ 1.609 ms      │ 100     │ 100
│  │  │                              1.793 GB/s    │ 1.614 GB/s    │ 1.671 GB/s    │ 1.676 GB/s    │         │
│  │  ├─ 50000                       1.052 ms      │ 1.269 ms      │ 1.104 ms      │ 1.108 ms      │ 100     │ 100
│  │  │                              2.562 GB/s    │ 2.124 GB/s    │ 2.442 GB/s    │ 2.433 GB/s    │         │
│  │  ╰─ 100000                      998.5 µs      │ 1.376 ms      │ 1.031 ms      │ 1.038 ms      │ 100     │ 100
│  │                                 2.701 GB/s    │ 1.96 GB/s     │ 2.615 GB/s    │ 2.598 GB/s    │         │
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           809 µs        │ 1.206 ms      │ 871.6 µs      │ 876.6 µs      │ 100     │ 100
│  │  │                              719.1 MB/s    │ 482.1 MB/s    │ 667.5 MB/s    │ 663.7 MB/s    │         │
│  │  ├─ 2                           1.473 ms      │ 1.883 ms      │ 1.521 ms      │ 1.536 ms      │ 100     │ 100
│  │  │                              394.8 MB/s    │ 308.9 MB/s    │ 382.4 MB/s    │ 378.7 MB/s    │         │
│  │  ├─ 3                           3.526 ms      │ 4.478 ms      │ 3.639 ms      │ 3.655 ms      │ 100     │ 100
│  │  │                              164.9 MB/s    │ 129.9 MB/s    │ 159.8 MB/s    │ 159.1 MB/s    │         │
│  │  ╰─ 4                           4.285 ms      │ 5.414 ms      │ 4.405 ms      │ 4.424 ms      │ 100     │ 100
│  │                                 135.7 MB/s    │ 107.4 MB/s    │ 132 MB/s      │ 131.5 MB/s    │         │
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    3.144 ms      │ 3.493 ms      │ 3.285 ms      │ 3.293 ms      │ 100     │ 100
│  │  │                              185 MB/s      │ 166.5 MB/s    │ 177 MB/s      │ 176.6 MB/s    │         │
│  │  ├─ "delete_normalize"          7.867 ms      │ 8.477 ms      │ 7.991 ms      │ 8.003 ms      │ 100     │ 100
│  │  │                              73.95 MB/s    │ 68.63 MB/s    │ 72.81 MB/s    │ 72.7 MB/s     │         │
│  │  ╰─ "none"                      841.8 µs      │ 924.2 µs      │ 874.7 µs      │ 876.9 µs      │ 100     │ 100
│  │                                 691.1 MB/s    │ 629.5 MB/s    │ 665.1 MB/s    │ 663.5 MB/s    │         │
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        437.9 µs      │ 514.4 µs      │ 448.3 µs      │ 453 µs        │ 100     │ 100
│     │                              1.328 GB/s    │ 1.13 GB/s     │ 1.297 GB/s    │ 1.284 GB/s    │         │
│     ├─ 10000                       847.3 µs      │ 942.4 µs      │ 876.2 µs      │ 878.4 µs      │ 100     │ 100
│     │                              686.6 MB/s    │ 617.3 MB/s    │ 664 MB/s      │ 662.4 MB/s    │         │
│     ├─ 50000                       543.4 µs      │ 625.4 µs      │ 556.3 µs      │ 561.2 µs      │ 100     │ 100
│     │                              1.07 GB/s     │ 930.2 MB/s    │ 1.045 GB/s    │ 1.036 GB/s    │         │
│     ╰─ 100000                      463.3 µs      │ 543.3 µs      │ 480.5 µs      │ 482.1 µs      │ 100     │ 100
│                                    1.255 GB/s    │ 1.07 GB/s     │ 1.21 GB/s     │ 1.206 GB/s    │         │
├─ is_match_no_match                               │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           695.4 µs      │ 780.9 µs      │ 710 µs        │ 712.7 µs      │ 100     │ 100
│  │  │                              3.879 GB/s    │ 3.455 GB/s    │ 3.799 GB/s    │ 3.785 GB/s    │         │
│  │  ├─ 2                           651.9 µs      │ 771 µs        │ 705.8 µs      │ 707.7 µs      │ 100     │ 100
│  │  │                              4.138 GB/s    │ 3.499 GB/s    │ 3.822 GB/s    │ 3.812 GB/s    │         │
│  │  ├─ 3                           651.5 µs      │ 732 µs        │ 687.6 µs      │ 690.7 µs      │ 100     │ 100
│  │  │                              4.141 GB/s    │ 3.685 GB/s    │ 3.923 GB/s    │ 3.905 GB/s    │         │
│  │  ╰─ 4                           661.2 µs      │ 756 µs        │ 695.9 µs      │ 697 µs        │ 100     │ 100
│  │                                 4.08 GB/s     │ 3.568 GB/s    │ 3.876 GB/s    │ 3.87 GB/s     │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    3.391 ms      │ 3.525 ms      │ 3.421 ms      │ 3.425 ms      │ 100     │ 100
│  │  │                              795.5 MB/s    │ 765.3 MB/s    │ 788.4 MB/s    │ 787.5 MB/s    │         │
│  │  ├─ "fanjian"                   2.594 ms      │ 2.708 ms      │ 2.624 ms      │ 2.627 ms      │ 100     │ 100
│  │  │                              1.039 GB/s    │ 996.3 MB/s    │ 1.028 GB/s    │ 1.026 GB/s    │         │
│  │  ├─ "fanjian_delete_normalize"  9.061 ms      │ 9.335 ms      │ 9.128 ms      │ 9.141 ms      │ 100     │ 100
│  │  │                              297.7 MB/s    │ 289 MB/s      │ 295.5 MB/s    │ 295.1 MB/s    │         │
│  │  ╰─ "none"                      693.7 µs      │ 755.2 µs      │ 712 µs        │ 713.1 µs      │ 100     │ 100
│  │                                 3.889 GB/s    │ 3.572 GB/s    │ 3.788 GB/s    │ 3.783 GB/s    │         │
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        669.2 µs      │ 754.1 µs      │ 691.4 µs      │ 693.8 µs      │ 100     │ 100
│  │  │                              4.031 GB/s    │ 3.577 GB/s    │ 3.901 GB/s    │ 3.888 GB/s    │         │
│  │  ├─ 10000                       664.3 µs      │ 748.2 µs      │ 698.7 µs      │ 699.5 µs      │ 100     │ 100
│  │  │                              4.061 GB/s    │ 3.605 GB/s    │ 3.861 GB/s    │ 3.856 GB/s    │         │
│  │  ├─ 50000                       651.4 µs      │ 758.7 µs      │ 695.7 µs      │ 697.3 µs      │ 100     │ 100
│  │  │                              4.141 GB/s    │ 3.556 GB/s    │ 3.878 GB/s    │ 3.869 GB/s    │         │
│  │  ╰─ 100000                      677.6 µs      │ 747.9 µs      │ 685.5 µs      │ 688.7 µs      │ 100     │ 100
│  │                                 3.981 GB/s    │ 3.607 GB/s    │ 3.935 GB/s    │ 3.917 GB/s    │         │
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           431.9 µs      │ 486.1 µs      │ 437.2 µs      │ 444.5 µs      │ 100     │ 100
│  │  │                              1.347 GB/s    │ 1.196 GB/s    │ 1.33 GB/s     │ 1.308 GB/s    │         │
│  │  ├─ 2                           410.2 µs      │ 498.2 µs      │ 432.7 µs      │ 438 µs        │ 100     │ 100
│  │  │                              1.418 GB/s    │ 1.167 GB/s    │ 1.344 GB/s    │ 1.328 GB/s    │         │
│  │  ├─ 3                           417.8 µs      │ 473.9 µs      │ 440.7 µs      │ 440.2 µs      │ 100     │ 100
│  │  │                              1.392 GB/s    │ 1.227 GB/s    │ 1.32 GB/s     │ 1.321 GB/s    │         │
│  │  ╰─ 4                           432.3 µs      │ 471.2 µs      │ 439.4 µs      │ 442.4 µs      │ 100     │ 100
│  │                                 1.345 GB/s    │ 1.234 GB/s    │ 1.323 GB/s    │ 1.315 GB/s    │         │
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    2.217 ms      │ 2.415 ms      │ 2.288 ms      │ 2.292 ms      │ 100     │ 100
│  │  │                              262.4 MB/s    │ 240.8 MB/s    │ 254.3 MB/s    │ 253.8 MB/s    │         │
│  │  ├─ "delete_normalize"          3.135 ms      │ 3.419 ms      │ 3.255 ms      │ 3.258 ms      │ 100     │ 100
│  │  │                              185.5 MB/s    │ 170.1 MB/s    │ 178.7 MB/s    │ 178.5 MB/s    │         │
│  │  ╰─ "none"                      407.4 µs      │ 496.2 µs      │ 429.7 µs      │ 436.3 µs      │ 100     │ 100
│  │                                 1.427 GB/s    │ 1.172 GB/s    │ 1.354 GB/s    │ 1.333 GB/s    │         │
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        422.3 µs      │ 493.9 µs      │ 434.2 µs      │ 439.6 µs      │ 100     │ 100
│     │                              1.377 GB/s    │ 1.178 GB/s    │ 1.339 GB/s    │ 1.323 GB/s    │         │
│     ├─ 10000                       411.4 µs      │ 460.6 µs      │ 426.1 µs      │ 429.8 µs      │ 100     │ 100
│     │                              1.414 GB/s    │ 1.263 GB/s    │ 1.365 GB/s    │ 1.353 GB/s    │         │
│     ├─ 50000                       411.1 µs      │ 478.2 µs      │ 430.8 µs      │ 432.8 µs      │ 100     │ 100
│     │                              1.415 GB/s    │ 1.216 GB/s    │ 1.35 GB/s     │ 1.344 GB/s    │         │
│     ╰─ 100000                      417.1 µs      │ 473 µs        │ 427.6 µs      │ 432.9 µs      │ 100     │ 100
│                                    1.394 GB/s    │ 1.23 GB/s     │ 1.36 GB/s     │ 1.344 GB/s    │         │
├─ search_match                                    │               │               │               │         │
│  ├─ cn_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           5.409 ms      │ 6.047 ms      │ 5.546 ms      │ 5.559 ms      │ 100     │ 100
│  │  │                              498.7 MB/s    │ 446.1 MB/s    │ 486.4 MB/s    │ 485.3 MB/s    │         │
│  │  ├─ 2                           6.926 ms      │ 8.708 ms      │ 7.179 ms      │ 7.246 ms      │ 100     │ 100
│  │  │                              389.5 MB/s    │ 309.8 MB/s    │ 375.7 MB/s    │ 372.3 MB/s    │         │
│  │  ├─ 3                           9.349 ms      │ 42.31 ms      │ 10.03 ms      │ 10.54 ms      │ 100     │ 100
│  │  │                              288.5 MB/s    │ 63.76 MB/s    │ 268.8 MB/s    │ 255.8 MB/s    │         │
│  │  ╰─ 4                           10.54 ms      │ 13.72 ms      │ 11.16 ms      │ 11.28 ms      │ 100     │ 100
│  │                                 255.7 MB/s    │ 196.5 MB/s    │ 241.6 MB/s    │ 239 MB/s      │         │
│  ├─ cn_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    11.37 ms      │ 33.69 ms      │ 11.62 ms      │ 12.7 ms       │ 100     │ 100
│  │  │                              237.1 MB/s    │ 80.07 MB/s    │ 232.1 MB/s    │ 212.4 MB/s    │         │
│  │  ├─ "fanjian"                   7.408 ms      │ 9.252 ms      │ 7.546 ms      │ 7.594 ms      │ 100     │ 100
│  │  │                              364.1 MB/s    │ 291.6 MB/s    │ 357.5 MB/s    │ 355.2 MB/s    │         │
│  │  ├─ "fanjian_delete_normalize"  20.29 ms      │ 21.58 ms      │ 20.47 ms      │ 20.52 ms      │ 100     │ 100
│  │  │                              132.9 MB/s    │ 124.9 MB/s    │ 131.8 MB/s    │ 131.4 MB/s    │         │
│  │  ╰─ "none"                      5.426 ms      │ 10.11 ms      │ 5.59 ms       │ 5.667 ms      │ 100     │ 100
│  │                                 497.2 MB/s    │ 266.8 MB/s    │ 482.5 MB/s    │ 476 MB/s      │         │
│  ├─ cn_by_size                                   │               │               │               │         │
│  │  ├─ 1000                        4.05 ms       │ 4.308 ms      │ 4.229 ms      │ 4.211 ms      │ 100     │ 100
│  │  │                              666.1 MB/s    │ 626.1 MB/s    │ 637.9 MB/s    │ 640.6 MB/s    │         │
│  │  ├─ 10000                       5.475 ms      │ 5.966 ms      │ 5.536 ms      │ 5.557 ms      │ 100     │ 100
│  │  │                              492.7 MB/s    │ 452.1 MB/s    │ 487.3 MB/s    │ 485.4 MB/s    │         │
│  │  ├─ 50000                       11.81 ms      │ 15.01 ms      │ 12.55 ms      │ 12.69 ms      │ 100     │ 100
│  │  │                              228.3 MB/s    │ 179.6 MB/s    │ 214.8 MB/s    │ 212.5 MB/s    │         │
│  │  ╰─ 100000                      20.2 ms       │ 27.03 ms      │ 21.73 ms      │ 22.19 ms      │ 100     │ 100
│  │                                 133.5 MB/s    │ 99.78 MB/s    │ 124.1 MB/s    │ 121.5 MB/s    │         │
│  ├─ en_by_combinations                           │               │               │               │         │
│  │  ├─ 1                           1.336 ms      │ 1.51 ms       │ 1.376 ms      │ 1.381 ms      │ 100     │ 100
│  │  │                              435.4 MB/s    │ 385.2 MB/s    │ 422.8 MB/s    │ 421.1 MB/s    │         │
│  │  ├─ 2                           1.526 ms      │ 1.993 ms      │ 1.574 ms      │ 1.596 ms      │ 100     │ 100
│  │  │                              381.2 MB/s    │ 291.8 MB/s    │ 369.4 MB/s    │ 364.4 MB/s    │         │
│  │  ├─ 3                           3.665 ms      │ 4.584 ms      │ 3.767 ms      │ 3.792 ms      │ 100     │ 100
│  │  │                              158.7 MB/s    │ 126.9 MB/s    │ 154.4 MB/s    │ 153.4 MB/s    │         │
│  │  ╰─ 4                           4.495 ms      │ 5.837 ms      │ 4.647 ms      │ 4.679 ms      │ 100     │ 100
│  │                                 129.4 MB/s    │ 99.68 MB/s    │ 125.1 MB/s    │ 124.3 MB/s    │         │
│  ├─ en_by_process_type                           │               │               │               │         │
│  │  ├─ "delete"                    3.609 ms      │ 4.061 ms      │ 3.722 ms      │ 3.731 ms      │ 100     │ 100
│  │  │                              161.2 MB/s    │ 143.2 MB/s    │ 156.3 MB/s    │ 155.9 MB/s    │         │
│  │  ├─ "delete_normalize"          19 ms         │ 24.9 ms       │ 19.35 ms      │ 19.46 ms      │ 100     │ 100
│  │  │                              30.61 MB/s    │ 23.35 MB/s    │ 30.06 MB/s    │ 29.89 MB/s    │         │
│  │  ╰─ "none"                      1.335 ms      │ 1.498 ms      │ 1.375 ms      │ 1.378 ms      │ 100     │ 100
│  │                                 435.7 MB/s    │ 388.3 MB/s    │ 423.1 MB/s    │ 422.1 MB/s    │         │
│  ╰─ en_by_size                                   │               │               │               │         │
│     ├─ 1000                        484.2 µs      │ 606.1 µs      │ 507.1 µs      │ 511.5 µs      │ 100     │ 100
│     │                              1.201 GB/s    │ 959.9 MB/s    │ 1.147 GB/s    │ 1.137 GB/s    │         │
│     ├─ 10000                       1.329 ms      │ 1.505 ms      │ 1.347 ms      │ 1.359 ms      │ 100     │ 100
│     │                              437.5 MB/s    │ 386.5 MB/s    │ 431.6 MB/s    │ 428 MB/s      │         │
│     ├─ 50000                       6.116 ms      │ 7.819 ms      │ 6.324 ms      │ 6.344 ms      │ 100     │ 100
│     │                              95.13 MB/s    │ 74.4 MB/s     │ 91.99 MB/s    │ 91.71 MB/s    │         │
│     ╰─ 100000                      14.76 ms      │ 31.32 ms      │ 15.71 ms      │ 16.33 ms      │ 100     │ 100
│                                    39.39 MB/s    │ 18.57 MB/s    │ 37.01 MB/s    │ 35.62 MB/s    │         │
╰─ search_no_match                                 │               │               │               │         │
   ├─ cn_by_combinations                           │               │               │               │         │
   │  ├─ 1                           744.8 µs      │ 831 µs        │ 769.3 µs      │ 773.7 µs      │ 100     │ 100
   │  │                              3.622 GB/s    │ 3.246 GB/s    │ 3.507 GB/s    │ 3.487 GB/s    │         │
   │  ├─ 2                           719.1 µs      │ 814.8 µs      │ 771.2 µs      │ 775 µs        │ 100     │ 100
   │  │                              3.751 GB/s    │ 3.311 GB/s    │ 3.498 GB/s    │ 3.481 GB/s    │         │
   │  ├─ 3                           722.3 µs      │ 821.1 µs      │ 771.9 µs      │ 773.1 µs      │ 100     │ 100
   │  │                              3.735 GB/s    │ 3.285 GB/s    │ 3.495 GB/s    │ 3.489 GB/s    │         │
   │  ╰─ 4                           736.2 µs      │ 843.6 µs      │ 766.7 µs      │ 769.7 µs      │ 100     │ 100
   │                                 3.664 GB/s    │ 3.198 GB/s    │ 3.518 GB/s    │ 3.505 GB/s    │         │
   ├─ cn_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    3.399 ms      │ 3.519 ms      │ 3.434 ms      │ 3.437 ms      │ 100     │ 100
   │  │                              793.5 MB/s    │ 766.6 MB/s    │ 785.4 MB/s    │ 784.7 MB/s    │         │
   │  ├─ "fanjian"                   2.629 ms      │ 2.827 ms      │ 2.644 ms      │ 2.655 ms      │ 100     │ 100
   │  │                              1.026 GB/s    │ 954.2 MB/s    │ 1.02 GB/s     │ 1.016 GB/s    │         │
   │  ├─ "fanjian_delete_normalize"  9.103 ms      │ 9.514 ms      │ 9.191 ms      │ 9.196 ms      │ 100     │ 100
   │  │                              296.3 MB/s    │ 283.5 MB/s    │ 293.5 MB/s    │ 293.3 MB/s    │         │
   │  ╰─ "none"                      751.9 µs      │ 831.2 µs      │ 770.7 µs      │ 774.1 µs      │ 100     │ 100
   │                                 3.588 GB/s    │ 3.245 GB/s    │ 3.5 GB/s      │ 3.485 GB/s    │         │
   ├─ cn_by_size                                   │               │               │               │         │
   │  ├─ 1000                        716.9 µs      │ 848.2 µs      │ 765.6 µs      │ 767.7 µs      │ 100     │ 100
   │  │                              3.763 GB/s    │ 3.18 GB/s     │ 3.524 GB/s    │ 3.514 GB/s    │         │
   │  ├─ 10000                       716.7 µs      │ 840.7 µs      │ 768.6 µs      │ 761.4 µs      │ 100     │ 100
   │  │                              3.764 GB/s    │ 3.209 GB/s    │ 3.51 GB/s     │ 3.543 GB/s    │         │
   │  ├─ 50000                       757.5 µs      │ 826.5 µs      │ 779.6 µs      │ 781.4 µs      │ 100     │ 100
   │  │                              3.561 GB/s    │ 3.264 GB/s    │ 3.46 GB/s     │ 3.452 GB/s    │         │
   │  ╰─ 100000                      755.7 µs      │ 840.9 µs      │ 783.2 µs      │ 784.1 µs      │ 100     │ 100
   │                                 3.57 GB/s     │ 3.208 GB/s    │ 3.444 GB/s    │ 3.44 GB/s     │         │
   ├─ en_by_combinations                           │               │               │               │         │
   │  ├─ 1                           472 µs        │ 552.2 µs      │ 510.9 µs      │ 510.2 µs      │ 100     │ 100
   │  │                              1.232 GB/s    │ 1.053 GB/s    │ 1.138 GB/s    │ 1.14 GB/s     │         │
   │  ├─ 2                           443.3 µs      │ 559.1 µs      │ 517.2 µs      │ 513.3 µs      │ 100     │ 100
   │  │                              1.312 GB/s    │ 1.04 GB/s     │ 1.124 GB/s    │ 1.133 GB/s    │         │
   │  ├─ 3                           455.2 µs      │ 544.8 µs      │ 515.9 µs      │ 508.3 µs      │ 100     │ 100
   │  │                              1.278 GB/s    │ 1.067 GB/s    │ 1.127 GB/s    │ 1.144 GB/s    │         │
   │  ╰─ 4                           456.3 µs      │ 548.9 µs      │ 513.5 µs      │ 509.3 µs      │ 100     │ 100
   │                                 1.274 GB/s    │ 1.059 GB/s    │ 1.133 GB/s    │ 1.142 GB/s    │         │
   ├─ en_by_process_type                           │               │               │               │         │
   │  ├─ "delete"                    2.267 ms      │ 2.466 ms      │ 2.32 ms       │ 2.324 ms      │ 100     │ 100
   │  │                              256.6 MB/s    │ 235.8 MB/s    │ 250.7 MB/s    │ 250.3 MB/s    │         │
   │  ├─ "delete_normalize"          3.199 ms      │ 3.543 ms      │ 3.303 ms      │ 3.311 ms      │ 100     │ 100
   │  │                              181.8 MB/s    │ 164.1 MB/s    │ 176.1 MB/s    │ 175.6 MB/s    │         │
   │  ╰─ "none"                      459.9 µs      │ 539 µs        │ 492.9 µs      │ 495 µs        │ 100     │ 100
   │                                 1.265 GB/s    │ 1.079 GB/s    │ 1.18 GB/s     │ 1.175 GB/s    │         │
   ╰─ en_by_size                                   │               │               │               │         │
      ├─ 1000                        454.8 µs      │ 538.3 µs      │ 494.4 µs      │ 498.5 µs      │ 100     │ 100
      │                              1.279 GB/s    │ 1.08 GB/s     │ 1.176 GB/s    │ 1.167 GB/s    │         │
      ├─ 10000                       450.4 µs      │ 569.9 µs      │ 504.1 µs      │ 510.9 µs      │ 100     │ 100
      │                              1.291 GB/s    │ 1.02 GB/s     │ 1.154 GB/s    │ 1.138 GB/s    │         │
      ├─ 50000                       461.9 µs      │ 553.6 µs      │ 518.1 µs      │ 510.8 µs      │ 100     │ 100
      │                              1.259 GB/s    │ 1.05 GB/s     │ 1.123 GB/s    │ 1.139 GB/s    │         │
      ╰─ 100000                      447.3 µs      │ 604.7 µs      │ 489.4 µs      │ 497.6 µs      │ 100     │ 100
                                     1.3 GB/s      │ 962.1 MB/s    │ 1.188 GB/s    │ 1.169 GB/s    │         │
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
