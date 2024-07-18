import pytest

import msgspec

from matcher_py.matcher_py import SimpleMatcher
from matcher_py.extension_types import ProcessType

msgpack_encoder = msgspec.msgpack.Encoder()
json_encoder = msgspec.json.Encoder()
json_decoder = msgspec.json.Decoder()


def test_init_with_non_bytes():
    with pytest.raises(TypeError):
        SimpleMatcher(1)
        SimpleMatcher("")
        SimpleMatcher([])
        SimpleMatcher({})


def test_init_with_invalid_bytes():
    with pytest.raises(ValueError):
        SimpleMatcher(b"")
        SimpleMatcher(b"123")
        SimpleMatcher(b"invalid")
        SimpleMatcher(b"[]")
        SimpleMatcher(b"{}")


def test_init_with_empty_map():
    SimpleMatcher(msgpack_encoder.encode({}))
    SimpleMatcher(msgpack_encoder.encode({1: {}}))


def test_init_with_invalid_map():
    with pytest.raises(ValueError):
        SimpleMatcher(msgpack_encoder.encode({"a": 1}))
        SimpleMatcher(msgpack_encoder.encode({"a": {"b": 1}}))
        SimpleMatcher(msgpack_encoder.encode({1: []}))


def test_fanjian():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode({ProcessType.MatchFanjian: {1: "你好"}})
    )
    assert simple_matcher.is_match("妳好")
    assert simple_matcher.process("你好")[0]["word_id"] == 1
    assert simple_matcher.process("你好")[0]["word"] == "你好"

    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode({ProcessType.MatchFanjian: {1: "妳好"}})
    )
    assert simple_matcher.is_match("你好")
    assert simple_matcher.process("你好")[0]["word_id"] == 1
    assert simple_matcher.process("你好")[0]["word"] == "妳好"


def test_delete():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode({ProcessType.MatchDelete: {1: "你好"}})
    )
    assert simple_matcher.is_match("你！好")
    assert len(simple_matcher.process("你！好")) == 1


def test_normalize():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode(
            {
                ProcessType.MatchNormalize: {
                    1: "he11o",
                }
            }
        )
    )
    assert simple_matcher.is_match("ℋЀ⒈㈠Õ")
    assert simple_matcher.process("ℋЀ⒈㈠Õ")[0]["word_id"] == 1
    assert simple_matcher.process("ℋЀ⒈㈠Õ")[0]["word"] == "he11o"


def test_pinyin():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode(
            {
                ProcessType.MatchPinYin: {
                    1: "西安",
                }
            }
        )
    )
    assert simple_matcher.is_match("洗按")
    assert not simple_matcher.is_match("现")


def test_pinyinchar():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode(
            {
                ProcessType.MatchPinYinChar: {
                    1: "西安",
                }
            }
        )
    )
    assert simple_matcher.is_match("洗按")
    assert simple_matcher.is_match("现")
    assert simple_matcher.is_match("xian")
