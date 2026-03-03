import json
import pytest

from matcher_py.matcher_py import Matcher
from matcher_py.extension_types import (
    ProcessType,
    MatchTable,
    MatchTableType,
    RegexMatchType,
    SimMatchType,
)


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
    Matcher(json.dumps({}).encode())
    Matcher(json.dumps({1: []}).encode())
    Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            process_type=ProcessType.MatchNone
                        ),
                        word_list=[],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        ).encode()
    )


def test_init_with_invalid_map():
    with pytest.raises(ValueError):
        Matcher(json.dumps({"a": 1}).encode())
        Matcher(json.dumps({"a": {"b": 1}}).encode())
        Matcher(json.dumps({"c": {}}).encode())


def test_regex():
    matcher = Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            process_type=ProcessType.MatchNone,
                            regex_match_type=RegexMatchType.MatchRegex,
                        ),
                        word_list=["h[aeiou]llo", "w[aeiou]rd"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        ).encode()
    )
    assert matcher.is_match("hallo")
    assert matcher.is_match("ward")
    assert matcher.word_match("hallo")[1][0]["table_id"] == 1
    assert matcher.word_match("hallo")[1][0]["word"] == "h[aeiou]llo"


def test_similar_char():
    matcher = Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            process_type=ProcessType.MatchNone,
                            regex_match_type=RegexMatchType.MatchSimilarChar,
                        ),
                        word_list=["hello,hi,H,你好", "world,word,🌍,世界"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        ).encode()
    )
    assert matcher.is_match("helloworld")
    assert matcher.is_match("hi世界")
    assert matcher.word_match("helloworld")[1][0]["table_id"] == 1
    assert (
        matcher.word_match("helloworld")[1][0]["word"]
        == "(?:hello|hi|H|你好).?(?:world|word|🌍|世界)"
    )


def test_similar_text_levenshtein():
    matcher = Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Similar(
                            process_type=ProcessType.MatchNone,
                            sim_match_type=SimMatchType.MatchLevenshtein,
                            threshold=0.8,
                        ),
                        word_list=["helloworld"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        ).encode()
    )
    assert matcher.is_match("helloworl")
    assert matcher.is_match("halloworld")
    assert matcher.is_match("ha1loworld")
    assert not matcher.is_match("ha1loworld1")
    assert matcher.word_match("helloworl")[1][0]["table_id"] == 1
    assert matcher.word_match("helloworl")[1][0]["word"] == "helloworld"


def test_acrostic():
    matcher = Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Regex(
                            process_type=ProcessType.MatchNone,
                            regex_match_type=RegexMatchType.MatchAcrostic,
                        ),
                        word_list=["h,e,l,l,o", "你,好"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=[],
                    )
                ]
            }
        ).encode()
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
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            process_type=ProcessType.MatchNone
                        ),
                        word_list=["helloworld"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=["worldwide"],
                    )
                ]
            }
        ).encode()
    )
    assert matcher.is_match("helloworld")
    assert not matcher.is_match("helloworldwide")

    matcher = Matcher(
        json.dumps(
            {
                1: [
                    MatchTable(
                        table_id=1,
                        match_table_type=MatchTableType.Simple(
                            process_type=ProcessType.MatchNone
                        ),
                        word_list=["helloworld"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=["worldwide"],
                    ),
                    MatchTable(
                        table_id=2,
                        match_table_type=MatchTableType.Regex(
                            process_type=ProcessType.MatchNone,
                            regex_match_type=RegexMatchType.MatchRegex,
                        ),
                        word_list=["hello"],
                        exemption_process_type=ProcessType.MatchNone,
                        exemption_word_list=["worldwide"],
                    ),
                ]
            }
        ).encode()
    )
    assert matcher.is_match("helloworld")
    assert not matcher.is_match("helloworldwide")
