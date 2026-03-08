# Matcher Rust Implementation with PyO3 Binding

![PyPI - Version](https://img.shields.io/pypi/v/matcher_py)
![PyPI - Python Version](https://img.shields.io/pypi/pyversions/matcher_py)
![PyPI - License](https://img.shields.io/pypi/l/matcher_py)

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust with PyO3 bindings.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Features

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
- **Efficient Handling of Large Word Lists**: Optimized for performance.

## Installation

### Use pip

```shell
pip install matcher_py
```

### Build from source

You need to have `rust` and `maturin` installed.

```shell
# Clone the repository
git clone https://github.com/Lips7/Matcher.git
cd Matcher/matcher_py

# Install maturin
pip install maturin

# Build and install the package
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
* `FANJIAN`: Traditional Chinese to simplified Chinese transformation. Based on [FANJIAN](../matcher_rs/process_map/FANJIAN.txt).
  * `妳好` -> `你好`
  * `現⾝` -> `现身`
* `DELETE`: Delete all punctuation, special characters and white spaces. Based on [TEXT_DELETE](../matcher_rs/process_map/TEXT-DELETE.txt) and `WHITE_SPACE`.
  * `hello, world!` -> `helloworld`
  * `《你∷好》` -> `你好`
* `NORMALIZE`: Normalize all English character variations and number variations to basic characters. Based on [NORM](../matcher_rs//process_map/NORM.txt) and [NUM_NORM](../matcher_rs//process_map/NUM-NORM.txt).
  * `ℋЀ⒈㈠Õ` -> `he11o`
  * `⒈Ƨ㊂` -> `123`
* `PINYIN`: Convert all unicode Chinese characters to pinyin with boundaries. Based on [PINYIN](../matcher_rs/process_map/PINYIN.txt).
  * `你好` -> ` ni  hao `
  * `西安` -> ` xi  an `
* `PINYIN_CHAR`: Convert all unicode Chinese characters to pinyin without boundaries. Based on [PINYIN](../matcher_rs/process_map/PINYIN.txt).
  * `你好` -> `nihao`
  * `西安` -> `xian`

You can combine these transformations as needed. Pre-defined combinations like `DELETE_NORMALIZE` and `FANJIAN_DELETE_NORMALIZE` are provided for convenience.

Avoid combining `PINYIN` and `PINYIN_CHAR` due to that `PINYIN` is a more limited version of `PINYIN_CHAR`, in some cases like `xian`, can be treat as two words `xi` and `an`, or only one word `xian`.

## Contributing

Contributions to `matcher_py` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_py` is licensed under the MIT OR Apache-2.0 license.

## More Information

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).