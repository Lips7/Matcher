# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

It's helpful for
- **Precision and Recall**: Word matching is a retrieval process, LOGICAL match improves precision while TEXT VARIATIONS match improves recall.
- **Content Filtering**: Detecting and filtering out offensive or sensitive words.
- **Search Engines**: Improving search results by identifying relevant keywords.
- **Text Analysis**: Extracting specific information from large volumes of text.
- **Spam Detection**: Identifying spam content in emails or messages.
- ···

## Architecture

```
                          Construction
┌─────────────────────────────────────────────────────────────┐
│  Rules ──▶ parse & dedup ──▶ transform trie ──▶ AC automata │
└─────────────────────────────────────────────────────────────┘

                             Query
┌─────────────────────────────────────────────────────────────┐
│  Text ──▶ walk trie ──▶ scan variants ──▶ evaluate ──▶ hits │
│             │                 │                             │
│        transform text    AC automaton                       │
│        (reuse shared     (bytewise or                       │
│         prefixes)         charwise)                         │
└─────────────────────────────────────────────────────────────┘
```

All sub-patterns are deduplicated into a single Aho-Corasick automaton for O(N) text scanning. Text transformations share a prefix trie so `VariantNorm|Delete` reuses the VariantNorm result. For simple literal matchers without transforms, `is_match` delegates directly to the AC automaton — skipping TLS state setup entirely.

For the full narrative walkthrough, see the [Design Document](./DESIGN.md).

## Features

- **Text Transformation**:
  - **VariantNorm**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `𝜢𝕰𝕃𝙻𝝧 𝙒ⓞᵣℒ𝒟!` -> `hello world!`
  - **Romanize**: Convert CJK characters to space-separated romanized form (Pinyin, Romaji, RR) for fuzzy matching.
    Example: `西安` -> ` xi an`, matches `洗按` -> ` xi an`, but not `先` -> ` xian`
  - **RomanizeChar**: Convert CJK characters to romanized form without boundary spaces.
    Example: `西安` -> `xian`, matches `洗按` and `先` -> `xian`
  - **EmojiNorm**: Convert emoji to English words (CLDR short names) and strip modifiers.
    Example: `👍🏽` -> `thumbs_up`, `🔥` -> `fire`
- **AND OR NOT Word Matching**:
  - Takes into account the number of repetitions of words.
  - `&` (AND): `hello&world` matches `hello world` and `world,hello`
  - `|` (OR): `color|colour` matches `color` and `colour`
  - `~` (NOT): `hello~helloo~hhello` matches `hello` but not `helloo` and `hhello`
  - `\b` (word boundary): `\bcat\b` matches "the cat" but not "concatenate"
  - Repeated segments: `无&法&无&天` matches `无无法天` (because `无` is repeated twice), but not `无法天`
  - Combined: `color|colour&bright~dark` matches "bright color" but not "dark colour"
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Quick Start

<details>
<summary><b>Rust</b></summary>

```toml
# Cargo.toml
[dependencies]
matcher_rs = "0.15"
```

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "hello&world")         // AND: both must appear
    .add_word(ProcessType::None, 2, "color|colour")        // OR: either spelling
    .add_word(ProcessType::None, 3, r"\bcat\b")            // word boundary
    .build()
    .unwrap();

assert!(matcher.is_match("hello, world!"));
assert!(matcher.is_match("nice colour"));
assert!(!matcher.is_match("concatenate"));                  // "cat" not a whole word
```

See the [Rust README](./matcher_rs/README.md) for full docs.

</details>

<details>
<summary><b>Python</b></summary>

```shell
pip install matcher_py
```

```python
from matcher_py import ProcessType, SimpleMatcherBuilder

builder = SimpleMatcherBuilder()
builder.add_word(ProcessType.NONE, 1, "hello&world")     # AND: both must appear
builder.add_word(ProcessType.NONE, 2, "color|colour")    # OR: either spelling
builder.add_word(ProcessType.NONE, 3, r"\bcat\b")        # word boundary
matcher = builder.build()

assert matcher.is_match("hello, world!")
assert matcher.is_match("nice colour")
assert not matcher.is_match("concatenate")  # "cat" not a whole word
```

See the [Python README](./matcher_py/README.md) for full docs.

</details>

<details>
<summary><b>Java</b></summary>

```java
import com.matcherjava.SimpleMatcher;
import com.matcherjava.SimpleMatcherBuilder;
import com.matcherjava.extensiontypes.ProcessType;

try (SimpleMatcher matcher = new SimpleMatcherBuilder()
    .add(ProcessType.NONE, 1, "hello&world")       // AND: both must appear
    .add(ProcessType.NONE, 2, "color|colour")      // OR: either spelling
    .add(ProcessType.NONE, 3, "\\bcat\\b")         // word boundary
    .build()) {
    assert matcher.isMatch("hello, world!");
    assert matcher.isMatch("nice colour");
    assert !matcher.isMatch("concatenate");   // "cat" not a whole word
}
```

See the [Java README](./matcher_java/README.md) for full docs.

</details>

<details>
<summary><b>C</b></summary>

```c
#include "matcher_c.h"

void* builder = init_simple_matcher_builder();
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 1, "hello&world");    // AND
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 2, "color|colour");   // OR
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 3, "\\bcat\\b");      // word boundary
void* matcher = simple_matcher_builder_build(builder);

simple_matcher_is_match(matcher, "hello, world!");   // true  — AND
simple_matcher_is_match(matcher, "nice colour");     // true  — OR
simple_matcher_is_match(matcher, "concatenate");     // false — word boundary
drop_simple_matcher(matcher);
```

See the [C README](./matcher_c/README.md) for full docs.

</details>

### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
just build
```

This builds all packages and copies the dynamic libraries to the right locations. You can also run `cargo build --release` directly — the C and Java libraries will be in `target/release/`:
- `libmatcher_c.so` / `libmatcher_c.dylib` / `matcher_c.dll`
- `libmatcher_java.so` / `libmatcher_java.dylib` / `matcher_java.dll`

## Common Pitfalls

- **`EmojiNorm` + `Delete` don't compose**: `Delete` strips emoji codepoints before `EmojiNorm` can convert them to words. Use `EmojiNorm | Normalize` instead.
- **`Romanize` vs `RomanizeChar`**: `Romanize` adds boundary spaces (`西安` → ` xi an`) so homophones like `洗按` match but `先` doesn't. `RomanizeChar` omits spaces (`xian`) for fuzzier matching.
- **Including `None` in a composite ProcessType**: `None | Delete` matches against *both* the original text and the delete-transformed text. Useful when some sub-patterns should match raw input.
- **Repeated AND segments count repetitions**: `无&法&无&天` requires `无` to appear at least twice in the text.
- **`\b` is per-sub-pattern, not per-rule**: `\bcat\b&dog` requires "cat" as a whole word but "dog" as a substring.

## Benchmarks

Please refer to [benchmarks](./matcher_rs/README.md#benchmarks) for details.
