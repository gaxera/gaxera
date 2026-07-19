fn main() {
    println!("cargo:rustc-link-arg=-Tkernel/src/arch/x86_64/linker.ld");
    println!("cargo:rerun-if-changed=src/arch/x86_64/linker.ld");
}
