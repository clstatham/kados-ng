fn main() {
    println!("cargo:rerun-if-changed=../../arch/aarch64/linker.ld");
}
