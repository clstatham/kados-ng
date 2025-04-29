use core::arch::{asm, global_asm};

pub mod serial;

global_asm!(include_str!("boot.S"));

pub fn exit_qemu(code: u32) -> ! {
    use qemu_exit::QEMUExit;
    qemu_exit::AArch64::new().exit(code);
}

pub fn halt_loop() -> ! {
    loop {
        unsafe { asm!("wfi", options(nomem, nostack, preserves_flags)) }
    }
}
