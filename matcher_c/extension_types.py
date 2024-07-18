from enum import Enum, IntFlag
from typing import Dict, List, TypedDict


class ProcessType(IntFlag):
    MatchNone = 0b00000001
    MatchFanjian = 0b00000010
    MatchDelete = 0b00000100
    MatchNormalize = 0b00001000
    MatchDeleteNormalize = 0b00001100
    MatchFanjianDeleteNormalize = 0b00001110
    MatchPinYin = 0b00010000
    MatchPinYinChar = 0b00100000


class RegexMatchType(Enum):
    MatchSimilarChar = "similar_char"
    MatchAcrostic = "acrostic"
    MatchRegex = "regex"


class SimMatchType(Enum):
    MatchLevenshtein = "levenshtein"


class Simple(TypedDict):
    process_type: ProcessType


class Regex(TypedDict):
    process_type: ProcessType
    regex_match_type: RegexMatchType


class Similar(TypedDict):
    process_type: ProcessType
    sim_match_type: SimMatchType
    threshold: float


class MatchTableType:
    def Simple(process_type: ProcessType) -> Dict[str, Simple]:
        return {"simple": Simple(process_type=process_type)}

    def Regex(
        process_type: ProcessType, regex_match_type: RegexMatchType
    ) -> Dict[str, Regex]:
        return {
            "regex": Regex(process_type=process_type, regex_match_type=regex_match_type)
        }

    def Similar(
        process_type: ProcessType, sim_match_type: SimMatchType, threshold: float
    ) -> Dict[str, Similar]:
        return {
            "similar": Similar(
                process_type=process_type,
                sim_match_type=sim_match_type,
                threshold=threshold,
            )
        }


class MatchTable(TypedDict):
    table_id: int
    match_table_type: MatchTableType
    word_list: List[str]
    exemption_process_type: ProcessType
    exemption_word_list: List[str]


MatchTableMap = Dict[int, List[MatchTable]]


class MatchResult(TypedDict):
    table_id: int
    word: str


SimpleTable = Dict[ProcessType, Dict[int, str]]


class SimpleResult(TypedDict):
    word_id: int
    word: str
