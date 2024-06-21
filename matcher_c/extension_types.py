from enum import Enum, IntFlag
from typing import Dict, List

import msgspec


class SimpleMatchType(IntFlag):
    """
    IntFlag representing different simple match types.

    Attributes:
        MatchNone (int): A match type indicating no specific match criteria (0b00000001).
        MatchFanjian (int): A match type for matching between traditional and simplified Chinese characters (0b00000010).
        MatchWordDelete (int): A match type where words are deleted for matching purposes (0b00000100).
        MatchTextDelete (int): A match type where text is deleted for matching purposes (0b00001000).
        MatchDelete (int): A combined match type where both word and text deletions are applied (0b00001100).
        MatchNormalize (int): A match type where text normalization is applied (0b00010000).
        MatchDeleteNormalize (int): A combined match type where deletion and normalization are both applied (0b00011100).
        MatchFanjianDeleteNormalize (int): A combined match type that includes Fanjian matching, deletion, and normalization (0b00011110).
        MatchPinYin (int): A match type using Pinyin for matching Chinese characters (0b00100000).
        MatchPinYinChar (int): A match type using individual Pinyin characters for a finer granularity match (0b01000000).
    """

    MatchNone = 0b00000001
    MatchFanjian = 0b00000010
    MatchWordDelete = 0b00000100
    MatchTextDelete = 0b00001000
    MatchDelete = 0b00001100
    MatchNormalize = 0b00010000
    MatchDeleteNormalize = 0b00011100
    MatchFanjianDeleteNormalize = 0b00011110
    MatchPinYin = 0b00100000
    MatchPinYinChar = 0b01000000


class RegexMatchType(Enum):
    """
    Enum representing different regex match types.

    Attributes:
        MatchSimilarChar (str): A match type that finds characters similar to a given set ("similar_char").
        MatchAcrostic (str): A match type that looks for acrostic patterns ("acrostic").
        MatchRegex (str): A match type that uses regular expressions for matching ("regex").
    """

    MatchSimilarChar = "similar_char"
    MatchAcrostic = "acrostic"
    MatchRegex = "regex"


class SimMatchType(Enum):
    """
    Enum representing different similarity match types.

    Attributes:
        MatchLevenshtein (str): A match type using the Levenshtein distance algorithm for measuring the difference between two sequences ("levenshtein").
        MatchDamrauLevenshtein (str): A match type using the Damerau-Levenshtein distance algorithm, an extension of Levenshtein with transpositions allowed ("damrau_levenshtein").
        MatchIndel (str): A match type that uses insertion and deletion operations for matching purposes ("indel").
        MatchJaro (str): A match type using the Jaro distance algorithm to compare the similarity between two strings ("jaro").
        MatchJaroWinkler (str): A match type using the Jaro-Winkler distance algorithm, an extension of Jaro with added weight for matching starting characters ("jaro_winkler").
    """

    MatchLevenshtein = "levenshtein"
    MatchDamrauLevenshtein = "damrau_levenshtein"
    MatchIndel = "indel"
    MatchJaro = "jaro"
    MatchJaroWinkler = "jaro_winkler"


class Simple(msgspec.Struct):
    """
    Represents a simple match configuration.

    Attributes:
        simple_match_type (SimpleMatchType): The type of simple match to be used, as defined in SimpleMatchType.
    """

    simple_match_type: SimpleMatchType


class Regex(msgspec.Struct):
    """
    Represents a regular expression match configuration.

    Attributes:
        regex_match_type (RegexMatchType): The type of regular expression match to be used, as defined in RegexMatchType.
    """

    regex_match_type: RegexMatchType


class Similar(msgspec.Struct):
    """
    Represents a similarity match configuration.

    Attributes:
        sim_match_type (SimMatchType): The type of similarity match to be used, as defined in SimMatchType.
        threshold (float): The threshold value for the similarity match. This value determines the minimum similarity score required for a match to be considered successful.
    """

    sim_match_type: SimMatchType
    threshold: float


class MatchTableType:
    """
    A class representing different types of match tables.

    Attributes:
        Simple (Simple): Represents a simple match configuration.
        Regex (Regex): Represents a regular expression match configuration.
        Similar (Similar): Represents a similarity match configuration.
    """

    Simple = Simple
    Regex = Regex
    Similar = Similar


class MatchTable(msgspec.Struct):
    """
    Represents a match table configuration with various match types.

    Attributes:
        table_id (int): The unique identifier for the match table.
        match_table_type (MatchTableType): The type of match table, can be one of Simple, Regex, or Similar as defined in MatchTableType.
        word_list (List[str]): A list of words to be used for matching.
        exemption_simple_match_type (SimpleMatchType): Specifies which simple match type(s) to exempt from the match operation.
        exemption_word_list (List[str]): A list of words to exempt from the match operation.
    """

    table_id: int
    match_table_type: MatchTableType
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
