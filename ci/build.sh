cargo build --release --target=aarch64-apple-darwin
cp ./target/aarch64-apple-darwin/release/libmatcher_py.dylib ./matcher_py/matcher_py/matcher_py.so
cp ./target/aarch64-apple-darwin/release/libmatcher_c.dylib ./matcher_c/matcher_c.so
cp ./target/aarch64-apple-darwin/release/libmatcher_c.dylib ./matcher_java/src/main/resources/matcher_c.so