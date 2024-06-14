# Matcher

A high-performance, multi-functional word matcher implemented in Rust.

## Features

- **Supports Multiple Matching Methods**:
  - Simple word matching
  - Regex-based matching
  - Similarity-based matching
- **Text Normalization Options**:
  - Fanjian (Simplify traditional Chinese characters to simplified ones)
  - DeleteNormalize (Remove whitespaces, punctuation, and non-alphanumeric characters)
  - PinYin (Convert Chinese characters to Pinyin for fuzzy matching)
  - PinYinChar (Convert Chinese characters to Pinyin)
- **Combination and Repeated Word Matching**:
  - Handles combination and repetition of words with specified constraints.

## Usage

### Adding to Your Project

To use `matcher_rs` in your Rust project, add the following to your `Cargo.toml` file:

```toml
[dependencies]
matcher_rs = "*"
```

### Basic Example

Here’s a basic example of how to use the `Matcher` struct for text matching:

```rust
use std::collections::HashMap;
use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, SimpleMatchType};

let match_table_map: MatchTableMap = HashMap::from_iter(vec![
    ("key1", vec![MatchTable {
        table_id: 1,
        match_table_type: MatchTableType::Simple,
        simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        word_list: vec!["example", "test"],
        exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        exemption_word_list: vec![],
    }]),
]);
let matcher = Matcher::new(match_table_map);
let text = "This is an example text.";
let results = matcher.word_match(text);
```

```rust
use std::collections::HashMap;
use matcher_rs::{SimpleMatchType, SimpleMatcher};

let mut simple_match_type_word_map = HashMap::default();
let mut simple_word_map = HashMap::default();

simple_word_map.insert(1, "你好");
simple_word_map.insert(2, "123");

simple_match_type_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);

let matcher = SimpleMatcher::new(simple_match_type_word_map);
let text = "你好，世界！";
let results = matcher.process(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Benchmarks

The `matcher_rs` library includes benchmarks to measure the performance of the matcher. You can find the benchmarks in the [bench.rs](./benches/bench.rs) file. To run the benchmarks, use the following command:

```shell
cargo bench
```

## Contributing

Contributions to `matcher_rs` are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_rs` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).