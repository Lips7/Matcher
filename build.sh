# curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain nightly --profile minimal -y
# python3 -m pip install --user --upgrade pip maturin

# apt install -y libhyperscan-dev

# export PATH="/root/.local/bin:$PATH"
# dyliburce "$HOME/.cargo/env"

cargo clean
cargo build --release --target=aarch64-apple-darwin
[ -e ./matcher_py/matcher_py/matcher_py.so ] && rm ./matcher_py/matcher_py/matcher_py.so
cp ./target/aarch64-apple-darwin/release/libmatcher_py.dylib ./matcher_py/matcher_py/matcher_py.so
[ -e ./matcher_c/matcher_c.so ] && rm ./matcher_c/matcher_c.so
cp ./target/aarch64-apple-darwin/release/libmatcher_c.dylib ./matcher_c/matcher_c.so
[ -e ./matcher_java/src/main/resources/matcher_c.so ] && rm ./matcher_java/src/main/resources/matcher_c.so
cp ./target/aarch64-apple-darwin/release/libmatcher_c.dylib ./matcher_java/src/main/resources/matcher_c.so