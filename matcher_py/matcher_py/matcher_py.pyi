from typing import Dict, List, Optional, Tuple, TypedDict

import numpy as np

class SimpleResult(TypedDict):
    word_id: int
    word: str

class MatchResult(TypedDict):
    table_id: int
    word: str

class Matcher:
    """
    Matcher class is designed to perform various types of text matching
    and processing operations on given text inputs. It offers functionalities
    for detecting matches, processing single text inputs, and batch processing
    multiple text inputs both in list and NumPy array formats.

    Methods:
        __init__(self, match_table_map_bytes: bytes) -> None:
            Initializes the Matcher instance with the provided byte array representing
            the match table map.
        __getnewargs__(self) -> Tuple[bytes, str, str]:
            Retrieves the arguments necessary to create a new instance of Matcher.
        __getstate__(self) -> Dict:
            Gets the state of the Matcher instance as a dictionary.
        __setstate__(self, state_dict: Dict):
            Sets the state of the Matcher instance from the provided state dictionary.
        is_match(self, text: str) -> bool:
            Checks if the provided text matches according to the match table.
        word_match_raw(self, text: str) -> Dict[int, List[MatchResult]]:
            Matches words in the provided text and returns a dictionary representation.
        word_match(self, text: str) -> Dict[int, str]:
            Matches words in the provided text and returns a dictionary representation.
        word_match_as_string(self, text: str) -> str:
            Matches words in the provided text and returns a string representation.
        batch_word_match_as_dict(self, text_array: List[str]) -> List[Dict[int, str]]:
            Processes a list of texts, matching words in each text and returning a list
            of dictionaries representing the matches.
        batch_word_match_as_string(self, text_array: List[str]) -> List[str]:
            Processes a list of texts, matching words in each text and returning a list
            of strings representing the matches.
        numpy_word_match_as_dict(self, text_array: np.ndarray, inplace=False) -> Optional[np.ndarray]:
            Processes a NumPy array of texts, matching words in each text and returning a
            NumPy array of dictionaries representing the matches. If inplace is True,
            the operation is performed in-place.
        numpy_word_match_as_string(self, text_array: np.ndarray, inplace=False) -> Optional[np.ndarray]:
            Processes a NumPy array of texts, matching words in each text and returning a
            NumPy array of strings representing the matches. If inplace is True,
            the operation is performed in-place.
    """
    def __init__(self, match_table_map_bytes: bytes) -> None: ...
    def __getnewargs__(self) -> Tuple[bytes, str, str]: ...
    def __getstate__(self) -> Dict: ...
    def __setstate__(self, state_dict: Dict): ...
    def is_match(self, text: str) -> bool: ...
    def word_match_raw(self, text: str) -> Dict[int, List[MatchResult]]: ...
    def word_match(self, text: str) -> Dict[int, str]: ...
    def word_match_as_string(self, text: str) -> str: ...
    def batch_word_match_as_dict(
        self, text_array: List[str]
    ) -> List[Dict[int, str]]: ...
    def batch_word_match_as_string(self, text_array: List[str]) -> List[str]: ...
    def numpy_word_match_as_dict(
        self, text_array: np.ndarray, inplace=False
    ) -> Optional[np.ndarray]: ...

class SimpleMatcher:
    """
    SimpleMatcher class is designed to perform basic text matching and processing
    operations using a provided word list dictionary in byte format. It offers functionalities
    for detecting matches, processing single text inputs, and batch processing multiple text
    inputs both in list and NumPy array formats.

    Methods:
        __init__(self, simple_wordlist_dict_bytes: bytes) -> None:
            Initializes the SimpleMatcher instance with the provided byte array representing
            the simple word list dictionary.
        __getnewargs__(self) -> bytes:
            Retrieves the arguments necessary to create a new instance of SimpleMatcher.
        __getstate__(self) -> bytes:
            Gets the state of the SimpleMatcher instance as a byte array.
        __setstate__(self, simple_wordlist_dict_bytes: bytes):
            Sets the state of the SimpleMatcher instance from the provided byte array
            representing the word list dictionary.
        is_match(self, text: str) -> bool:
            Checks if the provided text matches any word in the simple word list.
        simple_process(self, text: str) -> List[SimpleResult]:
            Processes the provided text, matching words and returning a list of SimpleResult
            dictionaries representing the matches.
        batch_simple_process(self, text_array: List[str]) -> List[List[SimpleResult]]:
            Processes a list of texts, matching words in each text and returning a list of lists
            of SimpleResult dictionaries representing the matches.
        numpy_simple_process(self, text_array: np.ndarray, inplace=False) -> Optional[np.ndarray]:
            Processes a NumPy array of texts, matching words in each text and returning a NumPy
            array of lists of SimpleResult dictionaries representing the matches. If inplace is True,
            the operation is performed in-place.
    """
    def __init__(self, simple_wordlist_dict_bytes: bytes) -> None: ...
    def __getnewargs__(self) -> bytes: ...
    def __getstate__(self) -> bytes: ...
    def __setstate__(self, simple_wordlist_dict_bytes: bytes): ...
    def is_match(self, text: str) -> bool: ...
    def simple_process(self, text: str) -> List[SimpleResult]: ...
    def batch_simple_process(
        self, text_array: List[str]
    ) -> List[List[SimpleResult]]: ...
    def numpy_simple_process(
        self, text_array: np.ndarray, inplace=False
    ) -> Optional[np.ndarray]: ...
