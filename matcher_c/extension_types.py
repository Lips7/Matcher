from enum import Enum, IntFlag
from typing import Dict, List, TypedDict, Union


class ProcessType(IntFlag):
    """
    An enumeration representing various types of text processing operations.

    Attributes:
        MatchNone (IntFlag): An operation that performs no matching (binary 00000001).
        MatchFanjian (IntFlag): An operation that matches traditional and simplified Chinese characters (binary 00000010).
        MatchDelete (IntFlag): An operation that matches deleted characters (binary 00000100).
        MatchNormalize (IntFlag): An operation that normalizes characters (binary 00001000).
        MatchDeleteNormalize (IntFlag): A combined operation that deletes and normalizes characters (binary 00001100).
        MatchFanjianDeleteNormalize (IntFlag): A combined operation that matches traditional and simplified Chinese characters,
                                              deletes, and normalizes (binary 00001110).
        MatchPinYin (IntFlag): An operation that matches Pinyin representations of Chinese characters (binary 00010000).
        MatchPinYinChar (IntFlag): An operation that matches individual characters in the Pinyin representation (binary 00100000).
    """

    MatchNone = 0b00000001
    MatchFanjian = 0b00000010
    MatchDelete = 0b00000100
    MatchNormalize = 0b00001000
    MatchDeleteNormalize = 0b00001100
    MatchFanjianDeleteNormalize = 0b00001110
    MatchPinYin = 0b00010000
    MatchPinYinChar = 0b00100000


class RegexMatchType(str, Enum):
    """
    An enumeration representing various types of regex matching operations.

    Attributes:
        MatchSimilarChar (str): An operation that matches characters that are similar in some way.
        MatchAcrostic (str): An operation that matches acrostic patterns.
        MatchRegex (str): An operation that matches using standard regular expressions.
    """

    MatchSimilarChar = "similar_char"
    MatchAcrostic = "acrostic"
    MatchRegex = "regex"


class SimMatchType(str, Enum):
    """
    An enumeration representing various types of similarity matching operations.

    Attributes:
        MatchLevenshtein (str): An operation that matches using the Levenshtein distance metric.
    """

    MatchLevenshtein = "levenshtein"


class Simple(TypedDict):
    """
    A TypedDict representing a simple text processing operation.

    Attributes:
        process_type (ProcessType): The type of processing operation to be performed.
    """

    process_type: ProcessType


class Regex(TypedDict):
    """
    A TypedDict representing a regex-based text processing operation.

    Attributes:
        process_type (ProcessType): The type of processing operation to be performed.
        regex_match_type (RegexMatchType): The type of regex matching operation to be used.
    """

    process_type: ProcessType
    regex_match_type: RegexMatchType


class Similar(TypedDict):
    """
    A TypedDict representing a similarity-based text processing operation.

    Attributes:
        process_type (ProcessType): The type of processing operation to be performed.
        sim_match_type (SimMatchType): The type of similarity matching operation to be used.
        threshold (float): The threshold value for the similarity matching operation.
    """

    process_type: ProcessType
    sim_match_type: SimMatchType
    threshold: float


class MatchTableType:
    def Simple(process_type: ProcessType) -> Dict[str, Simple]:
        """
        Create a dictionary representing a simple text processing operation.

        Args:
            process_type (ProcessType): The type of processing operation to be performed.

        Returns:
            Dict[str, Simple]: A dictionary with one key "simple" mapping to a Simple TypedDict
                               containing the provided process_type.
        """
        return {"simple": Simple(process_type=process_type)}

    def Regex(
        process_type: ProcessType, regex_match_type: RegexMatchType
    ) -> Dict[str, Regex]:
        """
        Create a dictionary representing a regex-based text processing operation.

        Args:
            process_type (ProcessType): The type of processing operation to be performed.
            regex_match_type (RegexMatchType): The type of regex matching operation to be used.

        Returns:
            Dict[str, Regex]: A dictionary with one key "regex" mapping to a Regex TypedDict
                              containing the provided process_type and regex_match_type.
        """
        return {
            "regex": Regex(process_type=process_type, regex_match_type=regex_match_type)
        }

    def Similar(
        process_type: ProcessType, sim_match_type: SimMatchType, threshold: float
    ) -> Dict[str, Similar]:
        """
        Create a dictionary representing a similarity-based text processing operation.
        Args:
            process_type (ProcessType): The type of processing operation to be performed.
            sim_match_type (SimMatchType): The type of similarity matching operation to be used.
            threshold (float): The threshold value for the similarity matching operation.

        Returns:
            Dict[str, Similar]: A dictionary with one key "similar" mapping to a Similar TypedDict
                                containing the provided process_type, sim_match_type, and threshold.
        """
        return {
            "similar": Similar(
                process_type=process_type,
                sim_match_type=sim_match_type,
                threshold=threshold,
            )
        }


class MatchTable(TypedDict):
    """
    A TypedDict representing a table for matching operations.

    Attributes:
        table_id (int): A unique identifier for the match table.
        match_table_type (Union[Dict[str, Simple], Dict[str, Regex], Dict[str, Similar]]):
            A dictionary that specifies the type of match operation to be performed. The key is a string indicating
            the match type ('simple', 'regex', 'similar'), and the value is a corresponding TypedDict describing
            the operation.
        word_list (List[str]): A list of words that are subject to the matching operations.
        exemption_process_type (ProcessType): The type of process for which certain words are exempt from matching.
        exemption_word_list (List[str]): A list of words that are exempt from the matching process.
    """

    table_id: int
    match_table_type: Union[Dict[str, Simple], Dict[str, Regex], Dict[str, Similar]]
    word_list: List[str]
    exemption_process_type: ProcessType
    exemption_word_list: List[str]


MatchTableMap = Dict[int, List[MatchTable]]
"""
A type alias for mapping table identifiers to lists of MatchTable objects.

Type:
    Dict[int, List[MatchTable]]

This dictionary maps an integer table ID to a list of MatchTable objects that correspond to the ID. It is used to
organize and retrieve match tables based on their unique identifiers.
"""


class MatchResult(TypedDict):
    """
    A TypedDict representing the result of a matching operation.

    Attributes:
        match_id (int): A unique identifier for the match result.
        table_id (int): The identifier of the match table where the matching operation was performed.
        word_id (int): The identifier of the matched word within the word list.
        word (str): The matched word.
        similarity (float): The similarity score of the match operation.
    """

    match_id: int
    table_id: int
    word_id: int
    word: str
    similarity: float


SimpleTable = Dict[ProcessType, Dict[int, str]]
"""
A type alias for representing a simple table structure for text processing.

This dictionary maps a `ProcessType` to another dictionary that maps an integer ID to a string.
The outer dictionary's keys represent different types of processing operations, while the inner
dictionary's keys represent unique identifiers corresponding to specific strings related to the
operations.

Type:
    Dict[ProcessType, Dict[int, str]]
"""


class SimpleResult(TypedDict):
    """
    A TypedDict representing a simplified result of a text processing operation.

    Attributes:
        word_id (int): The identifier of the word within the word list.
        word (str): The word corresponding to the word_id.
    """

    word_id: int
    word: str
