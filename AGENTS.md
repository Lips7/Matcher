# AGENTS.md — Matcher Repository Guide

This file is intended for AI coding agents. It describes the project layout, conventions, how to build and test, and important design patterns to be aware of before making changes.

---

## Repository Overview

Matcher is a high-performance, multi-language word-matching library implemented in Rust and exposed via FFI bindings.

| Directory       | Language      | Build Tool       | Purpose                                                  |
| --------------- | ------------- | ---------------- | -------------------------------------------------------- |
| `matcher_rs/`   | Rust          | `cargo`          | Core library — all matching logic lives here             |
| `matcher_py/`   | Python (PyO3) | `maturin`        | Python bindings (`pip install matcher_py`)               |
| `matcher_c/`    | C (FFI)       | `cargo` (cdylib) | C FFI bindings & exported headers                        |
| `matcher_java/` | Java (JNA)    | `maven`          | Java bindings using `matcher_c` library                  |
| `data/`         | —             | —                | Unicode process maps used to build transformation tables |

The single source of truth for matching logic is **`matcher_rs`**. All other packages are wrappers around it.

---

## Core Concepts

### Extension Types & Configuration
Matching rules are typically defined in JSON. To ensure type safety across languages, we use "Extension Types":
- **Python/C**: `extension_types.py` (found in both `matcher_py` and `matcher_c`). It defines `TypedDict` and `IntFlag` enums (e.g., `ProcessType`, `MatchTable`, `MatchTableType`).
- **Java**: `com.matcher_java.extension_types` package. Contains Java classes equivalent to the Python types for serialized configuration.

Always use these helper types when constructing configurations in tests or examples.

### Core Engines
- **`SimpleMatcher`**: Fast Aho-Corasick matcher over a flat word map. Supports AND (`&`), NOT (`~`) logic within a word entry.
- **`Matcher`**: Orchestrates `SimpleMatcher`, `RegexMatcher`, and `SimMatcher` (similarity) across multiple `MatchTable`s.

---

## Workspace Layout

### `matcher_rs/` (Rust Core)
- `src/lib.rs`: Exports public API.
- `src/builder.rs`: Builder patterns for all matchers.
- `src/process/`: Transformation logic (Fanjian, Pinyin, etc.).
- `src/simple_matcher.rs`: Core AC engine.

### `matcher_c/` (C FFI)
- `src/lib.rs`: Defines the `no_mangle` FFI surface.
- `matcher_c.h`: Hand-written header matching the Rust FFI.

---

## Build & Test

### Rust Core (`matcher_rs`)
```bash
cargo build -p matcher_rs --release
cargo test -p matcher_rs
cargo bench -p matcher_rs --features "dfa"
```

### Python Bindings (`matcher_py`)
Uses `maturin` for building and `pytest` for testing.
```bash
cd matcher_py
maturin develop  # Build and install in current venv
pytest           # Run tests
```

### C Bindings (`matcher_c`)
```bash
cd matcher_c
cargo build --release  # Produces libmatcher_c.so/dylib/dll
```

### Java Bindings (`matcher_java`)
Requires the `matcher_c` dynamic library to be built first or available in the search path.
```bash
cd matcher_java
./build_native.sh  # Helper script to build the C lib and place it for JNA
mvn test           # Run Java tests
```

---

## FFI Best Practices & Memory Management

### The `drop_*` Pattern
Any pointer returned by the C FFI (e.g., from `init_matcher` or `matcher_process_as_string`) **must** be manually freed using the corresponding `drop_*` function to avoid memory leaks:
- `drop_matcher(void*)`
- `drop_simple_matcher(void*)`
- `drop_string(char*)`

### JSON Communication
Communication between languages is largely handled via JSON strings. The FFI layer deserializes these into Rust types defined in `matcher_rs`. Ensure the JSON structure exactly matches the `MatchTable` or `SimpleTable` requirements.

---

## Code Conventions

### README Standardization
All `README.md` files follow a standard professional layout:
1. Title + Badges
2. Overview
3. Table of Contents (Optional/Context-dependent)
4. Installation (Build from source / Pre-built)
5. Usage Examples (using Extension Types)
6. Important Notes (FFI details, toolchains)
