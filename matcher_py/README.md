# Matcher Rust Implementation with PyO3 Binding

## Installation

To install the `matcher_py` package, use pip:

```shell
pip install matcher_py
```

Or you can download pre-built `matcher_py` in [release](https://github.com/Lips7/Matcher/releases).

## Usage

### Python Usage

Refer to the [test.ipynb](./matcher_py/test.ipynb) file for Python usage examples.

The `msgspec` library is used to serialize the matcher configuration. You can also use `ormsgpack` or other msgpack serialization libraries, but for performance considerations, we recommend `msgspec`. All types are defined in [extension_types.py](./matcher_py/extension_types.py).

### Matcher

Here’s an example of how to use the `Matcher`:

```python
import msgspec
import numpy as np
from matcher_py import Matcher # type: ignore
from matcher_py.extension_types import MatchTableType, SimpleMatchType, MatchTable

msgpack_encoder = msgspec.msgpack.Encoder()

matcher = Matcher(
    msgpack_encoder.encode(
        {
            "test": [
                MatchTable(
                    table_id=1,
                    match_table_type=MatchTableType.Simple,
                    simple_match_type=SimpleMatchType.MatchFanjian | SimpleMatchType.MatchDeleteNormalize,
                    word_list=["蔔", "你好"],
                    exemption_simple_match_type=SimpleMatchType.MatchFanjian | SimpleMatchType.MatchDeleteNormalize,
                    exemption_word_list=[],
                )
            ]
        }
    )
)

# Perform matching
matcher.is_match(r"卜")
matcher.word_match(r"你，好")
matcher.word_match_as_string("你好")
matcher.batch_word_match_as_string(["你好", "你好", "你真棒"])

# Numpy integration for batch processing
text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object")
)
matcher.numpy_word_match_as_string(text_array)
matcher.numpy_word_match_as_string(text_array, inplace=True)
print(text_array)
```

### Simple Matcher

Here’s an example of how to use the `SimpleMatcher`:

```python
import msgspec
import numpy as np
from matcher_py import SimpleMatcher # type: ignore
from matcher_py.extension_types import SimpleMatchType

msgpack_encoder = msgspec.msgpack.Encoder()

simple_matcher = SimpleMatcher(
    msgpack_encoder.encode(
        {
            SimpleMatchType.MatchFanjian | SimpleMatchType.MatchDeleteNormalize: {
                1: "无,法,无,天",
                2: "xxx",
                3: "你好",
                6: r"It's /\/\y duty",
                4: "xxx,yyy",
            },
            SimpleMatchType.MatchFanjian: {
                4: "xxx,yyy",
            },
            SimpleMatchType.MatchNone: {
                5: "xxxxx,xxxxyyyyxxxxx",
            },
        }
    )
)

# Perform matching
simple_matcher.is_match("xxx")
simple_matcher.simple_process(r"It's /\/\y duty")
simple_matcher.batch_simple_process([r"It's /\/\y duty", "你好", "xxxxxxx"])

# Numpy integration for batch processing
text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object"),
)
simple_matcher.numpy_simple_process(text_array)
simple_matcher.numpy_simple_process(text_array, inplace=True)
print(text_array)
```

## Contributing

Contributions to `matcher_py` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_py` is licensed under the MIT OR Apache-2.0 license.

For more details, visit the [GitHub repository](https://github.com/Lips7/Matcher).