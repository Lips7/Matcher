build:
	$(eval OS := $(shell uname -s | tr A-Z a-z))
	$(eval EXT := $(shell if [ "$(OS)" = "darwin" ]; then echo "dylib"; elif [ "$(OS)" = "linux" ]; then echo "so"; else echo "dll"; fi))

	cargo update
	cargo build --release
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/libmatcher_c.$(EXT)
	mkdir -p ./matcher_java/src/main/resources
	cp ./target/release/libmatcher_java.$(EXT) ./matcher_java/src/main/resources/libmatcher_java.$(EXT)

	rm -rf ./target/wheels
	cd matcher_py && unset CONDA_PREFIX && uv run maturin build --release && uv run pip3 install ../target/wheels/*.whl

update:
	cargo update --verbose --recursive --breaking -Z unstable-options
	cargo upgrade --verbose --recursive

lint-rs:
	cargo fmt --all
	cargo clippy --workspace --all-targets --all-features -- -D warnings

lint-py:
	cd matcher_py && uv run ruff check --fix && uv run ty check

lint-java:
	cd matcher_java && mvn checkstyle:check

lint:
	$(MAKE) lint-rs
	$(MAKE) lint-py
	$(MAKE) lint-java

test-rs:
	$(MAKE) lint-rs
	cargo doc
	cd matcher_rs && cargo all-features test

test-py:
	$(MAKE) lint-py
	cd matcher_py && unset CONDA_PREFIX && uv run maturin develop && uv run pytest

test-java:
	$(MAKE) lint-java
	cd matcher_java && mvn test

test-c:
	$(CC) -Wall -Wextra -L./matcher_c -Wl,-rpath,./matcher_c -lmatcher_c -I./matcher_c matcher_c/tests/test_matcher.c -o matcher_c/tests/test_matcher
	./matcher_c/tests/test_matcher

test:
	$(MAKE) test-rs
	$(MAKE) test-py
	$(MAKE) test-java
	$(MAKE) test-c