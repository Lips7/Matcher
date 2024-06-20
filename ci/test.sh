ruff format .
pytest

cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc