# Design

## Matcher

### Overview

The `Matcher` is a powerful and complex system designed to identify sentence matches using multiple methods. Despite its complexity, it offers significant flexibility and power when used correctly. The main components of the `Matcher` are `MatchID` and `TableID`.

### Key Concepts

1. **MatchID**: Represents a unique identifier for a match.
2. **TableID**: Represents a unique identifier for a table within a match.

### Structure

The `Matcher` utilizes a JSON structure to define matches and tables. Below is an example of its configuration:

```json
{
    "777": [
        {
            "table_id": 45,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["hello", "world"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
        // other tables
    ]
    // other matches
}
```

- `777`: This is the `MatchID`.
- `45`: This is the `TableID`.

#### Table

Each `Table` represents a collection of words related to a specific topic (e.g., political, music, math). The table also includes a list of exemption words to exclude certain sentences. The logical operations within a table are as follows:

- **OR Logic (within `word_list`)**: The table matches if any word in the `word_list` is matched.
- **NOT Logic (between `word_list` and `exemption_word_list`)**: If any word in the `exemption_word_list` is matched, the table will not be considered as matched.

#### Match

A `Match` consists of multiple tables. Each match can specify a list of tables to perform the matching. This allows users to experiment with different combinations of tables to find the best configuration for their use case. The logical operation between matches is:

- **OR Logic (between matches)**: The result will report all the matches if any table inside the match is matched.

### Usage Cases

#### Table1 AND Table2 match
```json
Input:
{
    "1": [
        {
            "table_id": 1,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["hello", "world"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
    ],
    "2": [
        {
            "table_id": 2,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["你", "好"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
    ],
}

Output: Check if `match_id` 1 and 2 are both matched.
```

#### Table1 OR Table2 match
```json
Input:
{
    "1": [
        {
            "table_id": 1,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["hello", "world"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        },
        {
            "table_id": 2,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["你", "好"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
    ]
}

Output: Check if `match_id` 1 or 2 is matched.
```

#### Table1 NOT Table2 match
```json
Input:
{
    "1": [
        {
            "table_id": 1,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["hello", "world"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
    ],
    "2": [
        {
            "table_id": 2,
            "match_table_type": {"simple_match_type": "MatchNone"},
            "word_list": ["你", "好"],
            "exemption_simple_match_type": "MatchNone",
            "exemption_word_list": []
        }
    ],
}

Output: Check if `match_id` 1 is matched and 2 is not matched.
```

## SimpleMatcher

### Overview

The `SimpleMatcher` is the core component, designed to be fast, efficient, and easy to use. It handles large amounts of data and identifies words based on predefined types.

### Key Concepts

1. **WordID**: Represents a unique identifier for a word in the `SimpleMatcher`.

### Structure

The `SimpleMatcher` uses a mapping structure to define words and their IDs based on different match types. Below is an example configuration:

```json
{
    "SimpleMatchType.None": {
        "1": "hello,world",
        "2": "你好"
        // other words
    }
    // other simple match type word maps
}
```

- `1` and `2`: These are `WordID`s used to identify words in the `SimpleMatcher`.

### Real-world Application

In real-world scenarios, `word_id` is used to uniquely identify a word in the database, allowing for easy updates to the word and its variants.

### Logical Operations

- **OR Logic (between different `simple_match_type` and words in the same `simple_match_type`)**: The `simple_matcher` is considered matched if any word in the map is matched.
- **AND Logic (between words separated by `,` within a `WordID`)**: All words separated by `,` must be matched for the word to be considered as matched.

### Usage Cases

#### Word1 AND Word2 match
```json
Input:
{
    "SimpleMatchType.None": {
        "1": "word1,word2"
    }
}

Output: Check if `word_id` 1 is matched.
```

#### Word1 OR Word2 match
```json
Input:
{
    "SimpleMatchType.None": {
        "1": "word1",
        "2": "word2"
    }
}

Output: Check if `word_id` 1 or 2 is matched.
```

## Summary

The `Matcher` and `SimpleMatcher` systems are designed to provide a robust and flexible solution for word matching tasks. By understanding the logical operations and structures of `MatchID`, `TableID`, and `WordID`, users can effectively leverage these tools for complex matching requirements.