from typing import Dict, List, Optional, Tuple, TypedDict

import numpy as np

class SimpleResult(TypedDict):
    word_id: int
    word: str

class Matcher:
    def __init__(self, match_table_dict_bytes: bytes) -> None: ...
    def __getnewargs__(self) -> Tuple[bytes, str, str]: ...
    def __getstate__(self) -> Dict: ...
    def __setstate__(self, state_dict: Dict): ...
    def is_match(self, text: str) -> bool: ...
    def word_match(self, text: str) -> Dict[str, str]: ...
    def word_match_as_string(self, text: str) -> str: ...
    def batch_word_match_as_dict(
        self, text_array: List[str]
    ) -> List[Dict[str, str]]: ...
    def batch_word_match_as_string(self, text_array: List[str]) -> List[str]: ...
    def numpy_word_match_as_dict(
        self, text_array: np.ndarray, inplace=False
    ) -> Optional[np.ndarray]: ...
    def numpy_word_match_as_string(
        self, text_array: np.ndarray, inplace=False
    ) -> Optional[np.ndarray]: ...

class SimpleMatcher:
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
