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
cargo build --release
```

After building, you will find the dynamic library in the `target/release` directory:
- Linux: `libmatcher_c.so`
- macOS: `libmatcher_c.dylib`
- Windows: `matcher_c.dll`

## API

The full API is declared in [`matcher_c.h`](./matcher_c.h).

### ProcessType Bit Flags

| Define | Value | Description |
|--------|-------|-------------|
| `PROCESS_TYPE_NONE` | 1 | No transformation; match raw input |
| `PROCESS_TYPE_FANJIAN` | 2 | Traditional Chinese to Simplified Chinese |
| `PROCESS_TYPE_DELETE` | 4 | Remove symbols, punctuation, whitespace |
| `PROCESS_TYPE_NORMALIZE` | 8 | Normalize character variants to basic forms |
| `PROCESS_TYPE_DELETE_NORMALIZE` | 12 | Delete + Normalize combined |
| `PROCESS_TYPE_FANJIAN_DELETE_NORMALIZE` | 14 | Fanjian + Delete + Normalize combined |
| `PROCESS_TYPE_PINYIN` | 16 | Chinese characters to space-separated Pinyin |
| `PROCESS_TYPE_PINYIN_CHAR` | 32 | Chinese characters to Pinyin without boundary spaces |

### SimpleMatcher

```c
// Initialize from JSON config string. Returns NULL on error.
void* init_simple_matcher(const char* simple_table_bytes);

// Check if text matches any pattern.
bool simple_matcher_is_match(const void* simple_matcher, const char* text);

// Get match results as a JSON string. Caller must free with drop_string().
char* simple_matcher_process_as_string(const void* simple_matcher, const char* text);

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

    char* result = simple_matcher_process_as_string(matcher, "hello world");
    if (result) {
        printf("Result: %s\n", result);
        drop_string(result);
    }

    drop_simple_matcher(matcher);

    // --- Text Processing ---
    char* normalized = text_process(PROCESS_TYPE_NORMALIZE, "Ｈｅｌｌｏ");
    if (normalized) {
        printf("Normalized: %s\n", normalized);
        drop_string(normalized);
    }

    char** variants = reduce_text_process(PROCESS_TYPE_FANJIAN_DELETE_NORMALIZE, "你好，世界！");
    if (variants) {
        for (int i = 0; variants[i] != NULL; i++) {
            printf("Variant %d: %s\n", i, variants[i]);
        }
        drop_string_array(variants);
    }

    return 0;
}
```

## Contributing

Contributions to `matcher_c` are welcome! If you find a bug or have a feature request, please open an issue on the [GitHub repository](https://github.com/Lips7/Matcher). If you would like to contribute code, please fork the repository and submit a pull request.

## License

`matcher_c` is licensed under the MIT OR Apache-2.0 license.
