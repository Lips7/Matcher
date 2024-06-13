# Matcher

A high-performance matcher for massive amounts of sensitive words.

## Features

- **Multiple Matching Methods**:
  - **Simple Word Matching**
  - **Regex-Based Matching**
  - **Similarity-Based Matching**
- **Text Normalization**:
  - **Fanjian**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **DeleteNormalize**: Remove whitespaces, punctuation, and other non-alphanumeric characters.
    Example: `𝜢𝕰𝕃𝙻Ϙ 𝙒ⓞƦℒ𝒟!` -> `helloworld`
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

- Can handle words with a maximum of 32 combined words and 8 repeated words.
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