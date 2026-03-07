build:
	$(eval OS := $(shell uname -s | tr A-Z a-z))
	$(eval EXT := $(shell if [ "$(OS)" = "darwin" ]; then echo "dylib"; elif [ "$(OS)" = "linux" ]; then echo "so"; else echo "dll"; fi))

	cargo update
	cargo build --release
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_c/matcher_c.$(EXT)
	cp ./target/release/libmatcher_c.$(EXT) ./matcher_java/src/main/resources/libmatcher_c.$(EXT)

test:
	cargo fmt --all
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo doc

	cd matcher_rs && cargo all-features test
	cd matcher_java && mvn test-compile exec:java -Dexec.classpathScope=test -Dexec.mainClass="com.matcher_java.MatcherJavaExample"

	cd matcher_py && unset CONDA_PREFIX && uv run maturin develop && uv run pytest

update:
	cargo update --verbose --recursive --breaking -Z unstable-options
	cargo upgrade --verbose --recursive