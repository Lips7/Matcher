# Matcher

A high performance multiple functional word matcher implemented in Rust.

## Usage

To use `matcher_rs` in your Rust project, add the following to your `Cargo.toml` file:

```toml
[dependencies]
matcher_rs = "0.1.6"
```

You can then use the Matcher struct to perform text matching. Here's a basic example:

```rust
use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, SimpleMatchType};
use gxhash::HashMap as GxHashMap;

let match_table_map: MatchTableMap = GxHashMap::from_iter(vec![
    ("key1", vec![MatchTable {
        table_id: 1,
        match_table_type: MatchTableType::Simple,
        simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        word_list: vec!["example", "test"],
        exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        exemption_word_list: vec![],
    }]),
]);
let matcher = Matcher::new(&match_table_map);
let text = "This is an example text.";
let results = matcher.word_match(text);
```

For more detailed usage examples, please refer to the [test.rs](./tests/test.rs) file.

## Benchmarks
- The matcher_rs library includes benchmarks to measure the performance of the matcher. You can find the benchmarks in the [bench.rs](./benches/bench.rs) file. To run the benchmarks, use the following command:

```shell
cargo bench
```

## Contributing
Contributions to matcher_rs are welcome! If you find a bug or have a feature request, please open an issue on the GitHub repository. If you would like to contribute code, please fork the repository and submit a pull request.

## License
matcher_rs is licensed under the MIT OR Apache-2.0 license. See the [LICENSE](../License.md) file for more information.