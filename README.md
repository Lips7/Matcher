# Matcher

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)![Python](https://img.shields.io/badge/python-3670A0?style=for-the-badge&logo=python&logoColor=ffdd54)![Java](https://img.shields.io/badge/java-%23ED8B00.svg?style=for-the-badge&logo=openjdk&logoColor=white)![C](https://img.shields.io/badge/c-%2300599C.svg?style=for-the-badge&logo=c&logoColor=white)

![GitHub License](https://img.shields.io/github/license/lips7/Matcher)![GitHub Tag](https://img.shields.io/github/v/tag/Lips7/Matcher)

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)![docs.rs](https://img.shields.io/docsrs/matcher_rs)![Crates.io Total Downloads](https://img.shields.io/crates/d/matcher_rs)

![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)![PyPI - Downloads](https://img.shields.io/pypi/dm/matcher_py)

A high-performance matcher for massive amounts of sensitive words.

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

## Limitations
- `SimpleMatchType` has only 6 available flags (`None`, `Fanjian`, `Delete`, `Normalize`, `PinYin`, `PinYinChar`), others are just pre-defined combination of them.
  - `Delete` is a combination of `WordDelete` and `TextDelete`, perform different delete strategy on word and text.
  - `PinYin` and `PinYinChar` shouldn't be enabled at same time. 'cause `PinYin` is a more limited version of `PinYinChar`, users'd better choose one of them.
- Can handle words with a maximum of 32 combined words (more than 32 then effective combined words are not guaranteed) and 8 repeated words (more than 8 repeated words will be limited to 8).
- Users must ensure the correctness of input data and the global uniqueness of `match_id`, `table_id`, and `word_id`.

## Usage

### General Instructions

- Non-Rust users must use **msgpack** for serializing matcher configurations to bytes.
  - **Why msgpack?** It handles backslashes better and is faster than JSON.
  - Example issue with JSON: `It's /\/\y duty` is processed incorrectly.

### Platform-Specific Instructions

#### Rust Users
- See the [Rust README](./matcher_rs/README.md)

#### Python Users
- See the [Python README](./matcher_py/README.md)

#### Java Users
- Install Rust.
- Clone the repository.
- Run `cargo build --release`.
- Copy `target/release/libmatcher.so` (or `libmatcher.dylib` for Mac) to `matcher_java/src/resources/matcher_c.so`.
- See the [Java README](./matcher_java/README.md)

#### C Users
- Install Rust.
- Clone the repository.
- Run `cargo build --release`.
- Copy `target/release/libmatcher.so` (or `libmatcher.dylib` for Mac) to `matcher_c/matcher_c.so`.
- See the [C README](./matcher_c/README.md)

## Design

- Most features are based on [aho_corasick](https://github.com/BurntSushi/aho-corasick), which supports finding multiple patterns simultaneously with SIMD acceleration in some cases.
- For detailed implementation, see the [Design Document](./DESIGN.md).