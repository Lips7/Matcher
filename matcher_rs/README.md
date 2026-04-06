# Matcher

A high-performance Rust library for multi-pattern matching with logical operators (`&`/`~`) and configurable text normalization pipelines. Designed for content moderation, keyword filtering, and any scenario where you need to match thousands of rules against text with precision control over recall.

For internal architecture details, see the [Design Document](../DESIGN.md).

## Quick Start

```shell
cargo add matcher_rs
```

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "apple&pie")
    .add_word(ProcessType::VariantNorm, 2, "你好")
    .build()
    .unwrap();

assert!(matcher.is_match("I like apple and pie"));
let results = matcher.process("你好世界");
assert_eq!(results[0].word_id, 2);
```

## How It Works

```
Rules → parse & dedup → build transform trie → compile AC automata
                                                        │
Query text → walk trie (transform + scan each variant) → evaluate rules → results
```

All sub-patterns across all rules are deduplicated into a single Aho-Corasick automaton for O(N) text scanning. Text transformations share a prefix trie so `VariantNorm|Delete` reuses the VariantNorm result rather than recomputing it. Per-rule state uses generation-stamped sparse sets for O(1) amortized reset between queries.

## ProcessType

Controls which text transformations are applied before matching:

| Flag | Example |
|------|---------|
| `None` | Match against the original input text |
| `VariantNorm` | CJK variant normalization: `測試` → `测试`, `ｶﾀｶﾅ` → `カタカナ` |
| `Delete` | Remove punctuation/symbols/whitespace: `hello, world!` → `helloworld` |
| `Normalize` | NFKC casefold + numeric: `ＡＢⅣ①` → `ab41` |
| `Romanize` | CJK → space-separated romanization: `你好` → ` ni hao`, `한글` → ` han geul` |
| `RomanizeChar` | CJK → romanization (no spaces): `你好` → `nihao` |

Compose with `|`: `ProcessType::VariantNorm | ProcessType::Delete`. Pre-defined aliases: `DeleteNormalize`, `VariantNormDeleteNormalize`.

Including `None` in a composite type keeps the raw-text path alongside transformed variants — one sub-pattern can match raw text while another matches the transformed variant.

## Rule Syntax

| Operator | Meaning | Example |
|----------|---------|---------|
| `&` | All sub-patterns must appear (any order) | `"apple&pie"` fires when both appear |
| `~` | Following sub-pattern must be absent | `"banana~peel"` fires when banana appears without peel |

Repeated segments count: `"无&法&无&天"` requires two matches of `"无"`.

## Examples

```rust
use matcher_rs::{text_process, reduce_text_process, ProcessType};

let result = text_process(ProcessType::Delete, "你好，世界！");
let results = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "你好，世界！");
```

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::VariantNorm, 1, "你好")
    .add_word(ProcessType::VariantNorm, 2, "世界")
    .build()
    .unwrap();

let results = matcher.process("你好，世界！");
```

For more examples, see [test_simple_matcher.rs](./tests/test_simple_matcher.rs).

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `perf` | on | Meta-feature enabling `dfa` and `simd_runtime_dispatch` |
| `dfa` | via `perf` | `aho-corasick` DFA for bytewise engine. 1.7–3.3× faster (Teddy prefilter), ~17× more memory. |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD kernel selection (AVX2/NEON) for transforms and density counting |

### Feature Comparison

| Feature Set | Engine | Speed | Memory | Best For |
|:---|:---|:---|:---|:---|
| **Default (`perf`)** | DFA + SIMD density dispatch | Fastest | Higher | General purpose |
| `--no-default-features --features dfa` | DFA, no SIMD transforms | Fast | Higher | When SIMD dispatch is not needed |
| `--no-default-features` | `daachorse`-only, portable transforms | Good | Lower | Lean builds |

## Benchmarks

Benchmarked on **MacBook Air M4 (24GB RAM)**. Test data: [CN_WORD_LIST_100000](../data/word_list/cn/cn_words_100000.txt) against [CN_HAYSTACK](../data/text/cn/西游记.txt) and [EN_WORD_LIST_100000](../data/word_list/en/en_words_100000.txt) against [EN_HAYSTACK](../data/text/en/sherlock.txt).

Full records: [bench_records/](./bench_records/). Latest: [latest.txt](./bench_records/latest.txt).

```shell
just bench-search                          # Main throughput workflow
just bench-search --quick                  # Quick directional signal (~2-3 min)
just bench-build                           # Matcher construction workflow
just bench-engine-search                   # Raw engine throughput workflow
just bench-engine-is-match                 # Engine is_match workflow
just bench-all                             # All presets
```

Protocol: run serially, benchmark only the affected preset, let the script warm the binary, compare aggregated run sets (not single medians), prefer plugged-in power and low background load.

```shell
just bench-compare <baseline_dir> <candidate_dir>     # aggregated comparison
just bench-compare-raw <baseline.txt> <candidate.txt>  # raw file comparison
```

## Contributing

Contributions welcome! Open an issue for bugs or feature requests. Fork and submit a PR for code contributions.

## License

MIT OR Apache-2.0.

## More Information

[GitHub repository](https://github.com/Lips7/Matcher)
