# Matcher C FFI Bindings

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

C FFI bindings for the [Matcher](https://github.com/Lips7/Matcher) library — a high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Installation

### Build from source

Requires the Rust **nightly** toolchain.

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
just build
```

`just build` compiles all packages and copies the dynamic library into the `matcher_c/` directory automatically. You can also build manually with `cargo build --release` — the library will be in `target/release/`:
- Linux: `libmatcher_c.so`
- macOS: `libmatcher_c.dylib`
- Windows: `matcher_c.dll`

## API

The full API is declared in [`matcher_c.h`](./matcher_c.h).

### ProcessType Bit Flags

| Define | Value | Description |
|--------|-------|-------------|
| `PROCESS_TYPE_NONE` | 1 | No transformation; match raw input |
| `PROCESS_TYPE_VARIANT_NORM` | 2 | CJK variant normalization |
| `PROCESS_TYPE_DELETE` | 4 | Remove symbols, punctuation, whitespace |
| `PROCESS_TYPE_NORMALIZE` | 8 | Normalize character variants to basic forms |
| `PROCESS_TYPE_DELETE_NORMALIZE` | 12 | Delete + Normalize combined |
| `PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE` | 14 | VariantNorm + Delete + Normalize combined |
| `PROCESS_TYPE_ROMANIZE` | 16 | CJK characters to space-separated romanization (Pinyin, Romaji, RR) |
| `PROCESS_TYPE_ROMANIZE_CHAR` | 32 | CJK characters to romanization without boundary spaces |
| `PROCESS_TYPE_EMOJI_NORM` | 64 | Emoji → English words (CLDR short names), strips modifiers |

### Result Types

```c
// A single match result. word is an owned null-terminated UTF-8 string.
typedef struct {
    uint32_t word_id;
    char* word;
} SimpleResult;

// A list of match results.
typedef struct {
    size_t len;
    SimpleResult* items;
} SimpleResultList;
```

### SimpleMatcher

```c
// Initialize from JSON config string. Returns NULL on error.
void* init_simple_matcher(const char* simple_table_bytes);

// Check if text matches any pattern.
bool simple_matcher_is_match(const void* simple_matcher, const char* text);

// Get all matches as a SimpleResultList. Caller must free with drop_simple_result_list().
SimpleResultList* simple_matcher_process(const void* simple_matcher, const char* text);

// Get the first match. Returns NULL if no match. Caller must free with drop_simple_result().
SimpleResult* simple_matcher_find_match(const void* simple_matcher, const char* text);

// Free a SimpleMatcher instance.
void drop_simple_matcher(void* simple_matcher);
```

### Text Processing

```c
// Apply a ProcessType transformation to text. Caller must free with drop_string().
char* text_process(ProcessType process_type, const char* text);

// Apply all intermediate transformations up to process_type.
// Returns a NULL-terminated array. Caller must free with drop_string_array().
char** reduce_text_process(ProcessType process_type, const char* text);
```

### Memory Management

```c
void drop_simple_result(SimpleResult* result);
void drop_simple_result_list(SimpleResultList* list);
void drop_string(char* ptr);
void drop_string_array(char** array);
```

**Always** call the appropriate `drop_*` function on returned pointers to avoid memory leaks.

## Usage Example

```c
#include <stdio.h>
#include <stdbool.h>
#include "matcher_c.h"

int main() {
    // --- SimpleMatcher ---
    // Config: ProcessType as outer key, word_id -> pattern as inner map
    // PROCESS_TYPE_NONE = 1
    char* config = "{\"1\":{\"1\":\"hello&world\"}}";

    void* matcher = init_simple_matcher(config);
    if (!matcher) return 1;

    if (simple_matcher_is_match(matcher, "hello world")) {
        printf("Matched!\n");
    }

    SimpleResultList* results = simple_matcher_process(matcher, "hello world");
    if (results) {
        for (size_t i = 0; i < results->len; i++) {
            printf("  word_id=%u, word=%s\n",
                   results->items[i].word_id, results->items[i].word);
        }
        drop_simple_result_list(results);
    }

    SimpleResult* first = simple_matcher_find_match(matcher, "hello world");
    if (first) {
        printf("First match: word_id=%u, word=%s\n", first->word_id, first->word);
        drop_simple_result(first);
    }

    drop_simple_matcher(matcher);

    // --- OR, NOT, and Word Boundary ---
    // Rules: OR (word 1), NOT (word 2), word boundary (word 3), combined (word 4)
    char* rules_config = "{\"1\":{"
        "\"1\":\"color|colour\","
        "\"2\":\"banana~peel\","
        "\"3\":\"\\\\bcat\\\\b\","
        "\"4\":\"bright&color|colour~\\\\bdark\\\\b\""
    "}}";
    void* rules_matcher = init_simple_matcher(rules_config);
    if (rules_matcher) {
        printf("OR:       %d\n", simple_matcher_is_match(rules_matcher, "nice colour"));     // 1
        printf("NOT ok:   %d\n", simple_matcher_is_match(rules_matcher, "banana split"));    // 1
        printf("NOT veto: %d\n", simple_matcher_is_match(rules_matcher, "banana peel"));     // 0
        printf("Boundary: %d\n", simple_matcher_is_match(rules_matcher, "the cat sat"));     // 1
        printf("Substr:   %d\n", simple_matcher_is_match(rules_matcher, "concatenate"));     // 0
        printf("Combined: %d\n", simple_matcher_is_match(rules_matcher, "bright colour"));   // 1
        drop_simple_matcher(rules_matcher);
    }

    // --- Text Processing ---
    char* normalized = text_process(PROCESS_TYPE_NORMALIZE, "Ｈｅｌｌｏ");
    if (normalized) {
        printf("Normalized: %s\n", normalized);
        drop_string(normalized);
    }

    char** variants = reduce_text_process(PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE, "你好，世界！");
    if (variants) {
        for (int i = 0; variants[i] != NULL; i++) {
            printf("Variant %d: %s\n", i, variants[i]);
        }
        drop_string_array(variants);
    }

    return 0;
}
```

## Error Handling

- **Construction** (`init_simple_matcher`): returns `NULL` if the JSON config is malformed or contains invalid `ProcessType` values. Always check the return value.
- **Matching** (`simple_matcher_is_match`, `simple_matcher_process`, `simple_matcher_find_match`): infallible once the matcher is built. `find_match` returns `NULL` for no match (not an error).

## Contributing

Contributions to `matcher_c` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_c` is licensed under the MIT OR Apache-2.0 license.
