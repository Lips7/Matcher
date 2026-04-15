# Matcher

A high-performance Rust library for multi-pattern matching with logical operators (`&`/`~`/`|`) and configurable text normalization pipelines. Designed for content moderation, keyword filtering, and any scenario where you need to match thousands of rules against text with precision control over recall.

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
| `EmojiNorm` | Emoji → English words (CLDR short names): `👍🏽` → `thumbs_up`, `🔥` → `fire` |

Compose with `|`: `ProcessType::VariantNorm | ProcessType::Delete`. Pre-defined aliases: `DeleteNormalize`, `VariantNormDeleteNormalize`.

**Note:** `EmojiNorm` does not compose usefully with `Delete` — Delete removes emoji before EmojiNorm can see them. Use `EmojiNorm | Normalize` for emoji→word matching.

Including `None` in a composite type keeps the raw-text path alongside transformed variants — one sub-pattern can match raw text while another matches the transformed variant.

## Rule Syntax

| Operator | Meaning | Example |
|----------|---------|---------|
| `&` | All sub-patterns must appear (any order) | `"apple&pie"` fires when both appear |
| `\|` | Any alternative matches the segment | `"color\|colour"` fires when either appears |
| `~` | Following sub-pattern must be absent | `"banana~peel"` fires when banana appears without peel |
| `\b` | Word boundary at start/end of sub-pattern | `"\bcat\b"` matches "the cat" but not "concatenate" |

`|` binds tighter than `&`/`~`: `"a|b&c|d~e|f"` means (a OR b) AND (c OR d) AND NOT (e OR f).
`\b` is per-sub-pattern (after `&`/`~`/`|` splitting): `"\bcat\b&dog"` requires "cat" as whole word, "dog" as substring.

Repeated segments count: `"无&法&无&天"` requires two matches of `"无"`.

## Examples

### Text Processing

```rust
use matcher_rs::{text_process, reduce_text_process, ProcessType};

let result = text_process(ProcessType::Delete, "你好，世界！");
let results = reduce_text_process(ProcessType::VariantNormDeleteNormalize, "你好，世界！");
```

### AND Matching

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::VariantNorm, 1, "你好")
    .add_word(ProcessType::VariantNorm, 2, "世界")
    .build()
    .unwrap();

let results = matcher.process("你好，世界！");
```

### OR Matching

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "color|colour")
    .build()
    .unwrap();

assert!(matcher.is_match("nice color"));   // matches "color"
assert!(matcher.is_match("nice colour"));  // matches "colour"
assert!(!matcher.is_match("nice hue"));
```

### NOT Matching

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "banana~peel")
    .build()
    .unwrap();

assert!(matcher.is_match("banana split"));   // "banana" present, "peel" absent
assert!(!matcher.is_match("banana peel"));   // vetoed by "peel"
```

### Word Boundary

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, r"\bcat\b")
    .build()
    .unwrap();

assert!(matcher.is_match("the cat sat"));    // whole word "cat"
assert!(!matcher.is_match("concatenate"));   // "cat" is a substring, not a word
assert!(!matcher.is_match("cats and dogs")); // "cats" ≠ "cat"
```

### Combined Operators

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

// (bright) AND (color OR colour) AND NOT (\bdark\b)
let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, r"bright&color|colour~\bdark\b")
    .build()
    .unwrap();

assert!(matcher.is_match("bright colour"));       // AND + OR satisfied, no veto
assert!(!matcher.is_match("bright dark color"));   // vetoed by whole-word "dark"
assert!(matcher.is_match("bright darken color"));  // "darken" ≠ "\bdark\b"
```

### Callback & Early-Exit API

Beyond `process()` (returns `Vec`) and `process_into()` (reuses `Vec`), two zero/low-allocation query methods are available:

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "hello")
    .add_word(ProcessType::None, 2, "world")
    .add_word(ProcessType::None, 3, "missing")
    .build()
    .unwrap();

// for_each_match — zero-allocation callback, early exit by returning true
let mut ids = Vec::new();
matcher.for_each_match("hello world", |r| {
    ids.push(r.word_id);
    false // continue
});
assert_eq!(ids.len(), 2);

// find_match — first matching rule (early exit)
let first = matcher.find_match("hello world").unwrap();
assert!(first.word_id == 1 || first.word_id == 2);
```

