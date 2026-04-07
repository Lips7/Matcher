class ProcessType:
    """
    An enumeration representing various types of text processing operations.

    Flags compose with ``|``. For example, ``ProcessType.DELETE | ProcessType.NORMALIZE``
    is equivalent to ``ProcessType.DELETE_NORMALIZE``.
    """

    NONE: int
    VARIANT_NORM: int
    DELETE: int
    NORMALIZE: int
    DELETE_NORMALIZE: int
    VARIANT_NORM_DELETE_NORMALIZE: int
    ROMANIZE: int
    ROMANIZE_CHAR: int
    EMOJI_NORM: int

class SimpleResult:
    """
    A match result returned by :meth:`SimpleMatcher.process`.

    Attributes:
        word_id (int): The identifier of the word within the word list.
        word (str): The word corresponding to the word_id.
    """

    word_id: int
    word: str

def text_process(process_type: int | ProcessType, text: str) -> str:
    """
    Apply all transformations in *process_type* to *text* and return the
    final transformed string.

    Parameters:
        process_type: Which transformations to apply (e.g. ``ProcessType.DELETE``).
        text: The input text.

    Returns:
        The fully transformed text.
    """

def reduce_text_process(process_type: int | ProcessType, text: str) -> list[str]:
    """
    Apply transformations in *process_type* incrementally and return every
    intermediate variant (one per transform step).

    Parameters:
        process_type: Which transformations to apply.
        text: The input text.

    Returns:
        A list of strings, one for each intermediate transformation stage.
    """

class SimpleMatcher:
    """
    High-performance multi-pattern matcher with logical operators and text
    normalization.

    Construct from a JSON-encoded ``SimpleTable`` mapping
    ``{ProcessType: {word_id: pattern_string}}``.  Once built, matching
    methods are infallible and thread-safe.
    """

    def __init__(self, simple_table_bytes: bytes) -> None:
        """
        Build a matcher from a JSON-encoded ``SimpleTable``.

        Parameters:
            simple_table_bytes: UTF-8 JSON bytes of the form
                ``{ProcessType: {word_id: pattern}}``.

        Raises:
            ValueError: If the JSON is malformed or contains invalid
                ``ProcessType`` values.
        """

    def __getnewargs__(self) -> tuple[bytes]:
        """Return constructor args for pickle support."""

    def __getstate__(self) -> bytes:
        """Serialize matcher state for pickling."""

    def __setstate__(self, simple_table_bytes: bytes) -> None:
        """Restore matcher state from pickle data."""

    def __repr__(self) -> str:
        """Return a summary string showing search mode and rule count."""

    def is_match(self, text: str) -> bool:
        """
        Return ``True`` if *text* matches any pattern in the matcher.

        This is the fastest check — use it when you only need a boolean answer.
        """

    def process(self, text: str) -> list[SimpleResult]:
        """
        Return all patterns that match *text*.

        Each :class:`SimpleResult` contains the ``word_id`` and ``word``
        of a matched pattern. Results are deduplicated but unordered.
        """

    def batch_is_match(self, texts: list[str]) -> list[bool]:
        """
        Check multiple texts in one call. Releases the GIL internally.

        Returns a list of booleans, one per input text.
        """

    def batch_process(self, texts: list[str]) -> list[list[SimpleResult]]:
        """
        Process multiple texts in one call. Releases the GIL internally.

        Returns a list of result lists, one per input text.
        """
