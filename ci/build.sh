cargo update
cargo build --release
cp ./target/release/libmatcher_c.dylib ./matcher_c/matcher_c.so
cp ./target/release/libmatcher_c.dylib ./matcher_java/src/main/resources/matcher_c.so