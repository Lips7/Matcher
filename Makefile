build:
	cargo update
	cargo build --release
	$(eval OS := $(shell uname -s | tr A-Z a-z))
	$(eval EXT := $(shell if [ "$(OS)" = "darwin" ]; then echo "dylib"; elif [ "$(OS)" = "linux" ]; then echo "so"; else echo "dll"; fi))
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/matcher_c.so
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_java/src/main/resources/matcher_c.so

test:
	cargo fmt --all
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo doc

	cd matcher_rs && cargo all-features test

	cd matcher_py && unset CONDA_PREFIX && uv run maturin develop && uv run pytest

update:
	cargo update --verbose --recursive --breaking
	cargo upgrade --verbose --recursive