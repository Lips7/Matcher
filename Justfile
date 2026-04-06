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
test-rs *args:
    cargo all-features nextest run {{args}}
    cargo test --doc

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

# -- Bench ---------------------------------------------------------------------
# All bench recipes accept pass-through args: --quick, --filter, --repeats, etc.
#   just bench-search                              # Full search preset (~15 min)
#   just bench-search --quick                      # Quick directional signal (~2 min)
#   just bench-search --filter text_transform      # Only transform benchmarks (~2 min)
#   just bench-search --filter scaling             # Only scaling benchmarks (~5 min)

_bench_script := "matcher_rs/scripts/run_benchmarks.py"

bench-search *args:
    uv run {{_bench_script}} --preset search {{args}}

bench-build *args:
    uv run {{_bench_script}} --preset build {{args}}

bench-engine-search *args:
    uv run {{_bench_script}} --preset engine-search {{args}}

bench-engine-build *args:
    uv run {{_bench_script}} --preset engine-build {{args}}

bench-engine-is-match *args:
    uv run {{_bench_script}} --preset engine-is-match {{args}}

bench-all *args:
    uv run {{_bench_script}} --preset all {{args}}

bench-compare baseline candidate *args:
    uv run matcher_rs/scripts/compare_benchmark_runs.py "{{baseline}}" "{{candidate}}" {{args}}

bench-compare-raw baseline candidate *args:
    uv run matcher_rs/scripts/compare_benchmarks.py "{{baseline}}" "{{candidate}}" {{args}}

bench-viz *args:
    uv run matcher_rs/scripts/visualize_benchmarks.py {{args}}

# Engine dispatch characterization — full matrix sweep to CSV
# Examples:
#   just characterize-engines                                     # full (~20-30 min)
#   just characterize-engines-quick                               # subset (~3 min)
#   ENGINES=ac_dfa,daac_charwise SIZES=500,10000 just characterize-engines  # custom
characterize-engines *args:
    cargo run --profile bench --example characterize_engines -p matcher_rs {{args}}

characterize-engines-quick:
    SIZES=500,2000,10000,50000 PAT_CJK=0,50,100 TEXT_CJK=0,20,50,100 ITERS=3 \
    cargo run --profile bench --example characterize_engines -p matcher_rs

characterize-viz *args:
    uv run matcher_rs/scripts/visualize_dispatch.py {{args}}

# Profile with Xcode Instruments (Time Profiler)
# Examples:
#   just profile record --mode is_match --dict en --rules 10000 --analyze
#   just profile record --mode process --dict cn --open
#   just profile analyze /tmp/prof_*.trace
profile *args:
    uv run matcher_rs/scripts/instruments_profile.py {{args}}

# -- Coverage ------------------------------------------------------------------

coverage:
    cargo tarpaulin -p matcher_rs --fail-under 75 --out xml \
        --exclude-files 'matcher_rs/benches/*' \
        --exclude-files 'matcher_rs/examples/*' \
        --exclude-files 'matcher_py/*' \
        --exclude-files 'matcher_java/*' \
        --exclude-files 'matcher_c/*'
    @echo "Coverage report: cobertura.xml"
