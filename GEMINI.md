# Matcher Project Context (GEMINI.md)

## Project Overview
**Matcher** is a high-performance, multi-language string matching library implemented in Rust. It is designed to solve complex matching problems involving **logical operations** (AND, OR, NOT) and **text variations** (Simplified/Traditional Chinese, Pinyin, Normalization, Symbol Removal).

The project consists of a core Rust library (`matcher_rs`) with bindings for:
- **Python** (`matcher_py`): Using PyO3 and Maturin.
- **Java** (`matcher_java`): Using JNA to interface with the C library.
- **C** (`matcher_c`): Providing an FFI layer for C/C++ and other languages.

### Key Features
- **Matching Engines**: 
  - `SimpleMatcher`: Ultra-fast Aho-Corasick based matcher.
  - `RegexMatcher`: Supports regular expressions and `RegexSet`.
  - `SimMatcher`: Fuzzy/Similarity-based matching (via `rapidfuzz`).
  - `Matcher`: A high-level orchestrator combining all engines with `MatchID` and `TableID` abstractions.
- **Text Transformation Pipeline**: Normalizes input text before matching (Fanjian, Pinyin, White-space removal, etc.).
- **Logical Operators**: Supports `&` (AND), `~` (NOT), and implicit OR logic within word lists.
- **Performance**: Optimized with DFA (optional), `mimalloc`, and pre-compiled static automata for zero-cost initialization of transformation rules.

---

## Building and Running

### Prerequisites
- **Rust**: Nightly toolchain recommended (`rust-toolchain.toml` specifies `nightly-2025-02-12`).
- **Python**: `uv` or `pip` with `maturin`.
- **Java**: Maven and JDK 21+.

### Key Commands
- **Build Core (Rust & C)**:
  ```bash
  cargo build --release
  ```
- **Build & Test Python Bindings**:
  ```bash
  cd matcher_py
  uv run maturin develop
  uv run pytest
  ```
- **Build Java Bindings**:
  ```bash
  # Requires libmatcher_c to be built and copied to resources
  make build
  cd matcher_java
  mvn clean install
  ```
- **Run All Tests**:
  ```bash
  make test
  ```
- **Benchmarks**:
  ```bash
  cd matcher_rs
  cargo bench
  ```

---

## Architecture & Development Conventions

### Project Structure
- `matcher_rs/`: The heart of the project. Contains the matching logic and text processing engines.
- `matcher_py/`: Python package structure, including Rust FFI code and Python stubs (`.pyi`).
- `matcher_c/`: C header and Rust FFI implementation.
- `matcher_java/`: Java source code and JNA mappings.
- `data/`: Raw data files for building transformation maps (Unicode variants, Pinyin, etc.).

### Coding Standards
- **Edition**: Rust 2024.
- **Performance First**: Favor zero-copy operations (`Cow<'a, str>`) and efficient allocators (`mimalloc`).
- **Safety**: While optimized, the codebase follows standard Rust safety practices unless `unsafe` is strictly necessary for performance (e.g., in Aho-Corasick).
- **Logical Syntax**: 
  - `word1&word2`: Both must match.
  - `word1~word2`: `word1` must match, but `word2` must NOT match.

### Testing Practices
- **Rust**: Extensive unit tests in `matcher_rs/tests/` and property-based testing (`proptest`).
- **Python**: `pytest` for integration testing of the bindings.
- **CI**: GitHub Actions (`.github/workflows/`) handles testing across OSs and coverage.

---

## Usage Patterns
- **Direct Matching**: Using `SimpleMatcher` for high-throughput word list filtering.
- **Complex Orchestration**: Using `Matcher` to define hierarchical matching rules with IDs for database integration.
- **Text Normalization**: Utilizing the `process` module to clean or transform text independently of matching.

For deep architectural details, refer to `DESIGN.md`.
