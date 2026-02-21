# AGENTS.md — Matcher Repository Guide

This file is intended for AI coding agents. It describes the project layout,
conventions, how to build and test, and important design patterns to be
aware of before making changes.

---

## Repository Overview

Matcher is a high-performance, multi-language word-matching library implemented
in Rust and exposed via FFI bindings:

| Directory       | Language      | Purpose                                                  |
| --------------- | ------------- | -------------------------------------------------------- |
| `matcher_rs/`   | Rust          | Core library — all matching logic lives here             |
| `matcher_py/`   | Python (PyO3) | Python bindings (`pip install matcher_py`)               |
| `matcher_c/`    | C             | C FFI bindings                                           |
| `matcher_java/` | Java (JNA)    | Java bindings                                            |
| `data/`         | —             | Unicode process maps used to build transformation tables |

The single source of truth for matching logic is **`matcher_rs`**. All other
packages are thin wrappers around it.

---

## Core Concepts

Read `DESIGN.md` for the full design. The short version:

- **`ProcessType`** (bitflags) — text normalisation pipelines applied before
  matching (e.g. `Fanjian`, `Delete`, `Normalize`, `PinYin`). Combinations are
  deduplicated at construction time via a `ProcessType` tree.
- **`SimpleMatcher`** — fast Aho-Corasick matcher over a flat word map. Supports
  AND (`&`), NOT (`~`) logic within a word entry. Primary inner engine.
- **`Matcher`** — orchestrates `SimpleMatcher`, `RegexMatcher`, and `SimMatcher`
  (similarity) across a map of `MatchTable`s grouped by `match_id`.
- **`MatchTable`** defines one matching rule:
  - `table_id` + `match_table_type` (`Simple | Regex | Similar`) — required
  - `word_list` — words to match
  - `exemption_process_type` + `exemption_word_list` — NOT-logic exemptions

---

## Workspace Layout

```
matcher_rs/
  src/
    lib.rs              ← crate root & public re-exports
    builder.rs          ← SimpleMatcherBuilder, MatchTableBuilder, MatcherBuilder
    matcher.rs          ← Matcher, MatchTable, MatchTableType, traits
    simple_matcher.rs   ← SimpleMatcher (Aho-Corasick core)
    regex_matcher.rs    ← RegexMatcher
    sim_matcher.rs      ← SimMatcher (Levenshtein / similarity)
    process/            ← ProcessType definitions, tree builder, text processors
    util/               ← SimpleWord combinator helpers
  tests/
    test.rs             ← integration tests
    test_proptest.rs    ← property-based tests
```

---

## Build & Test

All commands should be run from the repo root unless stated otherwise.

```bash
# Build the Rust core (release)
cargo build -p matcher_rs --release

# Run all Rust tests
cargo test -p matcher_rs

# Run only a specific test
cargo test -p matcher_rs <test_name>

# Build with the 'dfa' feature (required for benchmarks)
cargo build -p matcher_rs --features "dfa" --release

# Run benchmarks
cargo bench -p matcher_rs --features "dfa"
```

For Python bindings, see `matcher_py/README.md` and the `Makefile`.

---

## Code Conventions

### Rust
- Follow standard Rust idioms (`clippy` clean, no `unwrap` in library code).
- Use `#[derive(Default)]` + `new()` constructors for builder types.
- All public types and functions must have `///` doc comments with at least
  one `# Example` block that compiles as a doctest.
- Builders follow the **consuming builder** pattern (`mut self -> Self`).
- Imports: use top-level `use crate::{...}` — do not use inline `crate::` paths.

### Adding a new builder
Follow the pattern in `builder.rs`:
1. Define the struct with `pub` fields or private fields + methods.
2. Implement `new(required_fields...)` and one chainable method per optional field.
3. Implement `build(self) -> TargetType`.
4. Re-export from `lib.rs`.
5. Add integration tests to `tests/test.rs` inside the relevant `mod test_*` block.

### Adding a new `ProcessType` variant
`ProcessType` is a bitflag enum defined in `src/process/process_matcher.rs`.
After adding a variant you **must** update the process tree builder logic and
add a corresponding text processor.

---

## CI Workflows (`.github/workflows/`)

| File            | Trigger               | Purpose                      |
| --------------- | --------------------- | ---------------------------- |
| `rust.yml`      | push / PR             | Build + test on nightly      |
| `coverage.yml`  | push to `master` / PR | Tarpaulin coverage → Codecov |
| `bench.yml`     | push to `master` / PR | Criterion benchmarks         |
| `publish-*.yml` | tags                  | Publish crates / packages    |

The default branch is **`master`**.

---

## Key Design Invariants — Do Not Break

1. **`ProcessType` tree deduplication** — the tree built in `Matcher::new` ensures
   each text transformation is computed at most once per input string. Any change
   to how `process_type_set` is populated must preserve this.
2. **`simple_word_table_conf_index_list` offsets** — the unsafe indexing in
   `_word_match_with_processed_text_process_type_set` relies on these being
   contiguous and correctly offset. Changing how words are inserted into
   `simple_table` requires updating the offset bookkeeping in `Matcher::new`.
3. **Lifetime invariants** — `MatchTable<'a>` borrows string slices. Builders must
   not introduce owned `String`s where `&'a str` is expected, as this would break
   FFI and serde compatibility.
4. **`Cow<'a, str>` zero-copy** — text processing returns `Cow`. Avoid eagerly
   converting to `String`, especially inside hot matching paths.
