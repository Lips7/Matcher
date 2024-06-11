from enum import Enum, IntFlag
from typing import Dict, List

import msgspec


class MatchTableType(Enum):
    Simple = "simple"
    SimilarChar = "similar_char"
    Acrostic = "acrostic"
    SimilarTextLevenshtein = "similar_text_levenshtein"
    Regex = "regex"


class SimpleMatchType(IntFlag):
    MatchNone = 0b00000001
    MatchFanjian = 0b00000010
    MatchDeleteNormalize = 0b00011100
    MatchPinYin = 0b00100000
    MatchPinYinChar = 0b01000000


class MatchTable(msgspec.Struct):
    table_id: int
    match_table_type: MatchTableType
    simple_match_type: SimpleMatchType
    word_list: List[str]
    exemption_simple_match_type: SimpleMatchType
    exemption_word_list: List[str]


MatchTableMap = Dict[str, MatchTable]


class MatchResult(msgspec.Struct):
    table_id: int
    word: str


MatcherMatchResult = Dict[str, List[MatchResult]]


class SimpleResult(msgspec.Struct):
    word_id: int
    word: str


SimpleMatchTypeWordMap = Dict[SimpleMatchType, Dict[int, str]]
