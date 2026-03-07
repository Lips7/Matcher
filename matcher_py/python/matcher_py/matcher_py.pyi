from .extension_types import SimpleResult

def text_process(process_type: int, text: str) -> str:
    """
    Processes the given text based on the specified process type.

    Parameters:
    - process_type (int): An integer indicating the type of process to be applied to the text.
    - text (str): The text string that is to be processed.

    Returns:
    - str: The text string after processing.
    """

def reduce_text_process(process_type: int, text: str) -> list[str]:
    """
    Reduces the given text based on the specified process type and returns a list of strings.

    Parameters:
    - process_type (int): An integer indicating the type of process to be applied to the text.
    - text (str): The text string that is to be reduced.

    Returns:
    - List[str]: A list of strings after the reduction process.
    """

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
    def process(self, text: str) -> list[SimpleResult]: ...
