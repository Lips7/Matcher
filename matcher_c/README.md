# Matcher C FFI Bindings

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

C FFI bindings for the [Matcher](https://github.com/Lips7/Matcher) library — a high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust.

For detailed implementation, see the [Design Document](../DESIGN.md).

**Requirements:** C11+. All strings are null-terminated UTF-8.

## Installation

### Build from source

Requires the Rust **nightly** toolchain.

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
just build
```

`just build` compiles all packages and copies the dynamic library to the right location automatically. You can also build manually:

```shell
cargo build --release
cp target/release/libmatcher_c.dylib matcher_c/  # .so on Linux, .dll on Windows
```

### Link

```
-L/path/to/matcher_c -lmatcher_c
```

Include `matcher_c.h` in your source.

## Usage

### Builder (recommended)

The builder API avoids manual JSON construction:

```c
#include "matcher_c.h"

void* builder = init_simple_matcher_builder();
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 1, "hello&world");
simple_matcher_builder_add_word(builder, PROCESS_TYPE_DELETE, 2, "你好");

void* matcher = simple_matcher_builder_build(builder);
// builder is consumed — do NOT use or free it after build()

if (simple_matcher_is_match(matcher, "hello, world")) {
    printf("matched!\n");
}

SimpleResult* first = simple_matcher_find_match(matcher, "hello, world");
if (first) {
    printf("word_id=%u, word=%s\n", first->word_id, first->word);
    drop_simple_result(first);
}

drop_simple_matcher(matcher);
```

### JSON constructor

You can also construct from raw JSON bytes:

```c
const char* json = "{\"1\":{\"1\":\"hello&world\"}}";
void* matcher = init_simple_matcher(json);
// ... use matcher ...
drop_simple_matcher(matcher);
```

### ProcessType

| Constant | Value | Description |
|----------|-------|-------------|
| `PROCESS_TYPE_NONE` | 1 | No transformation; match raw input |
| `PROCESS_TYPE_VARIANT_NORM` | 2 | CJK variant normalization |
| `PROCESS_TYPE_DELETE` | 4 | Remove symbols, punctuation, whitespace |
| `PROCESS_TYPE_NORMALIZE` | 8 | Normalize character variants to basic forms |
| `PROCESS_TYPE_DELETE_NORMALIZE` | 12 | Delete + Normalize combined |
| `PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE` | 14 | VariantNorm + Delete + Normalize |
| `PROCESS_TYPE_ROMANIZE` | 16 | CJK → space-separated romanization |
| `PROCESS_TYPE_ROMANIZE_CHAR` | 32 | CJK → romanization without spaces |
| `PROCESS_TYPE_EMOJI_NORM` | 64 | Emoji → English words (CLDR) |

Combine with bitwise OR: `PROCESS_TYPE_DELETE | PROCESS_TYPE_NORMALIZE`.

### Query methods

```c
// Boolean check (fastest)
bool matched = simple_matcher_is_match(matcher, text);

// All matches
SimpleResultList* results = simple_matcher_process(matcher, text);
for (size_t i = 0; i < results->len; i++) {
    printf("%u: %s\n", results->items[i].word_id, results->items[i].word);
}
drop_simple_result_list(results);

// First match only (early exit)
SimpleResult* first = simple_matcher_find_match(matcher, text);
if (first) {
    // ...
    drop_simple_result(first);
}

// Memory introspection
size_t bytes = simple_matcher_heap_bytes(matcher);
```

### OR, NOT, and Word Boundary

Pattern operators work in both builder and JSON construction:

```c
void* builder = init_simple_matcher_builder();
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 1, "color|colour");      // OR
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 2, "banana~peel");       // NOT
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 3, "\\bcat\\b");         // word boundary
simple_matcher_builder_add_word(builder, PROCESS_TYPE_NONE, 4, "bright&color|colour~\\bdark\\b"); // combined
void* matcher = simple_matcher_builder_build(builder);

simple_matcher_is_match(matcher, "nice colour");       // true  (OR)
simple_matcher_is_match(matcher, "banana split");      // true  (NOT: no veto)
simple_matcher_is_match(matcher, "banana peel");       // false (NOT: vetoed)
simple_matcher_is_match(matcher, "the cat sat");       // true  (word boundary)
simple_matcher_is_match(matcher, "concatenate");       // false (substring, not whole word)

drop_simple_matcher(matcher);
```

### Text Processing

Apply transformations without a matcher:

```c
char* normalized = text_process(PROCESS_TYPE_NORMALIZE, "Ｈｅｌｌｏ");
printf("%s\n", normalized);  // "hello"
drop_string(normalized);

char** variants = reduce_text_process(PROCESS_TYPE_VARIANT_NORM_DELETE_NORMALIZE, "你好，世界！");
for (size_t i = 0; variants[i] != NULL; i++) {
    printf("Variant %zu: %s\n", i, variants[i]);
}
drop_string_array(variants);
```

## Memory Management

Every pointer returned has a corresponding `drop_*` function:

| Returned by | Free with |
|-------------|-----------|
| `init_simple_matcher` / `builder_build` | `drop_simple_matcher` |
| `simple_matcher_process` | `drop_simple_result_list` |
| `simple_matcher_find_match` | `drop_simple_result` |
| `text_process` | `drop_string` |
| `reduce_text_process` | `drop_string_array` |

`matcher_version()` returns a static pointer — do NOT free it.

`simple_matcher_builder_build()` always consumes the builder — do NOT free or reuse it after calling build.

## Error Handling

All functions return null / `false` on error and print diagnostics to stderr. Construction (`init_simple_matcher`, `builder_build`) is the only operation that can fail. Matching methods are infallible once the matcher is built.

## License

`matcher_c` is licensed under the MIT OR Apache-2.0 license.
