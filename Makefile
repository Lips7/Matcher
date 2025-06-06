build:
	cargo update
	cargo build --release
	cp ./target/release/libmatcher_c.dylib ./matcher_c/matcher_c.so
	cp ./target/release/libmatcher_c.dylib ./matcher_java/src/main/resources/matcher_c.so

test:
	cargo fmt
	cargo clippy --all-targets -- -D warnings
	cargo doc

	cd matcher_rs
	cargo test --no-default-features
	cargo test --no-default-features --features "dfa"
	cargo test --no-default-features --features "runtime_build"
	cargo test --no-default-features --features "runtime_build,dfa"
	cargo test --no-default-features --features "dfa,serde"
	cd ..

	cd matcher_py
	ruff format .
	uv sync
	pytest
	cd ..

update:
	cargo update --verbose --recursive --breaking -Z unstable-options
	cargo upgrade --verbose --recursive