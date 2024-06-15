import pytest
import msgspec
import numpy as np
from matcher_py.matcher_py import SimpleMatcher
from matcher_py.extension_types import SimpleMatchType

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
        msgpack_encoder.encode({SimpleMatchType.MatchFanjian: {1: "你好"}})
    )
    assert simple_matcher.is_match("妳好")
    assert simple_matcher.simple_process("你好")[0]["word_id"] == 1
    assert simple_matcher.simple_process("你好")[0]["word"] == "你好"

    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode({SimpleMatchType.MatchFanjian: {1: "妳好"}})
    )
    assert simple_matcher.is_match("你好")
    assert simple_matcher.simple_process("你好")[0]["word_id"] == 1
    assert simple_matcher.simple_process("你好")[0]["word"] == "妳好"


def test_delete():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode({SimpleMatchType.MatchDelete: {1: "你好"}})
    )
    assert simple_matcher.is_match("你！好")
    assert len(simple_matcher.simple_process("你！好")) == 1


def test_normalize():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode(
            {
                SimpleMatchType.MatchNormalize: {
                    1: "he11o",
                }
            }
        )
    )
    assert simple_matcher.is_match("ℋЀ⒈㈠ϕ")
    assert simple_matcher.simple_process("ℋЀ⒈㈠ϕ")[0]["word_id"] == 1
    assert simple_matcher.simple_process("ℋЀ⒈㈠ϕ")[0]["word"] == "he11o"


def test_pinyin():
    simple_matcher = SimpleMatcher(
        msgpack_encoder.encode(
            {
                SimpleMatchType.MatchPinYin: {
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
                SimpleMatchType.MatchPinYinChar: {
                    1: "西安",
                }
            }
        )
    )
    assert simple_matcher.is_match("洗按")
    assert simple_matcher.is_match("现")
    assert simple_matcher.is_match("xian")


@pytest.fixture(scope="module")
def simple_matcher():
    return SimpleMatcher(
        msgpack_encoder.encode({SimpleMatchType.MatchNone: {1: "helloworld"}})
    )


def test_batch_simple_process(simple_matcher):
    assert len(simple_matcher.batch_simple_process(["helloworld"])) == 1


def test_numpy_simple_process(simple_matcher):
    text_array = np.array(["helloworld"] * 1000, dtype=np.dtype("object"))
    simple_matcher.numpy_simple_process(text_array)
    simple_matcher.numpy_simple_process(text_array, inplace=True)
