fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/arch/aarch64/linker.ld");
    println!("cargo:rerun-if-changed=../bootloader/src/arch/aarch64/linker.ld");
    println!("cargo:rerun-if-changed=../bootloader/src/lib.rs");
}
