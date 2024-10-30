# Matcher Rust Implementation with PyO3 Binding

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

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

## Installation

### Use pip

```shell
pip install matcher_py
```

### Install pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## Usage

All relevant types are defined in [extension_types.py](./python/matcher_py/extension_types.py).

### Explanation of the configuration

* `Matcher`'s configuration is defined by the `MatchTableMap = Dict[int, List[MatchTable]]` type, the key of `MatchTableMap` is called `match_id`, **for each `match_id`, the `table_id` inside is required to be unique**.
* `SimpleMatcher`'s configuration is defined by the `SimpleTable = Dict[ProcessType, Dict[int, str]]` type, the value `Dict[int, str]`'s key is called `word_id`, **`word_id` is required to be globally unique**.

#### MatchTable

* `table_id`: The unique ID of the match table.
* `match_table_type`: The type of the match table.
* `word_list`: The word list of the match table.
* `exemption_process_type`: The type of the exemption simple match.
* `exemption_word_list`: The exemption word list of the match table.

For each match table, word matching is performed over the `word_list`, and exemption word matching is performed over the `exemption_word_list`. If the exemption word matching result is True, the word matching result will be False.

#### MatchTableType

* `Simple`: Supports simple multiple patterns matching with text normalization defined by `process_type`.
  * It can handle combination patterns and repeated times sensitive matching, delimited by `&` and `~`, such as `hello&world&hello` will match `hellohelloworld` and `worldhellohello`, but not `helloworld` due to the repeated times of `hello`.
* `Regex`: Supports regex patterns matching.
  * `SimilarChar`: Supports similar character matching using regex.
    * `["hello,hallo,hollo,hi", "word,world,wrd,🌍", "!,?,~"]` will match `helloworld!`, `hollowrd?`, `hi🌍~` ··· any combinations of the words split by `,` in the list.
  * `Acrostic`: Supports acrostic matching using regex **(currently only supports Chinese and simple English sentences)**.
    * `["h,e,l,l,o", "你,好"]` will match `hope, endures, love, lasts, onward.` and `你的笑容温暖, 好心情常伴。`.
  * `Regex`: Supports regex matching.
    * `["h[aeiou]llo", "w[aeiou]rd"]` will match `hello`, `world`, `hillo`, `wurld` ··· any text that matches the regex in the list.
* `Similar`: Supports similar text matching based on distance and threshold.
  * `Levenshtein`: Supports similar text matching based on Levenshtein distance.

#### ProcessType

* `None`: No transformation.
* `Fanjian`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](../matcher_rs/process_map/FANJIAN.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `Delete`: Delete all punctuation, special characters and white spaces. Based on [TEXT_DELETE](../matcher_rs/process_map/TEXT-DELETE.txt) and `WHITE_SPACE`.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `Normalize`: Normalize all English character variations and number variations to basic characters. Based on [NORM](../matcher_rs//process_map/NORM.txt) and [NUM_NORM](../matcher_rs//process_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠Õ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PinYin`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](../matcher_rs/process_map/PINYIN.txt).
  * `你好` -> ` ni  hao `
  * `西安` -> ` xi  an `
* `PinYinChar`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN](../matcher_rs/process_map/PINYIN.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DeleteNormalize` and `FanjianDeleteNormalize` are provided for convenience.

Avoid combining `PinYin` and `PinYinChar` due to that `PinYin` is a more limited version of `PinYinChar`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

### Text Process Usage

Here’s an example of how to use the `reduce_text_process` and `text_process` functions:

```python
from matcher_py import reduce_text_process, text_process
from matcher_py.extension_types import ProcessType

print(reduce_text_process(ProcessType.MatchDeleteNormalize, "hello, world!"))
print(text_process(ProcessType.MatchDelete, "hello, world!"))
```

### Matcher Basic Usage

Here’s an example of how to use the `Matcher`:

```python
import json

from matcher_py import Matcher
from matcher_py.extension_types import MatchTable, MatchTableType, ProcessType, RegexMatchType, SimMatchType

matcher = Matcher(
    json.dumps({
        1: [
            MatchTable(
                table_id=1,
                match_table_type=MatchTableType.Simple(process_type = ProcessType.MatchFanjianDeleteNormalize),
                word_list=["hello", "world"],
                exemption_process_type=ProcessType.MatchNone,
                exemption_word_list=["word"],
            ),
            MatchTable(
                table_id=2,
                match_table_type=MatchTableType.Regex(
                  process_type = ProcessType.MatchFanjianDeleteNormalize,
                  regex_match_type=RegexMatchType.Regex
                ),
                word_list=["h[aeiou]llo"],
                exemption_process_type=ProcessType.MatchNone,
                exemption_word_list=[],
            )
        ],
        2: [
            MatchTable(
                table_id=3,
                match_table_type=MatchTableType.Similar(
                  process_type = ProcessType.MatchFanjianDeleteNormalize,
                  sim_match_type=SimMatchType.MatchLevenshtein,
                  threshold=0.5
                ),
                word_list=["halxo"],
                exemption_process_type=ProcessType.MatchNone,
                exemption_word_list=[],
            )
        ]
    }).encode()
)
# Check if a text matches
assert matcher.is_match("hello")
assert not matcher.is_match("word")
# Perform process as a list
result = matcher.process("hello")
assert result == [{'match_id': 1,
  'table_id': 2,
  'word_id': 0,
  'word': 'h[aeiou]llo',
  'similarity': 1.0},
 {'match_id': 1,
  'table_id': 1,
  'word_id': 0,
  'word': 'hello',
  'similarity': 1.0},
 {'match_id': 2,
  'table_id': 3,
  'word_id': 0,
  'word': 'halxo',
  'similarity': 0.6}]
# Perform word matching as a dict
assert matcher.word_match(r"hello, world")[1] == [{'match_id': 1,
  'table_id': 2,
  'word_id': 0,
  'word': 'h[aeiou]llo',
  'similarity': 1.0},
 {'match_id': 1,
  'table_id': 1,
  'word_id': 0,
  'word': 'hello',
  'similarity': 1.0},
 {'match_id': 1,
  'table_id': 1,
  'word_id': 1,
  'word': 'world',
  'similarity': 1.0}]
# Perform word matching as a string
result = matcher.word_match_as_string("hello")
assert result == """{"2":[{"match_id":2,"table_id":3,"word_id":0,"word":"halxo","similarity":0.6}],"1":[{"match_id":1,"table_id":2,"word_id":0,"word":"h[aeiou]llo","similarity":1.0},{"match_id":1,"table_id":1,"word_id":0,"word":"hello","similarity":1.0}]}"""
```

### Simple Matcher Basic Usage

Here’s an example of how to use the `SimpleMatcher`:

```python
import json

from matcher_py import SimpleMatcher
from matcher_py.extension_types import ProcessType

simple_matcher = SimpleMatcher(
    json.dumps(
        {
            ProcessType.MatchNone: {
                1: "hello&world",
                2: "word&word~hello"
            },
            ProcessType.MatchDelete: {
                3: "hallo"
            }
        }
    ).encode()
)
# Check if a text matches
assert simple_matcher.is_match("hello^&!#*#&!^#*()world")
# Perform simple processing
result = simple_matcher.process("hello,world,word,word,hallo")
assert result == [{'word_id': 1, 'word': 'hello&world'}, {'word_id': 3, 'word': 'hallo'}]
```

## Contributing

Contributions to `matcher_py` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_py` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).