# Matcher Python Bindings (PyO3)

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)
![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)
![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

Python bindings for the [Matcher](https://github.com/Lips7/Matcher) library — a high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust via PyO3.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

- **Text Transformation**:
  - **VariantNorm**: Simplify traditional Chinese characters to simplified ones.
    Example: `蟲艸` -> `虫艹`
  - **Delete**: Remove specific characters.
    Example: `*Fu&*iii&^%%*&kkkk` -> `Fuiiikkkk`
  - **Normalize**: Normalize special characters to identifiable characters.
    Example: `ＡＢⅣ①℉` -> `ab41°f`
  - **Romanize**: Convert CJK characters to space-separated romanized form (Pinyin, Romaji, RR) for fuzzy matching.
    Example: `西安` -> ` xi an`, matches `洗按` -> ` xi an`, but not `先` -> ` xian`
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
- **Pickle Support**: `SimpleMatcher` instances can be pickled and unpickled for serialization.

## Installation

### Use pip

```shell
pip install matcher_py
```

### Build from source

Requires the Rust **nightly** toolchain.

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher/matcher_py

# Option 1: Using uv (recommended for development)
pip install uv
uv sync

# Option 2: Using maturin directly
pip install maturin
maturin develop --release
```

## Usage

All relevant types are defined in [matcher_py.pyi](./matcher_py.pyi).

### Text Process Usage

Here’s an example of how to use the `reduce_text_process` and `text_process` functions:

```python
from matcher_py import ProcessType, reduce_text_process, text_process

# Combine and reduce multiple transformations
print(reduce_text_process(ProcessType.DELETE_NORMALIZE, "hello, world!"))
# Perform a single transformation
print(text_process(ProcessType.DELETE, "hello, world!"))
```

### Simple Matcher Basic Usage

Here’s an example of how to use the `SimpleMatcher`:

```python
import json

from matcher_py import ProcessType, SimpleMatcher

simple_matcher = SimpleMatcher(
    json.dumps(
        {
            ProcessType.NONE: {
                1: "hello&world",
                2: "word&word~hello"
            },
            ProcessType.DELETE: {
                3: "hallo"
            }
        }
    ).encode()
)
# Check if a text matches
assert simple_matcher.is_match("hello^&!#*#&!^#*()world")
# Perform simple processing
result = simple_matcher.process("hello,world,word,word,hallo")
print(result)
```

## Explanation of the configuration

* `SimpleMatcher`'s configuration is defined by the `SimpleTable = Dict[ProcessType, Dict[int, str]]` type, the value `Dict[int, str]`'s key is called `word_id`, **`word_id` is required to be globally unique**.

### ProcessType

* `NONE`: No transformation.
* `VARIANT_NORM`: Traditional Chinese to simplified Chinese transformation. Based on [VARIANT_NORM](../matcher_rs/process_map/VARIANT_NORM.txt).
  * `測試` -> `测试`
  * `現⾝` -> `现身`
* `DELETE`: Delete all punctuation, special characters, separator characters, and configured control/format codepoints. Based on [TEXT_DELETE](../matcher_rs/process_map/TEXT-DELETE.txt).
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `NORMALIZE`: Normalize all English character variations and number variations to basic characters. Based on [NORM](../matcher_rs/process_map/NORM.txt) and [NUM_NORM](../matcher_rs/process_map/NUM-NORM.txt).
  * `ＡＢⅣ①℉` -> `ab41°f`
  * `ⅠⅡⅢ` -> `123`
* `ROMANIZE`: Convert CJK characters to space-separated romanization (Pinyin, Romaji, RR). Based on [ROMANIZE](../matcher_rs/process_map/ROMANIZE.txt).
  * `你好` -> ` ni hao`
  * `西安` -> ` xi an`
* `ROMANIZE_CHAR`: Convert CJK characters to romanized form without boundary spaces. Based on [ROMANIZE](../matcher_rs/process_map/ROMANIZE.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`
* `EMOJI_NORM`: Convert emoji to English words (CLDR short names) and strip modifiers. Based on [EMOJI_NORM](../matcher_rs/process_map/EMOJI_NORM.txt).
  * `👍🏽` -> `thumbs_up`
  * `🔥` -> `fire`

You can combine these transformations as needed. Pre-defined combinations like `DELETE_NORMALIZE` and `VARIANT_NORM_DELETE_NORMALIZE` are provided for convenience.

Be careful combining `ROMANIZE` and `ROMANIZE_CHAR`: they preserve different word boundaries, so the same input can behave like `xi` + `an` in one pipeline and `xian` in the other.

## Error Handling

- **Construction** (`SimpleMatcher(bytes)`): raises `ValueError` if the JSON is malformed or contains invalid `ProcessType` values. This is the only operation that can fail.
- **Matching** (`is_match`, `process`, `batch_*`): infallible once the matcher is built. These methods never raise exceptions.

## Contributing

Contributions to `matcher_py` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_py` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).