For more examples, run `cargo run --example basic -p matcher_rs` or see [test_operators.rs](./tests/test_operators.rs).

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `perf` | on | Meta-feature enabling `dfa` + `simd_runtime_dispatch` |
| `dfa` | via `perf` | `aho-corasick` DFA for bytewise engine. 1.7–3.3× faster, ~17× more memory. |
| `simd_runtime_dispatch` | via `perf` | Runtime SIMD kernel selection (AVX2/NEON) for transforms and `bytecount` character density |
| `rayon` | off | Parallel batch API (`batch_is_match`, `batch_process`, `batch_find_match`) via rayon. Enabled by all binding crates. |
| `serde` | off | Enables `Serialize`/`Deserialize` for `ProcessType` and `Serialize` for `SimpleResult`. Required for JSON constructor. Enabled by all binding crates. |

### Feature Comparison

| Feature Set | Engine | Speed | Memory | Best For |
|:---|:---|:---|:---|:---|
| **Default (`perf`)** | DFA + SIMD char-density dispatch | Fastest | Higher | General purpose |
| `--no-default-features --features dfa` | DFA, no SIMD transforms | Fast | Higher | When SIMD dispatch is not needed |
| `--no-default-features` | `daachorse`-only, portable transforms | Good | Lower | Lean builds |

### When to Use Which

- **Default (`perf`)**: Best for most use cases. DFA + SIMD gives maximum throughput. Use this unless you have a specific constraint.
- **Drop DFA** (`--no-default-features --features simd_runtime_dispatch`): When memory is constrained — DFA uses ~17× more memory than the DAAC fallback. Still gets SIMD-accelerated transforms and character density dispatch.
- **Minimal** (`--no-default-features`): Embedded, WASM, or minimal-dependency builds. Portable across all targets with good baseline performance.
- **Platform notes**: AVX2 (x86_64) and NEON (aarch64) are auto-detected at runtime when `simd_runtime_dispatch` is enabled — no manual target-feature flags needed.

## Benchmarks

Benchmarked on **MacBook Pro M3 Max**. Test data: [CN_WORD_LIST](../data/word/cn/jieba.txt) against [CN_HAYSTACK](../data/text/cn/三体.txt) and [EN_WORD_LIST](../data/word/en/dictionary.txt) against [EN_HAYSTACK](../data/text/en/sherlock.txt).

Latest: [latest_benchmark.txt](./latest_benchmark.txt).

```shell
# Run benchmarks
just bench-search                                      # Main throughput workflow (~15 min)
just bench-search --quick                              # Quick directional signal (~2-3 min)
just bench-search --filter text_transform              # Only transform benchmarks (~2 min)
just bench-search --filter rule_complexity             # Only rule shape benchmarks (~3 min)
just bench-search --filter "scaling::process_cn"       # Single benchmark group (~1 min)
just bench-build                                       # Matcher construction workflow
just bench-all                                         # All presets (search + build)

# Compare runs
just bench-compare <baseline> <candidate>              # Compare runs, dirs, or raw files

# Visualize
just bench-viz <run_dir>                               # Interactive HTML dashboard (Plotly)
just bench-viz <baseline_dir> <candidate_dir>          # Comparison visualization
```

Protocol: run serially, benchmark only the affected preset, let the script warm the binary, compare aggregated run sets (not single medians), prefer plugged-in power and low background load.

### Engine Characterization

Sweep the full (engine × size × pat_cjk × text_cjk) matrix to understand dispatch behavior:

```shell
just characterize-engines                              # Full sweep (~20-30 min)
just characterize-engines-quick                        # Subset (~3 min)
just characterize-viz <csv_file>                       # Interactive Plotly heatmap
```

### Profiling

Profile with Xcode Instruments (Time Profiler):

```shell
cargo run --profile profiling --example profile_search -p matcher_rs -- --list
cargo run --profile profiling --example profile_search -p matcher_rs -- --scene all
just profile record --scene en-search --analyze
just profile record --mode is_match --dict en --rules 10000 --analyze
```

## Contributing

Contributions welcome! Open an issue for bugs or feature requests. Fork and submit a PR for code contributions.

## License

MIT OR Apache-2.0.

## More Information

[GitHub repository](https://github.com/Lips7/Matcher)
