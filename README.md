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

## Features

For detailed implementation, see the [Design Document](./DESIGN.md).

- **Text Transformation**:
  - **VariantNorm**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `𝜢𝕰𝕃𝙻𝝧 𝙒ⓞᵣℒ𝒟!` -> `hello world!`
  - **Romanize**: Convert CJK characters to space-separated romanized form (Pinyin, Romaji, RR) for fuzzy matching.
    Example: `西安` -> ` xi  an `, matches `洗按` -> ` xi  an `, but not `先` -> ` xian `
  - **RomanizeChar**: Convert CJK characters to romanized form without boundary spaces.
    Example: `西安` -> `xian`, matches `洗按` and `先` -> `xian`
  - **EmojiNorm**: Convert emoji to English words (CLDR short names) and strip modifiers.
    Example: `👍🏽` -> `thumbs_up`, `🔥` -> `fire`
- **AND OR NOT Word Matching**:
  - Takes into account the number of repetitions of words.
  - `&` (AND): `hello&world` matches `hello world` and `world,hello`
  - `|` (OR): `color|colour` matches `color` and `colour`
  - `~` (NOT): `hello~helloo~hhello` matches `hello` but not `helloo` and `hhello`
  - Repeated segments: `无&法&无&天` matches `无无法天` (because `无` is repeated twice), but not `无法天`
  - Combined: `color|colour&bright~dark` matches "bright color" but not "dark colour"
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Quick Start

<details>
<summary><b>Rust</b></summary>

```toml
# Cargo.toml
[dependencies]
matcher_rs = "0.13"
```

```rust
use matcher_rs::{ProcessType, SimpleMatcherBuilder};

let matcher = SimpleMatcherBuilder::new()
    .add_word(ProcessType::None, 1, "hello&world")
    .build()
    .unwrap();

assert!(matcher.is_match("hello, world!"));
```

See the [Rust README](./matcher_rs/README.md) for full docs.

</details>

<details>
<summary><b>Python</b></summary>

```shell
pip install matcher_py
```

```python
import json
from matcher_py import ProcessType, SimpleMatcher

matcher = SimpleMatcher(
    json.dumps({ProcessType.NONE: {1: "hello&world"}}).encode()
)
assert matcher.is_match("hello, world!")
```

See the [Python README](./matcher_py/README.md) for full docs.

</details>

<details>
<summary><b>Java</b></summary>

```java
import com.matcherjava.SimpleMatcher;

byte[] config = "{\"1\":{\"1\":\"hello&world\"}}".getBytes();
try (SimpleMatcher matcher = new SimpleMatcher(config)) {
    assert matcher.isMatch("hello, world!");
}
```

See the [Java README](./matcher_java/README.md) for full docs.

</details>

<details>
<summary><b>C</b></summary>

```c
#include "matcher_c.h"

void* m = init_simple_matcher("{\"1\":{\"1\":\"hello&world\"}}");
bool matched = simple_matcher_is_match(m, "hello, world!");
drop_simple_matcher(m);
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

## Benchmarks

Please refer to [benchmarks](./matcher_rs/README.md#benchmarks) for details.
