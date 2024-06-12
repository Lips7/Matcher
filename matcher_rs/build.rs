fn main() {
    println!("cargo:rustc-env=RUSTFLAGS={}", "-C target-feature=+aes");
}
