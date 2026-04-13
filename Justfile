# Matcher — high-performance multi-language word/text matcher in Rust
#
# Workspace: matcher_rs (core), matcher_py (Python), matcher_java (Java), matcher_c (C)
# Toolchain: nightly Rust, divan (bench harness), cargo-nextest (test runner)
# Bench scripts: scripts/ (Python, run via uv)
# Bench records: scripts/bench_records/ (timestamped dirs with aggregate.json)

ext := if os() == "macos" { "dylib" } else if os() == "linux" { "so" } else { "dll" }

# ── Build ─────────────────────────────────────────────────────────────────────

# Full workspace build + copy binding artifacts to language-specific dirs.
build:
    cargo update
    cargo build --release -p matcher_c -p matcher_java -p matcher_py
    cp ./target/release/libmatcher_c.{{ext}} ./matcher_c/libmatcher_c.{{ext}}
    mkdir -p ./matcher_java/src/main/resources
    cp ./target/release/libmatcher_java.{{ext}} ./matcher_java/src/main/resources/libmatcher_java.{{ext}}
    cd matcher_py && uv sync

# Upgrade all dependencies (requires cargo-upgrade).
update:
    cargo update --verbose --recursive --breaking -Z unstable-options
    cargo upgrade --verbose --recursive

# ── Check / Format ────────────────────────────────────────────────────────────

# Fast type-check: no codegen, catches errors quickly.
check:
    cargo check --workspace --all-targets

# Auto-format all Rust code.
fmt:
    cargo fmt --all

# Check formatting without modifying files.
fmt-check:
    cargo fmt --all --check

# ── Lint ──────────────────────────────────────────────────────────────────────

# Full lint: per-crate lint + workspace-wide all-features clippy + doc build.
lint: lint-rs lint-py lint-java lint-c lint-scripts
    cargo all-features clippy --workspace --all-targets -- -D warnings
    cargo doc --workspace --all-features --no-deps

# Check-only lint (no auto-fix) — used in CI and pre-commit.
lint-check:
    cargo fmt --all --check
    cargo all-features clippy --workspace --all-targets -- -D warnings
    cargo doc --workspace --all-features --no-deps

[working-directory: 'matcher_rs']
lint-rs:
    cargo fmt --all
    cargo all-features clippy -- -- -D warnings

[working-directory: 'matcher_py']
lint-py:
    cargo fmt --all
    cargo clippy -- -D warnings
    uv run ruff check --fix && uv run ty check

[working-directory: 'matcher_java']
lint-java:
    cargo fmt --all
    cargo clippy -- -D warnings
    mvn checkstyle:check

[working-directory: 'matcher_c']
lint-c:
    cargo fmt --all
    cargo clippy -- -D warnings

[working-directory: 'scripts']
lint-scripts:
    uv run ty check

# ── Test ──────────────────────────────────────────────────────────────────────

# All languages: Rust (all feature combos + doctests) + Python + Java + C.
test: test-rs test-py test-java test-c

# Rust: all feature combos via cargo-all-features + doctests.
[working-directory: 'matcher_rs']
test-rs *args:
    cargo all-features nextest run {{args}}
    cargo test --doc

# Rust: default features only (fastest iteration).
# Pass-through args: test name (substring match), --no-default-features, --test <file>.
[working-directory: 'matcher_rs']
test-quick *args:
    cargo nextest run {{args}}

[working-directory: 'matcher_py']
test-py:
    uv run pytest

test-java:
    cargo build --release -p matcher_java
    mkdir -p ./matcher_java/src/main/resources
    cp ./target/release/libmatcher_java.{{ext}} ./matcher_java/src/main/resources/libmatcher_java.{{ext}}
    cd matcher_java && mvn test

test-c:
    cargo build --release -p matcher_c
    cp ./target/release/libmatcher_c.{{ext}} ./matcher_c/libmatcher_c.{{ext}}
    {{env("CC", "cc")}} -Wall -Wextra -L./matcher_c -Wl,-rpath,./matcher_c -lmatcher_c -I./matcher_c \
        matcher_c/tests/test_matcher.c -o matcher_c/tests/test_matcher
    ./matcher_c/tests/test_matcher

