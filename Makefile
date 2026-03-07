build:
	$(eval OS := $(shell uname -s | tr A-Z a-z))
	$(eval EXT := $(shell if [ "$(OS)" = "darwin" ]; then echo "dylib"; elif [ "$(OS)" = "linux" ]; then echo "so"; else echo "dll"; fi))

	cargo update
	cargo build --release
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/matcher_c.$(EXT)
	cp ./target/release/libmatcher_java.$(EXT) ./matcher_java/src/main/resources/libmatcher_java.$(EXT)

lint:
	cargo fmt --all
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo doc

	cd matcher_py && uv run ruff check --fix && uv run ty check
test:
	cd matcher_rs && cargo all-features test
	cd matcher_java && mvn test

	cd matcher_py && unset CONDA_PREFIX && uv run maturin develop && uv run pytest

update:
	cargo update --verbose --recursive --breaking -Z unstable-options
	cargo upgrade --verbose --recursive