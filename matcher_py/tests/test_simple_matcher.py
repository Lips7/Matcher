import json

import pytest

from matcher_py import ProcessType, SimpleMatcher


def test_init_with_non_bytes():
    with pytest.raises(TypeError):
        SimpleMatcher(1)  # ty: ignore[invalid-argument-type, unused-ignore-comment]
    with pytest.raises(TypeError):
        SimpleMatcher("")  # ty: ignore[invalid-argument-type, unused-ignore-comment]
    with pytest.raises(TypeError):
        SimpleMatcher([])  # ty: ignore[invalid-argument-type, unused-ignore-comment]
    with pytest.raises(TypeError):
        SimpleMatcher({})  # ty: ignore[invalid-argument-type, unused-ignore-comment]


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
    SimpleMatcher(json.dumps({}).encode())
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


def test_fanjian():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.FANJIAN: {1: "你好"}}).encode()
    )
    assert simple_matcher.is_match("妳好")
    assert simple_matcher.process("你好")[0].word_id == 1
    assert simple_matcher.process("你好")[0].word == "你好"

    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.FANJIAN: {1: "妳好"}}).encode()
    )
    assert simple_matcher.is_match("你好")
    assert simple_matcher.process("你好")[0].word_id == 1
    assert simple_matcher.process("你好")[0].word == "妳好"


def test_delete():
    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.DELETE: {1: "你好"}}).encode()
    )
    assert simple_matcher.is_match("你！好")
    assert len(simple_matcher.process("你！好")) == 1


def test_normalize():
    simple_matcher = SimpleMatcher(
        json.dumps({
            ProcessType.NORMALIZE: {
                1: "he11o",
            }
        }).encode()
    )
    assert simple_matcher.is_match("ℋЀ⒈㈠Õ")
    assert simple_matcher.process("ℋЀ⒈㈠Õ")[0].word_id == 1
    assert simple_matcher.process("ℋЀ⒈㈠Õ")[0].word == "he11o"


def test_pinyin():
    simple_matcher = SimpleMatcher(
        json.dumps({
            ProcessType.PINYIN: {
                1: "西安",
            }
        }).encode()
    )
    assert simple_matcher.is_match("洗按")
    assert not simple_matcher.is_match("现")


def test_pinyinchar():
    simple_matcher = SimpleMatcher(
        json.dumps({
            ProcessType.PINYIN_CHAR: {
                1: "西安",
            }
        }).encode()
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


def test_threading():
    import concurrent.futures

    simple_matcher = SimpleMatcher(
        json.dumps({ProcessType.FANJIAN: {1: "你好"}}).encode()
    )
    texts = ["妳好测试文本" * 100] * 200

    def run_serial():
        return [simple_matcher.is_match(t) for t in texts]

    def run_threaded():
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            return list(pool.map(simple_matcher.is_match, texts))

    serial = run_serial()
    threaded = run_threaded()

    assert serial == threaded
    assert all(serial)
