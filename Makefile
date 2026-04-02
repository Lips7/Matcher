OS  := $(shell uname -s | tr A-Z a-z)
EXT := $(shell \
	if [ "$(OS)" = "darwin" ]; then echo "dylib"; \
	elif [ "$(OS)" = "linux" ]; then echo "so"; \
	else echo "dll"; fi)

.PHONY: build update check fmt fmt-check \
	lint lint-rs lint-py lint-java lint-c \
	test test-rs test-py test-java test-c test-quick \
	bench-search bench-build bench-engine-search bench-engine-build bench-compare \
	coverage

# -- Build ---------------------------------------------------------------------

build:
	cargo update
	cargo build --release
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/libmatcher_c.$(EXT)
	mkdir -p ./matcher_java/src/main/resources
	cp ./target/release/libmatcher_java.$(EXT) ./matcher_java/src/main/resources/libmatcher_java.$(EXT)
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

lint: lint-rs lint-py lint-java

lint-rs:
	cd matcher_rs && cargo fmt --all
	cd matcher_rs && cargo all-features clippy -- -D warnings

lint-py:
	cd matcher_py && cargo fmt --all
	cd matcher_py && cargo clippy -- -D warnings
	cd matcher_py && uv run ruff check --fix && uv run ty check

lint-java:
	cd matcher_java && cargo fmt --all
	cd matcher_java && cargo clippy -- -D warnings
	cd matcher_java && mvn checkstyle:check

lint-c:
	cd matcher_c && cargo fmt --all
	cd matcher_c && cargo clippy -- -D warnings

# -- Test ----------------------------------------------------------------------

test: test-rs test-py test-java test-c

test-rs:
	cd matcher_rs && cargo all-features nextest run
	cd matcher_rs && cargo test --doc
	cd matcher_rs && cargo doc

test-quick:
	cd matcher_rs && cargo nextest run

test-py:
	cd matcher_py && uv run pytest

test-java:
	cargo build --release -p matcher_java
	mkdir -p ./matcher_java/src/main/resources
	cp ./target/release/libmatcher_java.$(EXT) ./matcher_java/src/main/resources/libmatcher_java.$(EXT)
	cd matcher_java && mvn test

test-c:
	cargo build --release -p matcher_c
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/libmatcher_c.$(EXT)
	$(CC) -Wall -Wextra -L./matcher_c -Wl,-rpath,./matcher_c -lmatcher_c -I./matcher_c \
		matcher_c/tests/test_matcher.c -o matcher_c/tests/test_matcher
	./matcher_c/tests/test_matcher

# -- Bench ---------------------------------------------------------------------

bench-search:
	python3 matcher_rs/scripts/run_benchmarks.py --preset search

bench-build:
	python3 matcher_rs/scripts/run_benchmarks.py --preset build

bench-engine-search:
	python3 matcher_rs/scripts/run_benchmarks.py --preset engine-search

bench-engine-build:
	python3 matcher_rs/scripts/run_benchmarks.py --preset engine-build

bench-compare:
	@test -n "$(BASELINE)" || (echo "BASELINE is required"; exit 1)
	@test -n "$(CANDIDATE)" || (echo "CANDIDATE is required"; exit 1)
	python3 matcher_rs/scripts/compare_benchmark_runs.py "$(BASELINE)" "$(CANDIDATE)"

# -- Coverage ------------------------------------------------------------------

coverage:
	cd matcher_rs && cargo tarpaulin --all-features --out html
	@echo "Coverage report: matcher_rs/tarpaulin-report.html"
