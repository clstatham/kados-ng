use core::arch::{asm, global_asm};

pub mod logging;
pub mod serial;
pub mod time;

global_asm!(include_str!("boot.S"));

pub fn exit_qemu(code: u32) -> ! {
    use qemu_exit::QEMUExit;
    qemu_exit::AArch64::new().exit(code)
}

pub fn halt_loop() -> ! {
    loop {
        unsafe { asm!("wfe", options(nomem, nostack, preserves_flags)) }
    }
}
