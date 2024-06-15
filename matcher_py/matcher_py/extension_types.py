from enum import Enum, IntFlag
from typing import Dict, List

import msgspec


class MatchTableType(Enum):
    """
    Enum representing different types of match tables.

    Attributes:
        Simple: Represents a simple match type.
        SimilarChar: Represents a match type where similar characters are matched.
        Acrostic: Represents a match type based on acrostics.
        SimilarTextLevenshtein: Represents a match type using the Levenshtein distance algorithm to find similar text.
        Regex: Represents a match type using regular expressions.
    """

    Simple = "simple"
    SimilarChar = "similar_char"
    Acrostic = "acrostic"
    SimilarTextLevenshtein = "similar_text_levenshtein"
    Regex = "regex"


class SimpleMatchType(IntFlag):
    """
    IntFlag representing different types of simple matches.

    Attributes:
        None: No transformations applied.
        Fanjian: Match simplified and traditional Chinese characters.
        Delete: Match with deletion of certain characters.
        Normalize: Match with normalization of certain characters.
        DeleteNormalize: Match with normalization and deletion of certain characters.
        FanjianDeleteNormalize: Match both simplified and traditional Chinese characters, with normalization and deletion of certain characters.
        PinYin: Match using Pinyin, the Romanization of Chinese characters, considering character boundaries.
        PinYinChar: Match using Pinyin, the Romanization of Chinese characters, without considering character boundaries.
    """

    MatchNone = 0b00000001
    MatchFanjian = 0b00000010
    MatchDelete = 0b00001100
    MatchNormalize = 0b00010000
    MatchDeleteNormalize = 0b00011100
    MatchFanjianDeleteNormalize = 0b00011110
    MatchPinYin = 0b00100000
    MatchPinYinChar = 0b01000000


class MatchTable(msgspec.Struct):
    """
    Data structure for representing a match table.

    Attributes:
        table_id (int): Unique identifier for the match table.
        match_table_type (MatchTableType): Type of matching applied in the table.
        simple_match_type (SimpleMatchType): Specific simple match criteria used.
        word_list (List[str]): List of words that the matching operates against.
        exemption_simple_match_type (SimpleMatchType): Simple match criteria to be exempted.
        exemption_word_list (List[str]): List of words that are exempted from matching.
    """

    table_id: int
    match_table_type: MatchTableType
    simple_match_type: SimpleMatchType
    word_list: List[str]
    exemption_simple_match_type: SimpleMatchType
    exemption_word_list: List[str]


MatchTableMap = Dict[int, List[MatchTable]]


class MatchResult(msgspec.Struct):
    table_id: int
    word: str


MatcherMatchResult = Dict[str, List[MatchResult]]


class SimpleResult(msgspec.Struct):
    word_id: int
    word: str


SimpleMatchTypeWordMap = Dict[SimpleMatchType, Dict[int, str]]
