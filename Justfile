ext := if os() == "macos" { "dylib" } else if os() == "linux" { "so" } else { "dll" }

# -- Build ---------------------------------------------------------------------

build:
    cargo update
    cargo build --release
    cp ./target/release/libmatcher_c.{{ext}} ./matcher_c/libmatcher_c.{{ext}}
    mkdir -p ./matcher_java/src/main/resources
    cp ./target/release/libmatcher_java.{{ext}} ./matcher_java/src/main/resources/libmatcher_java.{{ext}}
    cd matcher_py && uv sync

update:
    cargo update --verbose --recursive --breaking -Z unstable-options
    cargo upgrade --verbose --recursive

# -- Check / Format ------------------------------------------------------------

check:
    cargo check --workspace --all-targets

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

# -- Lint ----------------------------------------------------------------------

lint: lint-all lint-rs lint-py lint-java lint-c

lint-all:
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

# -- Test ----------------------------------------------------------------------

test: test-rs test-py test-java test-c

[working-directory: 'matcher_rs']
test-rs:
    cargo all-features nextest run
    cargo test --doc

[working-directory: 'matcher_rs']
test-quick:
    cargo nextest run

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

# -- Bench ---------------------------------------------------------------------

_bench_script := "matcher_rs/scripts/run_benchmarks.py"

bench-search *args:
    python3 {{_bench_script}} --preset search {{args}}

bench-build *args:
    python3 {{_bench_script}} --preset build {{args}}

bench-engine-search *args:
    python3 {{_bench_script}} --preset engine-search {{args}}

bench-engine-build *args:
    python3 {{_bench_script}} --preset engine-build {{args}}

bench-engine-is-match *args:
    python3 {{_bench_script}} --preset engine-is-match {{args}}

bench-all *args:
    python3 {{_bench_script}} --preset all {{args}}

bench-compare baseline candidate *args:
    python3 matcher_rs/scripts/compare_benchmark_runs.py "{{baseline}}" "{{candidate}}" {{args}}

bench-compare-raw baseline candidate *args:
    python3 matcher_rs/scripts/compare_benchmarks.py "{{baseline}}" "{{candidate}}" {{args}}

# -- Coverage ------------------------------------------------------------------

coverage:
    cargo tarpaulin -p matcher_rs --fail-under 75 --out xml \
        --exclude-files 'matcher_rs/src/simple_matcher/harry/avx512.rs' \
        --exclude-files 'matcher_rs/benches/*' \
        --exclude-files 'matcher_rs/examples/*' \
        --exclude-files 'matcher_py/*' \
        --exclude-files 'matcher_java/*' \
        --exclude-files 'matcher_c/*'
    @echo "Coverage report: cobertura.xml"
