# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

![Crates.io Version](https://img.shields.io/crates/v/matcher_rs)![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

A high-performance matcher designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching.

It's helpful for
- **Content Filtering**: Detecting and filtering out offensive or sensitive words.
- **Search Engines**: Improving search results by identifying relevant keywords.
- **Text Analysis**: Extracting specific information from large volumes of text.
- **Spam Detection**: Identifying spam content in emails or messages.
- ···

## Features

For detailed implementation, see the [Design Document](./DESIGN.md).

- **Multiple Matching Methods**:
  - Simple Word Matching
  - Regex-Based Matching
  - Similarity-Based Matching
- **Text Transformation**:
  - **Fanjian**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫草`
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
- **Customizable Exemption Lists**: Exclude specific words from matching.
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Usage

Non-Rust users must use **msgpack** for serializing matcher configurations to bytes. **Msgpack** handles backslashes better and is faster than JSON.
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

## Benchmarks

Please refer to [benchmarks](./matcher_rs/README.md#benchmarks) for details.

## Roadmap

### Performance
- [x] ~~Cache middle results during different ProcessType reduce_process_text function calling. (failed, too slow)~~
- [x] Try more aho-corasick library to improve performance and reduce memory usage.
  - [x] ~~https://github.com/daac-tools/crawdad (produce char-wise index, not byte-wise index, it's not acceptable)~~
  - [x] https://github.com/daac-tools/daachorse (use it when Fanjian, PinYin or PinYinChar transformation is performed)
  - [x] ~~Test char-wise HashMap transformation for Chinese Characters. (Too slow)~~
- [x] Make aho-corasick unsafe.
  - [x] See https://github.com/Lips7/aho-corasick.
- [ ] Optimize NOT logic word-wise.
- [x] Optimize `RegexMatcher` using `RegexSet`.
- [x] Optimize `SimpleMatcher` when multiple `ProcessType` are used.
  1. Consider if there are multiple `ProcessType`
   * None
   * Fanjian
   * FanjianDelete
   * FanjianDeleteNormalize
   * FanjianNormalize
  2. We can construct a chain of transformations,
   * None -> Fanjian -> Delete -> Normalize
   * &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;\ -> Normalize.
  3. Calcuate all possible transformations, and cache the results, so that instead calculating 8 times (Fanjian, Fanjian + Delete, Fanjian + Delete + Normalize, Fanjian + Normalize), we only need to calculate 4 times (Fanjian, Delete, Normalize, Normalize).
- [x] ~~Optimize process matcher when perform reduce text processing.~~
  1. Consider we have to perform FanjianDeleteNormalize, we need to perform Fanjian first, then Delete, then Normalize, 3 kinds of Process Matcher are needed to perform replacement or delete, the text has to be scanned 3 times.
  2. What if we only construct only 1 Process Matcher which's patterns contains all the Fanjian, Delete and Normalize 3 kinds of patterns? We could scan the text only once to get all the positions that should be perform replacement or delete.
  3. We need to take care of the byte index will change after replacement or delete, so we need to take the offset changes into account.
- [x] Merge multiple aho-corasick matcher into one when multiple `ProcessType` are used.
- [x] When `dfa` feature is disabled, use daachorse to perform text processing.
  - [x] Do not use it for simple process function, too slow to build.

### Flexibility
- [x] Cache `get_process_matcher` results globally, instead of caching result inside SimpleMatcher.
- [x] Expose `reduce_process_text` to Python.
- [x] Add a new function that can handle single simple match type.
  - [x] `text_process` now is available.
- [x] Add fuzzy matcher, https://github.com/lotabout/fuzzy-matcher.
  - [x] Use `rapidfuzz` instead.
- [x] Make `SimpleMatcher` and `Matcher` serializable.
  - [x] Make aho-corasick serializable.
  - [x] See https://github.com/Lips7/aho-corasick.
- [x] Implement NOT logic word-wise.
- [x] Support stable rust.
- [ ] Support iterator.
- [ ] A real java package.
- [x] Multiple Python version wheel build.
- [ ] Customize str conversion map.
- [x] Add Matcher process function to py, c and java.
- [ ] For simple matcher, is it possible to use regex-automata to replace aho-corasick? and support regex.
- [ ] Add simple match type to `RegexMatcher` and `SimMatcher` to pre-process a text.

### Readability
- [x] More precise and convenient MatchTable.
- [x] More detailed and rigorous benchmarks.
- [x] More detailed and rigorous tests.
- [x] More detailed simple match type explanation.
- [ ] More detailed [DESIGN](./DESIGN.md).
- [ ] Write a Chinese README.