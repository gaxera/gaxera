use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target == "x86_64-unknown-none" {
        println!("cargo:rustc-link-arg=-Tkernel/src/arch/x86_64/linker.ld");
        println!("cargo:rerun-if-changed=src/arch/x86_64/linker.ld");
    }
}
