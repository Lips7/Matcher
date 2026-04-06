# Contributing to Matcher

## Prerequisites

Run the setup checker to verify your environment:

```shell
./scripts/dev-setup.sh
```

This validates: Rust nightly, `just`, `cargo-nextest`, `cargo-all-features`, `uv`, Python 3.8+, Java 21+, Maven, and a C compiler. Use `--check` for non-interactive mode.

## Development Workflow

### Branch Naming

Create a descriptive branch from `master`:

```shell
git checkout -b fix-romanize-boundary
```

Use hyphens, not slashes. Keep names short and descriptive.

### Making Changes

```shell
just check          # Fast type-check (seconds)
just fmt            # Auto-format all Rust code
just test-quick     # Default-feature tests (~30s)
just test-rs        # Full feature matrix + doctests
just test           # All languages (Rust + Python + Java + C)
```

### Pre-commit Checks

Pre-commit hooks are configured in `.pre-commit-config.yaml`. Run before committing:

```shell
prek run
```

This runs `cargo fmt`, `clippy`, `cargo doc`, and language-specific linters.

### Commit Messages

Use the format: `scope: concise imperative summary`

```
fix: correct Romanize boundary handling for multi-char sequences
feat: add batch_process method to C bindings
refactor: extract shared page-table helpers into replace/mod.rs
docs: update DESIGN.md density-based dispatch section
test: add proptest for Delete transform edge cases
```

### Pull Requests

- Fill out the [PR template](./.github/PULL_REQUEST_TEMPLATE.md)
- Keep PRs focused on a single concern
- Run the full test suite before submitting: `just lint && just test`
- Update `DESIGN.md` if you change non-trivial internals

### Benchmarking

If your change touches the hot path, measure before and after:

```shell
just bench-search --quick    # ~2 min, directional signal
just bench-search            # ~15 min, authoritative measurement
```

Compare results:

```shell
just bench-compare path/to/baseline path/to/candidate
```

## Project Structure

- `matcher_rs/` — Core Rust library (all algorithms)
- `matcher_py/` — Python bindings (PyO3)
- `matcher_java/` — Java bindings (JNI)
- `matcher_c/` — C FFI bindings

See [DESIGN.md](./DESIGN.md) for the architectural walkthrough.

## License

By contributing, you agree that your contributions will be licensed under MIT OR Apache-2.0.
