# Matcher

A high-performance Rust matcher for rules that need both logical operators and text variation handling.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

- Logical rule syntax with `&` and `~`
- Configurable text-transformation pipelines through `ProcessType`
- Shared-prefix transform traversal so related pipelines reuse intermediate results
- Separate bytewise and charwise matcher engines chosen from the final rule set

## Usage

### Adding to Your Project

To use `matcher_rs` in your Rust project, run the following command:

```shell
cargo add matcher_rs
```

### Explanation of the configuration

#### ProcessType

* `None`: Match against the original input text.
* `Fanjian`: Traditional Chinese to simplified Chinese conversion. Based on [FANJIAN](./process_map/FANJIAN.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Remove the codepoints listed in [TEXT_DELETE](./process_map/TEXT-DELETE.txt) plus the built-in whitespace set.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Apply the replacement tables from [NORM](./process_map/NORM.txt) and [NUM_NORM](./process_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠Õ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert mapped codepoints to pinyin with boundary spaces. Based on [PINYIN](./process_map/PINYIN.txt).
  * `你好` -> ` ni  hao `
  * `西安` -> ` xi  an `
* `PinYinChar`: Convert the same mapped codepoints to pinyin with trimmed boundaries.
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Including `None` in a composite `ProcessType` keeps the raw-text path alongside transformed
variants. For example, `ProcessType::None | ProcessType::PinYin` allows one part of a rule to
match the original text while another part matches the Pinyin-transformed text.

Be careful combining `PinYin` and `PinYinChar`: they preserve different word boundaries, so the
same input can behave like `xi` + `an` in one pipeline and `xian` in the other.

#### Rule syntax

* `a&b`: both sub-patterns must appear, in any order
* `a~b`: `a` must appear and `b` must stay absent
* repeated segments count: `无&法&无&天` requires two matches of `无`

### Basic Example

Here’s a basic example of how to use the `SimpleMatcher` for text matching:

```rust
use matcher_rs::{text_process, reduce_text_process, ProcessType};

let result = text_process(ProcessType::Delete, "你好，世界！");
let results = reduce_text_process(ProcessType::FanjianDeleteNormalize, "你好，世界！");
```

`text_process` returns only the final transformed text. `reduce_text_process` returns each
changed intermediate result along one pipeline. For shared-prefix multi-variant traversal,
`SimpleMatcher` uses the internal transform-tree helpers instead of recomputing each path independently.


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
* `runtime_build`: Build transformation tables from the source text maps at runtime instead of loading build-time artifacts.
* `dfa`: Use `aho-corasick` DFA mode in the parts of the matcher that opt into it. This is enabled by default.
* `simd_runtime_dispatch`: Enabled by default. Selects the best available transform kernel at runtime (`AVX2` on x86-64, `NEON` on ARM64, portable fallback elsewhere).

### Feature Comparison & Recommendation

| Feature | Engine | Search Speed | Memory Usage | External Dependency | Best For |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Default** | Mixed bytewise/charwise engines with `dfa` enabled where applicable | **Fast** | Higher | None | General purpose use. |
| `simd_runtime_dispatch` | Runtime-selected transform kernels | **Fastest preprocess** | Neutral | None | Portable builds that should exploit the host CPU automatically. |
| `--no-default-features` | `daachorse`-first matching plus portable transform kernels | Good | Lower | None | Leaner builds and feature debugging. |
| `dfa` | Adds DFA-backed `aho-corasick` where this crate selects it | **Fast** | Higher | None | Custom feature sets that still want the default automaton choices. |

## Benchmarks

Benchmarked on **MacBook Air M4 (24GB RAM)**.
Test data: [CN_WORD_LIST_100000](../data/word_list/cn/cn_words_100000.txt) against [CN_HAYSTACK](../data/text/cn/西游记.txt) and [EN_WORD_LIST_100000](../data/word_list/en/en_words_100000.txt) against [EN_HAYSTACK](../data/text/en/sherlock.txt).

Full records are stored in [bench_records/](./bench_records/). Latest: [latest.txt](./bench_records/latest.txt).

For local benchmarking, use the helper script or the matching `Makefile` target instead of ad hoc `cargo bench` runs:

```shell
python3 matcher_rs/scripts/run_benchmarks.py --preset search
make bench-build
make bench-engine-search
```

The local protocol is:

* run benchmarks serially only
* benchmark only the preset affected by your change
* let the script warm the binary and collect repeated runs
* compare aggregated run sets, not a single median from one output file
* prefer plugged-in power, a warm build cache, and low background load
* treat rows marked noisy as informational rather than regression signals

Each run creates a timestamped directory under `matcher_rs/bench_records/` with raw outputs, `aggregate.json`, and `summary.txt`.

To compare two aggregated run sets:

```shell
python3 matcher_rs/scripts/compare_benchmark_runs.py \
  "matcher_rs/bench_records/2026-03-29_17-00-00_search" \
  "matcher_rs/bench_records/2026-03-29_17-20-00_search"
```

If you need a direct comparison between two single raw benchmark outputs, keep using:

```shell
python3 matcher_rs/scripts/compare_benchmarks.py \
  "matcher_rs/bench_records/2026-03-10 12:22:24.txt" \
  "matcher_rs/bench_records/2026-03-11 23:16:38.txt"
```

The single-file script treats the first file as the baseline and prints `Regression` and `Improvement`. The run-set script suppresses noisy rows by default and compares aggregate medians across repeats.

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
