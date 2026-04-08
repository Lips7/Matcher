import json

import pytest

from matcher_py import ProcessType, SimpleMatcher


def test_init_with_non_bytes():
    with pytest.raises(TypeError):
        SimpleMatcher(1)  # ty: ignore[invalid-argument-type]
    with pytest.raises(TypeError):
        SimpleMatcher("")  # ty: ignore[invalid-argument-type]
    with pytest.raises(TypeError):
        SimpleMatcher([])  # ty: ignore[invalid-argument-type]
    with pytest.raises(TypeError):
        SimpleMatcher({})  # ty: ignore[invalid-argument-type]


def test_init_with_invalid_bytes():
    with pytest.raises(ValueError):
        SimpleMatcher(b"")
    with pytest.raises(ValueError):
        SimpleMatcher(b"123")
    with pytest.raises(ValueError):
        SimpleMatcher(b"invalid")
    with pytest.raises(ValueError):
        SimpleMatcher(b"[]")


def test_init_with_empty_map():
    with pytest.raises(ValueError):
        SimpleMatcher(json.dumps({}).encode())
    with pytest.raises(ValueError):
        SimpleMatcher(json.dumps({1: {}}).encode())


def test_init_with_invalid_map():
    with pytest.raises(ValueError):
        SimpleMatcher(json.dumps({"a": 1}).encode())
    with pytest.raises(ValueError):
        SimpleMatcher(json.dumps({"a": {"b": 1}}).encode())
    with pytest.raises(ValueError):
        SimpleMatcher(json.dumps({1: []}).encode())


def test_backslashes():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.NONE: {1: r"It's /\/\y duty"}}).encode()
    )
    assert simple_matcher.is_match(r"It's /\/\y duty")
    assert simple_matcher.process(r"It's /\/\y duty")[0].word == r"It's /\/\y duty"


def test_variant_norm():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.VARIANT_NORM: {1: "测试"}}).encode()
    )
    assert simple_matcher.is_match("測試")
    assert simple_matcher.process("测试")[0].word_id == 1
    assert simple_matcher.process("测试")[0].word == "测试"

    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.VARIANT_NORM: {1: "測試"}}).encode()
    )
    assert simple_matcher.is_match("测试")
    assert simple_matcher.process("测试")[0].word_id == 1
    assert simple_matcher.process("测试")[0].word == "測試"


def test_delete():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.DELETE: {1: "你好"}}).encode()
    )
    assert simple_matcher.is_match("你！好")
    assert len(simple_matcher.process("你！好")) == 1


def test_normalize():
    simple_matcher = SimpleMatcher(
        json.dumps(
            {
                ProcessType.NORMALIZE: {
                    1: "ab41°f",
                }
            }
        ).encode()
    )
    assert simple_matcher.is_match("ＡＢⅣ①℉")
    assert simple_matcher.process("ＡＢⅣ①℉")[0].word_id == 1
    assert simple_matcher.process("ＡＢⅣ①℉")[0].word == "ab41°f"


def test_romanize():
    simple_matcher = SimpleMatcher(
        json.dumps(
            {
                ProcessType.ROMANIZE: {
                    1: "西安",
                }
            }
        ).encode()
    )
    assert simple_matcher.is_match("洗按")
    assert not simple_matcher.is_match("现")


def test_romanize_char():
    simple_matcher = SimpleMatcher(
        json.dumps(
            {
                ProcessType.ROMANIZE_CHAR: {
                    1: "西安",
                }
            }
        ).encode()
    )
    assert simple_matcher.is_match("洗按")
    assert simple_matcher.is_match("现")
    assert simple_matcher.is_match("xian")


def test_batch_is_match():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.NONE: {1: "hello", 2: "world"}}).encode()
    )
    assert simple_matcher.batch_is_match([]) == []
    assert simple_matcher.batch_is_match(["hello"]) == [True]
    assert simple_matcher.batch_is_match(["hello", "miss", "world", ""]) == [
        True,
        False,
        True,
        False,
    ]


def test_batch_process():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.NONE: {1: "hello", 2: "world"}}).encode()
    )
    results = simple_matcher.batch_process(["hello world", "miss", "hello"])
    assert len(results) == 3

    ids_0 = sorted(r.word_id for r in results[0])
    assert ids_0 == [1, 2]

    assert results[1] == []

    assert len(results[2]) == 1
    assert results[2][0].word_id == 1
    assert results[2][0].word == "hello"


def test_from_dict():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello", 2: "world"}})
    assert matcher.is_match("hello")
    assert matcher.is_match("world")
    assert not matcher.is_match("miss")

    results = matcher.process("hello world")
    ids = sorted(r.word_id for r in results)
    assert ids == [1, 2]


def test_from_dict_empty():
    with pytest.raises(ValueError):
        SimpleMatcher.from_dict({})


def test_from_dict_invalid():
    with pytest.raises((ValueError, TypeError)):
        SimpleMatcher.from_dict("not a dict")  # ty: ignore[invalid-argument-type]


def test_stats():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello", 2: "world"}})
    s = matcher.stats()
    assert s["rule_count"] == 2
    assert ProcessType.NONE in s["process_types"]


def test_stats_general():
    matcher = SimpleMatcher.from_dict(
        {
            ProcessType.NONE: {1: "hello"},
            ProcessType.DELETE: {2: "world"},
        }
    )
    s = matcher.stats()
    assert s["rule_count"] == 2


def test_pickle_roundtrip():
    import pickle

    original = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello"}})
    restored = pickle.loads(pickle.dumps(original))
    assert restored.is_match("hello")
    assert not restored.is_match("miss")


def test_repr():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello"}})
    r = repr(matcher)
    assert "SimpleMatcher" in r


def test_find_match_found():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello", 2: "world"}})
    result = matcher.find_match("hello world")
    assert result is not None
    assert result.word_id in (1, 2)


def test_find_match_none():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello"}})
    assert matcher.find_match("goodbye") is None
    assert matcher.find_match("") is None


def test_find_match_general():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "a&b"}})
    result = matcher.find_match("a and b")
    assert result is not None
    assert result.word_id == 1
    assert result.word == "a&b"
    assert matcher.find_match("a only") is None


def test_batch_find_match():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello", 2: "world"}})
    results = matcher.batch_find_match(["hello", "miss", "world", ""])
    assert len(results) == 4
    assert results[0] is not None and results[0].word_id == 1
    assert results[1] is None
    assert results[2] is not None and results[2].word_id == 2
    assert results[3] is None


def test_batch_find_match_empty():
    matcher = SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello"}})
    assert matcher.batch_find_match([]) == []


def test_threading():
    import concurrent.futures

    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.VARIANT_NORM: {1: "测试"}}).encode()
    )
    texts = ["測試测试文本" * 100] * 200

    def run_serial():
        return [simple_matcher.is_match(t) for t in texts]

    def run_threaded():
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            return list(pool.map(simple_matcher.is_match, texts))

    serial = run_serial()
    threaded = run_threaded()

    assert serial == threaded
    assert all(serial)
