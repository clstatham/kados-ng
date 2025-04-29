use core::arch::{asm, global_asm};

use aarch64_cpu::{asm, registers::*};

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

#[unsafe(no_mangle)]
pub extern "C" fn arch_main(stack_end: u64) -> ! {
    // enable timer counters for EL1
    CNTHCTL_EL2.write(CNTHCTL_EL2::EL1PCEN::SET + CNTHCTL_EL2::EL1PCTEN::SET);
    // no offset
    CNTVOFF_EL2.set(0);

    // set EL1 execution state to AARCH64
    HCR_EL2.write(HCR_EL2::RW::EL1IsAarch64);

    // fake saved program status with all interrupts masked
    SPSR_EL2.write(
        SPSR_EL2::D::Masked
            + SPSR_EL2::A::Masked
            + SPSR_EL2::I::Masked
            + SPSR_EL2::F::Masked
            + SPSR_EL2::M::EL1h, // use EL1 stack pointer
    );

    // set the link register to the kernel entry point
    ELR_EL2.set(crate::kernel_main as *const extern "C" fn() as u64);

    SP_EL1.set(stack_end);

    // "return" to EL1
    asm::eret()
}
