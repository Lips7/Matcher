# Matcher Rust Implement PyO3 binding
## Usage
Python usage is in the [test.ipynb](matcher_py/test.ipynb) file.
### Matcher
```Python
import msgspec
import numpy as np

from matcher_py import Matcher, SimpleMatcher # type: ignore
from extension_types import MatchTableType, SimpleMatchType, MatchTable

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

matcher.is_match(r"卜")

matcher.word_match(r"你，好")

matcher.word_match_as_string("你好")

matcher.batch_word_match_as_string(["你好", "你好", "你真棒"])

text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object")
)
matcher.numpy_word_match_as_string(text_array)

text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object")
)
matcher.numpy_word_match_as_string(text_array, inplace=True)
text_array
```

### Simple Matcher
```Python
import msgspec
import numpy as np

from matcher_py import Matcher, SimpleMatcher # type: ignore
from extension_types import MatchTableType, SimpleMatchType, MatchTable

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

simple_matcher.is_match("xxx")

simple_matcher.simple_process(r"It's /\/\y duty")

simple_matcher.batch_simple_process([r"It's /\/\y duty", "你好", "xxxxxxx"])

text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object"),
)
simple_matcher.numpy_simple_process(text_array)

text_array = np.array(
    [
        "Laborum eiusmod anim aliqua non veniam laboris officia dolor. Adipisicing sit est irure Lorem duis adipisicing exercitation. Cillum excepteur non anim ipsum eiusmod deserunt veniam. Nulla veniam sunt sint ad velit occaecat in deserunt nulla nisi excepteur. Cillum veniam Lorem aute eu. Nisi voluptate laboris quis sint pariatur ullamco minim pariatur officia non anim nisi nulla ipsum ad. Veniam pariatur ut occaecat ut veniam velit aliquip commodo culpa elit eu eiusmod."
    ]
    * 10000,
    dtype=np.dtype("object"),
)
simple_matcher.numpy_simple_process(text_array, inplace=True)
text_array
```