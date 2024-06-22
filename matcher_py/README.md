# Matcher Rust Implementation with PyO3 Binding

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

## Installation

### Use pip

```shell
pip install matcher_py
```

### Install pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Usage

The `msgspec` library is recommended for serializing the matcher configuration due to its performance benefits. You can also use other msgpack serialization libraries like `ormsgpack`. All relevant types are defined in [extension_types.py](./matcher_py/extension_types.py).

### Explanation of the configuration

* `Matcher`'s configuration is defined by the `MatchTableMap = Dict[int, List[MatchTable]]` type, the key of `MatchTableMap` is called `match_id`, for each `match_id`, the `table_id` inside **should but isn't required to be unique**.
* `SimpleMatcher`'s configuration is defined by the `SimpleMatchTableMap = Dict[SimpleMatchType, Dict[int, str]]` type, the value `Dict[int, str]`'s key is called `word_id`, **`word_id` is required to be globally unique**.

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
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](../matcher_rs/str_conv_map/FANJIAN.txt) and [UNICODE](../matcher_rs/str_conv_map/UNICODE.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [UPPER_LOWER](../matcher_rs/str_conv_map/UPPER-LOWER.txt), [EN_VARIATION](../matcher_rs/str_conv_map/EN-VARIATION.txt) and [NUM_NORM](../matcher_rs/str_conv_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠ϕ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](../matcher_rs/str_conv_map/PINYIN.txt).
  * `你好` -> `␀ni␀␀hao␀`
  * `西安` -> `␀xi␀␀an␀`
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN_CHAR](../matcher_rs/str_conv_map/PINYIN-CHAR.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

`Delete` is technologically a combination of `TextDelete` and `WordDelete`, we implement different delete methods for text and word. 'Cause we believe `CN_SPECIAL` and `EN_SPECIAL` are parts of the word, but not for text. For `text_process` and `reduce_text_process` functions, users should use `TextDelete` instead of `WordDelete`.
* `WordDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](../matcher_rs/str_conv_map/PUNCTUATION-SPECIAL.txt).
* `TextDelete`: Delete all patterns in [PUNCTUATION_SPECIAL](../matcher_rs/str_conv_map/PUNCTUATION-SPECIAL.txt), [CN_SPECIAL](../matcher_rs/str_conv_map/CN-SPECIAL.txt), [EN_SPECIAL](../matcher_rs/str_conv_map/EN-SPECIAL.txt).

### Limitations

Simple Match can handle words with a maximum of **32** combined words (more than 32 then effective combined words are not guaranteed) and **8** repeated words (more than 8 repeated words will be limited to 8).

### Text Process Usage

Here’s an example of how to use the `reduce_text_process` and `text_process` functions:

```python
from matcher_py import reduce_text_process, text_process
from matcher_py.extension_types import SimpleMatchType

print(reduce_text_process(SimpleMatchType.MatchTextDelete | SimpleMatchType.MatchNormalize, "hello, world!"))
print(text_process(SimpleMatchType.MatchTextDelete, "hello, world!"))
```

### Matcher Basic Usage

Here’s an example of how to use the `Matcher`:

```python
import msgspec
import numpy as np
from matcher_py import Matcher
from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType

msgpack_encoder = msgspec.msgpack.Encoder()
matcher = Matcher(
    msgpack_encoder.encode({
        1: [
            MatchTable(
                table_id=1,
                match_table_type=MatchTableType.Simple(simple_match_type = SimpleMatchType.MatchFanjianDeleteNormalize),
                word_list=["hello", "world"],
                exemption_simple_match_type=SimpleMatchType.MatchNone,
                exemption_word_list=["word"],
            )
        ]
    })
)
# Check if a text matches
assert matcher.is_match("hello")
assert not matcher.is_match("hello, word")
# Perform word matching as a dict
assert matcher.word_match(r"hello, world")[1]
# Perform word matching as a string
result = matcher.word_match_as_string("hello")
assert result == """{1:[{\"table_id\":1,\"word\":\"hello\"}]"}"""
# Perform batch processing as a dict using a list
text_list = ["hello", "world", "hello,word"]
batch_results = matcher.batch_word_match(text_list)
print(batch_results)
# Perform batch processing as a string using a list
text_list = ["hello", "world", "hello,word"]
batch_results = matcher.batch_word_match_as_string(text_list)
print(batch_results)
# Perform batch processing as a dict using a numpy array
text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
numpy_results = matcher.numpy_word_match(text_array)
print(numpy_results)
# Perform batch processing as a string using a numpy array
text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
numpy_results = matcher.numpy_word_match_as_string(text_array)
print(numpy_results)
```

### Simple Matcher Basic Usage

Here’s an example of how to use the `SimpleMatcher`:

```python
import msgspec
import numpy as np
from matcher_py import SimpleMatcher
from matcher_py.extension_types import SimpleMatchType

msgpack_encoder = msgspec.msgpack.Encoder()
simple_matcher = SimpleMatcher(
    msgpack_encoder.encode({SimpleMatchType.MatchNone: {1: "example"}})
)
# Check if a text matches
assert simple_matcher.is_match("example")
# Perform simple processing
results = simple_matcher.simple_process("example")
print(results)
# Perform batch processing using a list
text_list = ["example", "test", "example test"]
batch_results = simple_matcher.batch_simple_process(text_list)
print(batch_results)
# Perform batch processing using a NumPy array
text_array = np.array(["example", "test", "example test"], dtype=np.dtype("object"))
numpy_results = simple_matcher.numpy_simple_process(text_array)
print(numpy_results)
```

## Contributing

Contributions to `matcher_py` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_py` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).