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
unset CONDA_PREFIX
maturin develop
ruff format .
pytest
cd ..