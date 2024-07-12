cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features "serde"
cargo doc

cd matcher_py
unset CONDA_PREFIX
maturin develop
ruff format .
pytest

cd ..