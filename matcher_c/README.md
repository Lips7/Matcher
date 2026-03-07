# Matcher Rust Implement C FFI bindings

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/lips7/Matcher/test.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

A high-performance matcher designed to solve **LOGICAL** and **TEXT VARIATIONS** problems in word matching, implemented in Rust with C FFI bindings for cross-language support.

For detailed implementation, see the [Design Document](../DESIGN.md).

## Overview

This package provides C FFI (Foreign Function Interface) bindings for the Matcher library. It allows you to use the high-performance matching capabilities of Matcher in C, C++, Python (via `cffi`), and other languages that support C FFI.

Key features exposed:
- High-performance text matching with logical operators (`&`, `~`).
- Support for various text normalization processes (Fanjian, Delete, Normalize, PinYin).
- Multiple matching types: Simple Word Match.

## Installation

### Build from source

```shell
git clone https://github.com/Lips7/Matcher.git
cd Matcher/matcher_c
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
cargo build --release
```

After building, you will find the dynamic library in the `target/release` directory:
- Linux: `libmatcher_c.so`
- macOS: `libmatcher_c.dylib`
- Windows: `matcher_c.dll`

### Install pre-built binary

Visit the [release page](https://github.com/Lips7/Matcher/releases) to download the pre-built binary.

## C Usage Example

You can use the `matcher_c.h` header and the compiled library in your C projects.

```c
#include <stdio.h>
#include <stdbool.h>
#include "matcher_c.h"

int main() {
    // Configuration in JSON format
    // ProcessType: MatchNone = 1
    char* config = "{\"1\":{\"1\":\"hello&world\"}}";

    // Initialize simple matcher
    void* simple_matcher = init_simple_matcher(config);

    // Check if a text matches
    if (simple_matcher_is_match(simple_matcher, "hello world")) {
        printf("Matches!\n");
    }

    // Process and get result as JSON string
    char* result = simple_matcher_process_as_string(simple_matcher, "hello world");
    printf("Result: %s\n", result);

    // Clean up
    drop_string(result);
    drop_simple_matcher(simple_matcher);

    return 0;
}
```

## Python Usage Example

Using the C FFI bindings via Python's `cffi` library and the provided `extension_types.py`:

```python
import json
from cffi import FFI
from extension_types import ProcessType

# Initialize FFI and load library
ffi = FFI()
with open("./matcher_c.h", "r", encoding="utf-8") as f:
    ffi.cdef(f.read())
lib = ffi.dlopen("./libmatcher_c.so") # Adjust extension for your OS

# Define configuration using simple dict
config = {
    ProcessType.MatchNone: {
        1: "hello&world",
        2: "test"
    }
}

# Init simple matcher
matcher = lib.init_simple_matcher(json.dumps(config).encode())

# Check match
is_match = lib.simple_matcher_is_match(matcher, "hello world".encode("utf-8"))
print(f"Is match: {is_match}")

# Match and get string result
res = lib.simple_matcher_process_as_string(matcher, "hello world, test".encode("utf-8"))
print(ffi.string(res).decode("utf-8"))
lib.drop_string(res)

# Clean up
lib.drop_simple_matcher(matcher)
```

## Important Notes

1. **Header File**: The `matcher_c.h` defines the exported functions.
2. **Memory Management**: Always call `drop_matcher`, `drop_simple_matcher`, and `drop_string` for any pointer returned by the library to avoid memory leaks.
3. **Rust Toolchain**: Building from source requires the Rust **nightly** toolchain.