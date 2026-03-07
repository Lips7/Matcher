from enum import IntFlag
from typing import TypedDict


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


SimpleTable = dict[ProcessType, dict[int, str]]
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