# ── Benchmark ─────────────────────────────────────────────────────────────────
# Harness: divan. Orchestration: scripts/run_benchmarks.py.
# Output: timestamped dirs under scripts/bench_records/.
#
# Bench targets (3 binaries):
#   bench_search    — search throughput (scaling, rule shapes, text length, hit/miss)
#   bench_transform — text transform pipeline overhead
#   bench_build     — SimpleMatcher::new() construction time
#
# All bench recipes accept pass-through args for run_benchmarks.py:
#   --quick             sample-count=5, min-time=0.5, repeats=1, no warmup
#   --filter <pattern>  divan module filter (e.g., scaling, rule_complexity, text_transform)
#   --repeats N         recorded runs (default: 3)
#   --profile <name>    cargo profile (default: bench)

_bench_script := "scripts/run_benchmarks.py"

# Search preset: bench_search + bench_transform.
bench-search *args:
    uv run {{_bench_script}} --preset search {{args}}

# Build preset: bench_build only.
bench-build *args:
    uv run {{_bench_script}} --preset build {{args}}

# All presets: search + build.
bench-all *args:
    uv run {{_bench_script}} --preset all {{args}}

# Compare two benchmark runs. Inputs: run dirs, aggregate.json, or raw .txt files.
bench-compare baseline candidate *args:
    uv run scripts/compare_benchmarks.py "{{baseline}}" "{{candidate}}" {{args}}

# Interactive HTML dashboard from benchmark results (single run or comparison).
bench-viz *args:
    uv run scripts/visualize_benchmarks.py {{args}}

# Rebuild std with -C target-cpu=native for authoritative measurements.
# Use before final adopt/revert bench-compare runs. Adds ~30s compile time.
bench-buildstd *args:
    RUSTFLAGS="-C target-cpu=native" cargo +nightly -Z build-std=std,core bench --profile bench -p matcher_rs {{args}}

# ── Engine Characterization ───────────────────────────────────────────────────
# Sweeps (engine x size x pat_cjk x text_cjk) matrix → CSV to stdout.
# Engines: ac_dfa, daac_bytewise, daac_charwise. Requires dfa feature.
# Override via env: ENGINES, SIZES, PAT_CJK, TEXT_CJK, MODES, ITERS, TEXT_BYTES.

# Full matrix sweep.
characterize-engines *args:
    cargo run --profile bench --example characterize_engines -p matcher_rs {{args}}

# Quick subset for directional signal.
characterize-engines-quick:
    SIZES=500,2000,10000,50000 PAT_CJK=0,50,100 TEXT_CJK=0,20,50,100 ITERS=3 \
    cargo run --profile bench --example characterize_engines -p matcher_rs

# Visualize dispatch CSV as interactive Plotly heatmaps.
characterize-viz *args:
    uv run scripts/visualize_dispatch.py {{args}}

# ── Profiling ─────────────────────────────────────────────────────────────────
# macOS only (Xcode Instruments Time Profiler). Targets: profile_search, profile_build.
# Subcommands: record, analyze, open.
#   just profile record --scene en-search --analyze
#   just profile record --target build --dict en --rules 50000 --analyze
#   just profile analyze scripts/profile_records/prof_*.trace

profile *args:
    uv run scripts/instruments_profile.py {{args}}

# ── Fuzz ──────────────────────────────────────────────────────────────────────

fuzz target="fuzz_matcher_new" *args="":
    cd matcher_rs && cargo fuzz run {{target}} {{args}}

fuzz-list:
    cd matcher_rs && cargo fuzz list

# ── Coverage ──────────────────────────────────────────────────────────────────

coverage:
    cargo tarpaulin -p matcher_rs --fail-under 75 --out xml \
        --exclude-files 'matcher_rs/benches/*' \
        --exclude-files 'matcher_rs/examples/*' \
        --exclude-files 'matcher_py/*' \
        --exclude-files 'matcher_java/*' \
        --exclude-files 'matcher_c/*'
    @echo "Coverage report: cobertura.xml"

# ── Clean ─────────────────────────────────────────────────────────────────────

clean:
    cargo clean
    rm -f matcher_c/libmatcher_c.{{ext}}
    rm -f matcher_java/src/main/resources/libmatcher_java.{{ext}}
    rm -f matcher_c/tests/test_matcher
