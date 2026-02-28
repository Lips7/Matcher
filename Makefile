build:
	cargo update
	cargo build --release
	$(eval OS := $(shell uname -s | tr A-Z a-z))
	$(eval EXT := $(shell if [ "$(OS)" = "darwin" ]; then echo "dylib"; elif [ "$(OS)" = "linux" ]; then echo "so"; else echo "dll"; fi))
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/matcher_c.so
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_java/src/main/resources/matcher_c.so

test:
	cargo fmt
	cargo clippy --all-targets -- -D warnings
	cargo doc

	cd matcher_rs && cargo test --no-default-features
	cd matcher_rs && cargo test --no-default-features --features "dfa"
	cd matcher_rs && cargo test --no-default-features --features "runtime_build"
	cd matcher_rs && cargo test --no-default-features --features "runtime_build,dfa"

	cd matcher_py && ruff format . && uv sync && pytest

update:
	cargo update --verbose --recursive --breaking
	cargo upgrade --verbose --recursive