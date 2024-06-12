fn main() {
    println!("cargo:rustc-env=RUSTFLAGS={}", "-C target-feature=+aes");
    #[allow(unexpected_cfgs)]
    if cfg!(matcher_rs_docs_rs) {
        println!("cargo:rustc-env=RUSTFLAGS={}", "-C target-feature=+aes");
    }
}
