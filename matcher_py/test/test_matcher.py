import pytest

import msgspec
import numpy as np
from matcher_py.matcher_py import Matcher
from matcher_py.extension_types import (
    SimpleMatchType,
    MatchTable,
    MatchTableType,
    RegexMatchType,
    SimMatchType,
)

msgpack_encoder = msgspec.msgpack.Encoder()
json_encoder = msgspec.json.Encoder()
json_decoder = msgspec.json.Decoder()


def test_init_with_non_bytes():
    with pytest.raises(TypeError):
        Matcher(1)
        Matcher("")
        Matcher([])
        Matcher({})


def test_init_with_invalid_bytes():
    with pytest.raises(ValueError):
        Matcher(b"")
        Matcher(b"123")
        Matcher(b"invalid")
        Matcher(b"[]")
        Matcher(b"{}")


def test_init_with_empty_map():
    Matcher(msgpack_encoder.encode({}))
    Matcher(msgpack_encoder.encode({1: []}))
    Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            SimpleMatchType.MatchNone
                        ),
                        word_list=[],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )


def test_init_with_invalid_map():
    with pytest.raises(ValueError):
        Matcher(msgpack_encoder.encode({"a": 1}))
        Matcher(msgpack_encoder.encode({"a": {"b": 1}}))
        Matcher(msgpack_encoder.encode({"c": {}}))


def test_regex():
    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            RegexMatchType.MatchRegex
                        ),
                        word_list=["h[aeiou]llo", "w[aeiou]rd"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )
    assert matcher.is_match("hallo")
    assert matcher.is_match("ward")
    assert matcher.word_match("hallo")[1][0]["table_id"] == 1
    assert matcher.word_match("hallo")[1][0]["word"] == "h[aeiou]llo"


def test_similar_char():
    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            RegexMatchType.MatchSimilarChar
                        ),
                        word_list=["hello,hi,H,你好", "world,word,🌍,世界"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )
    assert matcher.is_match("helloworld")
    assert matcher.is_match("hi世界")
    assert matcher.word_match("helloworld")[1][0]["table_id"] == 1
    assert matcher.word_match("helloworld")[1][0]["word"] == "helloworld"


def test_similar_text_levenshtein():
    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Similar(
                            SimMatchType.MatchLevenshtein, 0.8
                        ),
                        word_list=["helloworld"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )
    assert matcher.is_match("helloworl")
    assert matcher.is_match("halloworld")
    assert matcher.is_match("ha1loworld")
    assert not matcher.is_match("ha1loworld1")
    assert matcher.word_match("helloworl")[1][0]["table_id"] == 1
    assert matcher.word_match("helloworl")[1][0]["word"] == "helloworld"


def test_acrostic():
    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            RegexMatchType.MatchAcrostic
                        ),
                        word_list=["h,e,l,l,o", "你,好"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )
    assert matcher.is_match("hope, endures, love, lasts, onward.")
    assert matcher.is_match(
        "Happy moments shared, Every smile and laugh, Love in every word, Lighting up our paths, Open hearts we show."
    )
    assert matcher.is_match("你的笑容温暖, 好心情常伴。")
    assert not matcher.is_match("你好")
    assert (
        matcher.word_match("hope, endures, love, lasts, onward.")[1][0]["word"]
        == "h,e,l,l,o"
    )
    assert matcher.word_match("你的笑容温暖, 好心情常伴。")[1][0]["word"] == "你,好"


def test_exemption():
    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            SimpleMatchType.MatchNone
                        ),
                        word_list=["helloworld"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=["worldwide"],
                    )
                ]
            }
        )
    )
    assert matcher.is_match("helloworld")
    assert not matcher.is_match("helloworldwide")

    matcher = Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            SimpleMatchType.MatchNone
                        ),
                        word_list=["helloworld"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=["worldwide"],
                    ),
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            RegexMatchType.MatchRegex
                        ),
                        word_list=["hello"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=["worldwide"],
                    ),
                ]
            }
        )
    )
    assert matcher.is_match("helloworld")
    assert not matcher.is_match("helloworldwide")


@pytest.fixture(scope="module")
def matcher():
    return Matcher(
        msgpack_encoder.encode(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            SimpleMatchType.MatchNone
                        ),
                        word_list=["helloworld"],
                        exemption_simple_match_type=SimpleMatchType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        )
    )


def test_batch_word_match(matcher):
    assert len(matcher.batch_word_match(["helloworld"])) == 1


def test_batch_word_match_as_string(matcher):
    assert len(matcher.batch_word_match_as_string(["helloworld"])) == 1


def test_numpy_word_match(matcher):
    text_array = np.array(["helloworld"] * 1000, dtype=np.dtype("object"))
    matcher.numpy_word_match(text_array)
    matcher.numpy_word_match(text_array, inplace=True)


def test_numpy_word_match_as_string(matcher):
    text_array = np.array(["helloworld"] * 1000, dtype=np.dtype("object"))
    matcher.numpy_word_match_as_string(text_array)
    matcher.numpy_word_match_as_string(text_array, inplace=True)
