# Matcher Rust Implement PyO3 binding
## Usage
Python usage is in the [test.ipynb](matcher_py/test.ipynb) file.
```Python
import msgspec

from matcher_py import Matcher, SimpleMatcher # type: ignore
from extension_types import MatchTableType, SimpleMatchType, MatchTable, MatchTableDict, SimpleWord, SimpleWordlistDict

msgpack_encoder = msgspec.msgpack.Encoder()

matcher = Matcher(
    msgpack_encoder.encode(
        {
            "test": [
                MatchTable(
                    table_id=1,
                    match_table_type=MatchTableType.Simple,
                    wordlist=["xxx"],
                    exemption_wordlist=[],
                    simple_match_type=SimpleMatchType.MatchFanjian | SimpleMatchType.MatchDeleteNormalize
                )
            ]
        }
    )
)

print(matcher.word_match("xxx")) # {"test": "[{"table_id":1,"word":"xxx"}]"}
print(matcher.word_match_as_string("xxx")) # "{"test": "[{"table_id":1,"word":"xxx"}]"}"
print(matcher.batch_word_match_as_string(["xxx", "xx"])) # ["{"test": "[{"table_id":1,"word":"xxx"}]"}"]

simple_matcher = SimpleMatcher(
    msgpack_encoder.encode({
      SimpleMatchType.MatchFanjian
      | SimpleMatchType.MatchDeleteNormalize: [
        {
          "word_id": 1,
          "word": "xxx"
        }
      ]
    })
)

print(simple_matcher.simple_process("xxx")) # [{"word_id":1,"word":"xxx"}]
print(simple_matcher.batch_simple_process(["xxx", "xx"])) # [[{"word_id":1,"word":"xxx"}], []]
```