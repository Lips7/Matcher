# Matcher

A high-performance, multi-functional word matcher implemented in Rust.

Designed to solve **AND OR NOT** and **TEXT VARIATIONS** problems in word/word_list matching. For detailed implementation, see the [Design Document](../DESIGN.md).

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

### Adding to Your Project

To use `matcher_rs` in your Rust project, run the following command:

```shell
cargo add matcher_rs
```

### Explanation of the configuration

* `Matcher`'s configuration is defined by the `MatchTableMap = HashMap<u32, Vec<MatchTable>>` type, the key of `MatchTableMap` is called `match_id`, for each `match_id`, the `table_id` inside **should but isn't required to be unique**.
* `SimpleMatcher`'s configuration is defined by the `SimpleMatchTableMap = HashMap<SimpleMatchType, HashMap<u32, &'a str>>` type, the value `HashMap<u32, &'a str>`'s key is called `word_id`, **`word_id` is required to be globally unique**.

#### MatchTable

* `table_id`: The unique ID of the match table.
* `match_table_type`: The type of the match table.
* `word_list`: The word list of the match table.
* `exemption_simple_match_type`: The type of the exemption simple match.
* `exemption_word_list`: The exemption word list of the match table.

For each match table, word matching is performed over the `word_list`, and exemption word matching is performed over the `exemption_word_list`. If the exemption word matching result is True, the word matching result will be False.

#### MatchTableType

* `Simple`: Supports simple multiple patterns matching with text normalization defined by `simple_match_type`.
  * We offer transformation methods for text normalization, including `Fanjian`, `Normalize`, `PinYin` ···.
  * It can handle combination patterns and repeated times sensitive matching, delimited by `,`, such as `hello,world,hello` will match `hellohelloworld` and `worldhellohello`, but not `helloworld` due to the repeated times of `hello`.
* `Regex`: Supports regex patterns matching.
  * `SimilarChar`: Supports similar character matching using regex.
    * `["hello,hallo,hollo,hi", "word,world,wrd,🌍", "!,?,~"]` will match `helloworld`, `hollowrd`, `hi🌍` ··· any combinations of the words split by `,` in the list.
  * `Acrostic`: Supports acrostic matching using regex **(currently only supports Chinese and simple English sentences)**.
    * `["h,e,l,l,o", "你,好"]` will match `hope, endures, love, lasts, onward.` and `你的笑容温暖, 好心情常伴。`.
  * `Regex`: Supports regex matching.
    * `["h[aeiou]llo", "w[aeiou]rd"]` will match `hello`, `world`, `hillo`, `wurld` ··· any text that matches the regex in the list.
* `Similar`: Supports similar text matching based on distance and threshold.
  * `Levenshtein`: Supports similar text matching based on Levenshtein distance.
  * `DamerauLevenshtein`: Supports similar text matching based on Damerau-Levenshtein distance.
  * `Indel`: Supports similar text matching based on Indel distance.
  * `Jaro`: Supports similar text matching based on Jaro distance.
  * `JaroWinkler`: Supports similar text matching based on Jaro-Winkler distance.

#### SimpleMatchType

* `None`: No transformation.
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](./str_conv_map/FANJIAN.txt) and [UNICODE](./str_conv_map/UNICODE.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [UPPER_LOWER](./str_conv_map/UPPER-LOWER.txt), [EN_VARIATION](./str_conv_map/EN-VARIATION.txt) and [NUM_NORM](./str_conv_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠ϕ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](./str_conv_map/PINYIN.txt).
  * `你好` -> `␀ni␀␀hao␀`
  * `西安` -> `␀xi␀␀an␀`
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN_CHAR](./str_conv_map/PINYIN-CHAR.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

`Delete` is technologically a combination of `TextDelete` and `WordDelete`, we implement different delete methods for text and word. 'Cause we believe `CN_SPECIAL` and `EN_SPECIAL` are parts of the word, but not for text. For `text_process` and `reduce_text_process` functions, users should use `TextDelete` instead of `WordDelete`.
* `WordDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](./str_conv_map/PUNCTUATION-SPECIAL.txt).
* `TextDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](./str_conv_map/PUNCTUATION-SPECIAL.txt), [CN_SPECIAL](./str_conv_map/CN-SPECIAL.txt), [EN_SPECIAL](./str_conv_map/EN-SPECIAL.txt).

### Basic Example

Here’s a basic example of how to use the `Matcher` struct for text matching:

```rust
use matcher_rs::{text_process, reduce_text_process, SimpleMatchType};

let result = text_process(SimpleMatchType::TextDelete, "你好，世界！");
let result = reduce_text_process(SimpleMatchType::FanjianDeleteNormalize, "你好，世界！");
```

```rust
use std::collections::HashMap;
use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, SimpleMatchType};

let match_table_map: MatchTableMap = HashMap::from_iter(vec![
    (1, vec![MatchTable {
        table_id: 1,
        match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::FanjianDeleteNormalize},
        word_list: vec!["example", "test"],
        exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
        exemption_word_list: vec![],
    }]),
]);
let matcher = Matcher::new(&match_table_map);
let text = "This is an example text.";
let results = matcher.word_match(text);
```

```rust
use std::collections::HashMap;
use matcher_rs::{SimpleMatchType, SimpleMatcher};

let mut simple_match_type_word_map = HashMap::default();
let mut simple_word_map = HashMap::default();

simple_word_map.insert(1, "你好");
simple_word_map.insert(2, "世界");

simple_match_type_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);

let matcher = SimpleMatcher::new(&simple_match_type_word_map);
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