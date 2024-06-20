# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

A high-performance matcher for massive amounts of sensitive words.

Designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching. For detailed implementation, see the [Design Document](./DESIGN.md).

It's helpful for
- **Content Filtering**: Detecting and filtering out offensive or sensitive words.
- **Search Engines**: Improving search results by identifying relevant keywords.
- **Text Analysis**: Extracting specific information from large volumes of text.
- **Spam Detection**: Identifying spam content in emails or messages.
- ···

## Features

- **Multiple Matching Methods**:
  - Simple Word Matching
  - Regex-Based Matching
  - Similarity-Based Matching
- **Text Normalization**:
  - **Fanjian**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `𝜢𝕰𝕃𝙻Ϙ 𝙒ⓞƦℒ𝒟!` -> `hello world`
  - **PinYin**: Convert Chinese characters to Pinyin for fuzzy matching.
    Example: `西安` -> `/xi//an/`, matches `洗按` -> `/xi//an/`, but not `先` -> `/xian/`
  - **PinYinChar**: Convert Chinese characters to Pinyin.
    Example: `西安` -> `xian`, matches `洗按` and `先` -> `xian`
- **Combination and Repeated Word Matching**:
  - Takes into account the number of repetitions of words.
  - Example: `hello,world` matches `hello world` and `world,hello`
  - Example: `无,法,无,天` matches `无无法天` (because `无` is repeated twice), but not `无法天`
- **Customizable Exemption Lists**: Exclude specific words from matching.
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Usage

Non-Rust users must use **msgpack** for serializing matcher configurations to bytes. **Why msgpack?** It handles backslashes better and is faster than JSON.
  - Example issue with JSON: `It's /\/\y duty` is processed incorrectly.

### Rust Users

See the [Rust README](./matcher_rs/README.md).

### Python Users

See the [Python README](./matcher_py/README.md).

### C, Java and Other Users

We provide dynamic library to link. See the [C README](./matcher_c/README.md) and [Java README](./matcher_java/README.md).

#### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

Then you should find the `libmatcher_c.so`/`libmatcher_c.dylib`/`matcher_c.dll` in the `target/release` directory.

#### Pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Roadmap
- [x] Cache get_process_matcher results globally, instead cache result inside SimpleMatcher.
- [x] Expose reduce_process_text to Python.
- [x] Cache middle results during different SimpleMatchType reduce_process_text function calling. (failed, too slow)
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] Try more aho_corasick library to improve performance and reduce memory usage
  - [x] https://github.com/daac-tools/crawdad (produce char-wise index, not byte-wise index, it's not acceptable)
  - [x] https://github.com/daac-tools/daachorse (use it when Fanjian, PinYin or PinYinChar transformation is performed)