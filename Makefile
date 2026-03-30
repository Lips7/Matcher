OS  := $(shell uname -s | tr A-Z a-z)
EXT := $(shell \
	if [ "$(OS)" = "darwin" ]; then echo "dylib"; \
	elif [ "$(OS)" = "linux" ]; then echo "so"; \
	else echo "dll"; fi)

.PHONY: build update lint lint-rs lint-py lint-java test test-rs test-py test-java test-c \
	bench-search bench-build bench-engine-search bench-engine-build bench-compare

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

# -- Lint ----------------------------------------------------------------------

lint: lint-rs lint-py lint-java

lint-rs:
	cargo fmt --all
	cargo all-features clippy --workspace --all-targets -- -D warnings

lint-py:
	cd matcher_py && uv run ruff check --fix && uv run ty check

lint-java:
	cd matcher_java && mvn checkstyle:check

# -- Test ----------------------------------------------------------------------

test: test-rs test-py test-java test-c

test-rs: lint-rs
	cargo doc
	cd matcher_rs && cargo all-features nextest run
	cargo test --doc

test-py: lint-py
	cd matcher_py && uv run pytest

test-java: lint-java
	cargo build --release
	mkdir -p ./matcher_java/src/main/resources
	cp ./target/release/libmatcher_java.$(EXT) ./matcher_java/src/main/resources/libmatcher_java.$(EXT)
	cd matcher_java && mvn test

test-c:
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
