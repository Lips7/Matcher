use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("target_os not defined!");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target_arch not defined!");

    if target_os == "linux" && target_arch == "arm" {
        println!("cargo:rustc-link-lib=atomic");
    }
}