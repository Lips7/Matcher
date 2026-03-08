import pytest

from matcher_py import ProcessType, reduce_text_process, text_process


def test_text_process():
    # Test with valid int (bitflags)
    res_int = text_process(ProcessType.DELETE, "hello word")
    assert isinstance(res_int, str)

    # Test with combined process types (should fail since text_process only accepts a single bit)
    combined = ProcessType.DELETE | ProcessType.NORMALIZE
    with pytest.raises(ValueError):
        text_process(combined, "hello word")


def test_reduce_text_process():
    res1 = reduce_text_process(ProcessType.FANJIAN | ProcessType.PINYIN, "测试")
    assert isinstance(res1, list)
    assert len(res1) > 0

    res2 = reduce_text_process(ProcessType.PINYIN_CHAR, "测试")
    assert isinstance(res2, list)
    assert len(res2) > 0


def test_invalid_type_raises_typeerror():
    with pytest.raises(TypeError):
        text_process("invalid_type", "hello word")  # ty: ignore[invalid-argument-type]

    with pytest.raises(TypeError):
        reduce_text_process([], "测试")  # ty: ignore[invalid-argument-type]
