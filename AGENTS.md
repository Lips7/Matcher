# Repository Guidelines

## Project Structure & Module Organization

Matcher is a Rust workspace with language bindings. The core implementation is in `matcher_rs/`: public APIs live in `src/lib.rs` and `src/builder.rs`, matching logic in `src/simple_matcher/`, and text transforms in `src/process/transform/`. Put core integration tests in `matcher_rs/tests/`; examples and performance benchmarks belong in `matcher_rs/examples/` and `matcher_rs/benches/`.

Bindings are isolated in `matcher_py/` (PyO3 and Python tests), `matcher_java/` (JNI, Maven sources/tests), and `matcher_c/` (FFI header and C test). Transformation maps are maintained in `matcher_rs/process_map/`; use `scripts/generate_process_map.py` when regenerating them. See `DESIGN.md` before changing matcher internals.

## Build, Test, and Development Commands

Use the nightly Rust toolchain specified by `rust-toolchain.toml` and run commands from the repository root:

```shell
just check                 # Fast workspace type-check
just fmt                   # Format Rust code
just fmt-check             # Verify Rust formatting
just test-quick            # Core Rust tests, default features
just test-rs               # Core feature matrix plus doctests
just test                  # Rust, Python, Java, and C suites
just lint-check            # Formatting, Clippy, and documentation checks
```

`just build` produces release binding libraries and prepares Python dependencies. It requires `just`, `cargo-nextest`, `cargo-all-features`, `uv`, Java 21+ with Maven, and a C compiler for the relevant suites.

## Coding Style & Naming Conventions

Follow `rustfmt.toml` by running `just fmt`; Rust uses edition 2024, crate-grouped imports, and standard four-space indentation. Use `snake_case` for Rust modules, functions, and test files; `CamelCase` for Rust types. Keep binding APIs consistent with the core API. Prefer small, focused modules and document public API behavior with Rust doc comments. Python follows Ruff and `ty`; Java must pass Maven Checkstyle.

## Testing Guidelines

Add regression coverage alongside the affected layer: Rust integration tests use `matcher_rs/tests/test_*.rs`, Python uses `matcher_py/tests/test_*.py`, and Java/C tests reside under each binding's `tests` directory. Name tests after observable behavior, such as `test_romanize_respects_word_boundary`. Run `just test-quick` while iterating, then `just lint-check && just test` before submission. Core coverage is enforced at 75% by `just coverage`.

## Commits & Pull Requests

Recent history uses short imperative messages, commonly conventional prefixes: `fix: correct ...`, `chore: update ...`, `docs: update ...`. Keep commits focused and avoid unrelated formatting changes. PRs should describe the behavior change, link relevant issues, include tests, and update `DESIGN.md` for meaningful architectural changes. Attach before/after benchmark comparisons for hot-path work; use `just bench-search --quick` for an initial signal.
