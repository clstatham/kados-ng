use aarch64_cpu::asm;

pub mod random;
pub mod serial;
pub mod time;

pub fn exit_qemu(code: u32) -> ! {
    use qemu_exit::QEMUExit;
    qemu_exit::AArch64::new().exit(code)
}

pub fn halt_loop() -> ! {
    loop {
        asm::wfe();
    }
}
