# curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain nightly --profile minimal -y
# python3 -m pip install --user --upgrade pip maturin

# apt install -y libhyperscan-dev

# export PATH="/root/.local/bin:$PATH"
# source "$HOME/.cargo/env"

# cargo clean
cargo build --release --target=x86_64-unknown-linux-gnu
cp ./target/x86_64-unknown-linux-gnu/release/libmatcher_py.so ./matcher_py/matcher_py/matcher_py.so
cp ./target/x86_64-unknown-linux-gnu/release/libmatcher_c.so ./matcher_c/matcher_c.so
cp ./target/x86_64-unknown-linux-gnu/release/libmatcher_c.so ./matcher_java/src/main/resources/matcher_c.so