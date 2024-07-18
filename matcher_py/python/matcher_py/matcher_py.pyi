from typing import Dict, List
from .extension_types import SimpleResult, MatchResult

def text_process(process_type: int, text: str) -> str:
    """
    Processes the given text based on the specified process type.

    Parameters:
    - process_type (int): An integer indicating the type of process to be applied to the text.
    - text (str): The text string that is to be processed.

    Returns:
    - str: The text string after processing.
    """
    ...

def reduce_text_process(process_type: int, text: str) -> List[str]:
    """
    Reduces the given text based on the specified process type and returns a list of strings.

    Parameters:
    - process_type (int): An integer indicating the type of process to be applied to the text.
    - text (str): The text string that is to be reduced.

    Returns:
    - List[str]: A list of strings after the reduction process.
    """
    ...

class Matcher:
    """
    A class used to perform various matching operations using a given set of match table map bytes.

    Methods:
    - __init__(self, match_table_map_bytes: bytes) -> None:
        Initializes the Matcher with the provided match table map bytes.
    - __getnewargs__(self) -> bytes:
        Returns the arguments necessary to create a new instance of the Matcher.
    - __getstate__(self) -> bytes:
        Returns the state of the Matcher, typically used for pickling.
    - __setstate__(self, match_table_map_bytes: bytes):
        Sets the state of the Matcher from the provided match table map bytes, typically used for unpickling.
    - is_match(self, text: str) -> bool:
        Checks whether the given text matches any patterns in the match table map.
    - process(self, text: str) -> List[MatchResult]:
        Processes the given text and returns a list of MatchResult objects corresponding to the matches found.
    - word_match(self, text: str) -> Dict[int, List[MatchResult]]:
        Performs a word-level match on the given text and returns a dictionary where the keys are word indices and the values are lists of MatchResult objects.
    - word_match_as_string(self, text: str) -> str:
        Performs a word-level match on the given text and returns a string representation of the matches found.
    """
    def __init__(self, match_table_map_bytes: bytes) -> None: ...
    def __getnewargs__(self) -> bytes: ...
    def __getstate__(self) -> bytes: ...
    def __setstate__(self, match_table_map_bytes: bytes): ...
    def is_match(self, text: str) -> bool: ...
    def process(self, text: str) -> List[MatchResult]: ...
    def word_match(self, text: str) -> Dict[int, List[MatchResult]]: ...
    def word_match_as_string(self, text: str) -> str: ...

class SimpleMatcher:
    """
    A class used to perform simplified matching operations using a provided set of simple table bytes.

    Methods:
    - __init__(self, simple_table_bytes: bytes) -> None:
        Initializes the SimpleMatcher with the provided simple table bytes.
    - __getnewargs__(self) -> bytes:
        Returns the arguments necessary to create a new instance of the SimpleMatcher.
    - __getstate__(self) -> bytes:
        Returns the state of the SimpleMatcher, typically used for pickling.
    - __setstate__(self, simple_table_bytes: bytes):
        Sets the state of the SimpleMatcher from the provided simple table bytes, typically used for unpickling.
    - is_match(self, text: str) -> bool:
        Checks whether the given text matches any patterns in the simple table.
    - process(self, text: str) -> List[SimpleResult]:
        Processes the given text and returns a list of SimpleResult objects corresponding to the matches found.
    """
    def __init__(self, simple_table_bytes: bytes) -> None: ...
    def __getnewargs__(self) -> bytes: ...
    def __getstate__(self) -> bytes: ...
    def __setstate__(self, simple_table_bytes: bytes): ...
    def is_match(self, text: str) -> bool: ...
    def process(self, text: str) -> List[SimpleResult]: ...
