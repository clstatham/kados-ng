use aarch64_cpu::registers::{FAR_EL1, Readable};

use crate::irq::irq_chip;
use crate::mem::paging::table::{PageTable, TableKind};
use crate::mem::units::VirtAddr;

core::arch::global_asm!(
    r#"
.section .text.vectors
.align 11                        
.global __exception_vectors
__exception_vectors:

/* ---------- Current EL with SP_EL0 ---------- */
    b   __sync_current_el_sp0
    .org 0x080                       
    b   __irq_current_el_sp0
    .org 0x100
    b   __fiq_current_el_sp0
    .org 0x180
    b   __serr_current_el_sp0

/* ---------- Current EL with SP_ELx ---------- */
    .org 0x200
    b   __sync_current_el_spx
    .org 0x280
    b   __irq_current_el_spx
    .org 0x300
    b   __fiq_current_el_spx
    .org 0x380
    b   __serr_current_el_spx

/* ---------- Lower EL, AArch64 ---------- */
    .org 0x400
    b   __sync_lower_el_a64
    .org 0x480
    b   __irq_lower_el_a64
    .org 0x500
    b   __fiq_lower_el_a64
    .org 0x580
    b   __serr_lower_el_a64

/* ---------- Lower EL, AArch32 ---------- */
    .org 0x600
    b   __sync_lower_el_a32
    .org 0x680
    b   __irq_lower_el_a32
    .org 0x700
    b   __fiq_lower_el_a32
    .org 0x780
    b   __serr_lower_el_a32

    .org 0x800
"#
);

unsafe extern "C" {
    unsafe static __exception_vectors: u8;
}

/// Returns the address of the exception vector table.
#[must_use]
pub unsafe fn exception_vector_table() -> VirtAddr {
    unsafe { VirtAddr::new_unchecked(&raw const __exception_vectors as usize) }
}

/// Registers used for returning from an interrupt or exception.
#[derive(Default, Clone, Copy)]
#[repr(C, packed)]
pub struct IretRegs {
    /// The stack pointer used when returning to user mode.
    pub sp_el0: usize,
    /// The exception syndrome register for EL1, which contains information about the exception.
    pub esr_el1: usize,
    /// The saved program status register for EL1, which contains the state of the processor at the time of the exception.
    pub spsr_el1: usize,
    /// The link register for EL1, which is used to return from the exception.
    pub elr_el1: usize,
}

impl IretRegs {
    pub fn dump(&self) {
        log::error!("ELR_EL1: {:>016X}", { self.elr_el1 });
        log::error!("SPSR_EL1: {:>016X}", { self.spsr_el1 });
        log::error!("ESR_EL1: {:>016X}", { self.esr_el1 });
        log::error!("SP_EL0: {:>016X}", { self.sp_el0 });
    }
}

/// Caller-saved registers used for scratch space during interrupts.
#[derive(Default, Clone, Copy)]
#[repr(C, packed)]
pub struct ScratchRegs {
    pub x0: usize,
    pub x1: usize,
    pub x2: usize,
    pub x3: usize,
    pub x4: usize,
    pub x5: usize,
    pub x6: usize,
    pub x7: usize,
    pub x8: usize,
    pub x9: usize,
    pub x10: usize,
    pub x11: usize,
    pub x12: usize,
    pub x13: usize,
    pub x14: usize,
    pub x15: usize,
    pub x16: usize,
    pub x17: usize,
    pub x18: usize,
    _padding: usize,
}

impl ScratchRegs {
    pub fn dump(&self) {
        log::error!("X0:    {:>016X}", { self.x0 });
        log::error!("X1:    {:>016X}", { self.x1 });
        log::error!("X2:    {:>016X}", { self.x2 });
        log::error!("X3:    {:>016X}", { self.x3 });
        log::error!("X4:    {:>016X}", { self.x4 });
        log::error!("X5:    {:>016X}", { self.x5 });
        log::error!("X6:    {:>016X}", { self.x6 });
        log::error!("X7:    {:>016X}", { self.x7 });
        log::error!("X8:    {:>016X}", { self.x8 });
        log::error!("X9:    {:>016X}", { self.x9 });
        log::error!("X10:   {:>016X}", { self.x10 });
        log::error!("X11:   {:>016X}", { self.x11 });
        log::error!("X12:   {:>016X}", { self.x12 });
        log::error!("X13:   {:>016X}", { self.x13 });
        log::error!("X14:   {:>016X}", { self.x14 });
        log::error!("X15:   {:>016X}", { self.x15 });
        log::error!("X16:   {:>016X}", { self.x16 });
        log::error!("X17:   {:>016X}", { self.x17 });
        log::error!("X18:   {:>016X}", { self.x18 });
    }
}

/// Callee-saved registers that are preserved across interrupts.
#[derive(Default, Clone, Copy)]
#[repr(C, packed)]
pub struct PreservedRegs {
    pub x19: usize,
    pub x20: usize,
    pub x21: usize,
    pub x22: usize,
    pub x23: usize,
    pub x24: usize,
    pub x25: usize,
    pub x26: usize,
    pub x27: usize,
    pub x28: usize,
    pub x29: usize,
    pub x30: usize,
}

impl PreservedRegs {
    pub fn dump(&self) {
        log::error!("X19:   {:>016X}", { self.x19 });
        log::error!("X20:   {:>016X}", { self.x20 });
        log::error!("X21:   {:>016X}", { self.x21 });
        log::error!("X22:   {:>016X}", { self.x22 });
        log::error!("X23:   {:>016X}", { self.x23 });
        log::error!("X24:   {:>016X}", { self.x24 });
        log::error!("X25:   {:>016X}", { self.x25 });
        log::error!("X26:   {:>016X}", { self.x26 });
        log::error!("X27:   {:>016X}", { self.x27 });
        log::error!("X28:   {:>016X}", { self.x28 });
        log::error!("X29:   {:>016X}", { self.x29 });
        log::error!("X30:   {:>016X}", { self.x30 });
    }
}

#[derive(Default, Clone, Copy)]
#[repr(C, packed)]
pub struct InterruptFrame {
    pub iret: IretRegs,
    pub scratch: ScratchRegs,
    pub preserved: PreservedRegs,
}

impl InterruptFrame {
    pub fn set_stack_pointer(&mut self, sp: usize) {
        self.iret.sp_el0 = sp;
    }

    pub fn set_instr_pointer(&mut self, pc: usize) {
        self.iret.elr_el1 = pc;
    }

    #[must_use]
    pub fn stack_pointer(&self) -> usize {
        self.iret.sp_el0
    }

    #[must_use]
    pub fn instr_pointer(&self) -> usize {
        self.iret.elr_el1
    }

    pub fn dump(&self) {
        self.iret.dump();
        self.scratch.dump();
        self.preserved.dump();
    }
}

#[macro_export]
macro_rules! push_scratch {
    () => {
        "
        str     x18,      [sp, #-16]!
        stp     x16, x17, [sp, #-16]!
        stp     x14, x15, [sp, #-16]!
        stp     x12, x13, [sp, #-16]!
        stp     x10, x11, [sp, #-16]!
        stp     x8, x9, [sp, #-16]!
        stp     x6, x7, [sp, #-16]!
        stp     x4, x5, [sp, #-16]!
        stp     x2, x3, [sp, #-16]!
        stp     x0, x1, [sp, #-16]!
    "
    };
}

#[macro_export]
macro_rules! pop_scratch {
    () => {
        "
        ldp     x0, x1, [sp], #16
        ldp     x2, x3, [sp], #16
        ldp     x4, x5, [sp], #16
        ldp     x6, x7, [sp], #16
        ldp     x8, x9, [sp], #16
        ldp     x10, x11, [sp], #16
        ldp     x12, x13, [sp], #16
        ldp     x14, x15, [sp], #16
        ldp     x16, x17, [sp], #16
        ldr     x18,      [sp], #16
    "
    };
}

#[macro_export]
macro_rules! push_preserved {
    () => {
        "
        stp     x29, x30, [sp, #-16]!
        stp     x27, x28, [sp, #-16]!
        stp     x25, x26, [sp, #-16]!
        stp     x23, x24, [sp, #-16]!
        stp     x21, x22, [sp, #-16]!
        stp     x19, x20, [sp, #-16]!
    "
    };
}

#[macro_export]
macro_rules! pop_preserved {
    () => {
        "
        ldp     x19, x20, [sp], #16
        ldp     x21, x22, [sp], #16
        ldp     x23, x24, [sp], #16
        ldp     x25, x26, [sp], #16
        ldp     x27, x28, [sp], #16
        ldp     x29, x30, [sp], #16
    "
    };
}

#[macro_export]
macro_rules! push_special {
    () => {
        "
        mrs     x14, spsr_el1
        mrs     x15, elr_el1
        stp     x14, x15, [sp, #-16]!

        mrs     x14, sp_el0
        mrs     x15, esr_el1
        stp     x14, x15, [sp, #-16]!
    "
    };
}

#[macro_export]
macro_rules! pop_special {
    () => {
        "
        ldp     x14, x15, [sp], 16
        msr     esr_el1, x15
        msr     sp_el0, x14

        ldp     x14, x15, [sp], 16
        msr     elr_el1, x15
        msr     spsr_el1, x14
    "
    };
}

#[macro_export]
macro_rules! exception_stack {
    ($name:ident, |$stack:ident| $code:block) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(stack: &mut InterruptFrame) {
            unsafe extern "C" fn inner($stack: &mut InterruptFrame) {
                $code
            }
            core::arch::naked_asm!(concat!(
                // Backup all userspace registers to stack
                push_preserved!(),
                push_scratch!(),
                push_special!(),

                // Call inner function with pointer to stack
                "mov x29, sp\n",
                "mov x0, sp\n",
                "bl {}",

                // Restore all userspace registers
                pop_special!(),
                pop_scratch!(),
                pop_preserved!(),

                "eret\n",
            ), sym inner);
        }
    };
}

#[unsafe(naked)]
pub unsafe extern "C" fn enter_usermode() -> ! {
    core::arch::naked_asm!(concat!(
        "blr x28\n",
        // Restore all userspace registers
        pop_special!(),
        pop_scratch!(),
        pop_preserved!(),
        "eret\n",
    ));
}

#[must_use]
pub fn exception_code(esr: usize) -> u8 {
    ((esr >> 26) & 0x3f) as u8
}

exception_stack!(__sync_current_el_sp0, |stack| {
    stack.dump();
    panic!("{}", stringify!(__sync_current_el_sp0))
});
exception_stack!(__irq_current_el_sp0, |_stack| {
    handle_irq();
});
exception_stack!(__fiq_current_el_sp0, |stack| {
    stack.dump();
    panic!("{}", stringify!(__fiq_current_el_sp0))
});
exception_stack!(__serr_current_el_sp0, |stack| {
    stack.dump();
    panic!("{}", stringify!(__serr_current_el_sp0))
});
exception_stack!(__sync_current_el_spx, |stack| {
    let error_code = exception_code(stack.iret.esr_el1);
    // if error_code == 0x3c {
    //     super::debugging::on_irq(stack, StopReason::SwBreakpoint);
    //     return;
    // } else if error_code == 0x0e {
    //     super::debugging::on_irq(stack, StopReason::HwBreakpoint);
    //     return;
    // }
    log::error!("SYNCHRONOUS EXCEPTION (current EL, SPX)");
    log::error!("Code: {error_code:#x}");
    if error_code == 0x25 {
        log::error!("Translation Fault");
        let faulted_addr = unsafe { VirtAddr::new_unchecked(FAR_EL1.get() as usize) };
        log::error!("Faulted addr: {faulted_addr}");

        let iss = stack.iret.esr_el1 & 0x01ff_ffff;
        let wn_r = (iss >> 6) & 1 == 1;
        let dfsc = iss & 0x3f;

        match dfsc {
            0b00_0000..=0b00_0011 => page_not_present(faulted_addr, wn_r, dfsc),
            0b00_1101..=0b00_1111 => permission_fault(faulted_addr, wn_r, dfsc),
            0b00_1001..=0b00_1011 => access_flag_fault(faulted_addr, wn_r, dfsc),
            _ => unhandled_fault(faulted_addr, wn_r, dfsc),
        }
    }
    panic!("{}", stringify!(__sync_current_el_spx))
});
exception_stack!(__irq_current_el_spx, |_stack| {
    handle_irq();
});
exception_stack!(__fiq_current_el_spx, |stack| {
    stack.dump();
    panic!("{}", stringify!(__fiq_current_el_spx))
});
exception_stack!(__serr_current_el_spx, |stack| {
    stack.dump();
    panic!("{}", stringify!(__serr_current_el_spx))
});
exception_stack!(__sync_lower_el_a64, |stack| {
    match exception_code(stack.iret.esr_el1) {
        0b01_0101 => {
            log::debug!("Syscall!");
        }
        code => {
            log::error!("{:#b}", code);
        }
    }

    stack.dump();
    panic!("{}", stringify!(__sync_lower_el_a64))
});
exception_stack!(__irq_lower_el_a64, |_stack| {
    handle_irq();
});
exception_stack!(__fiq_lower_el_a64, |stack| {
    stack.dump();
    panic!("{}", stringify!(__fiq_lower_el_a64))
});
exception_stack!(__serr_lower_el_a64, |stack| {
    stack.dump();
    panic!("{}", stringify!(__serr_lower_el_a64))
});
exception_stack!(__sync_lower_el_a32, |stack| {
    stack.dump();
    panic!("{}", stringify!(__sync_lower_el_a32))
});
exception_stack!(__irq_lower_el_a32, |_stack| {
    handle_irq();
});
exception_stack!(__fiq_lower_el_a32, |stack| {
    stack.dump();
    panic!("{}", stringify!(__fiq_lower_el_a32))
});
exception_stack!(__serr_lower_el_a32, |stack| {
    stack.dump();
    panic!("{}", stringify!(__serr_lower_el_a32))
});

fn page_not_present(_faulted_addr: VirtAddr, caused_by_write: bool, _dfsc: usize) {
    log::error!("Page not present (write = {caused_by_write})");
}
fn permission_fault(_faulted_addr: VirtAddr, caused_by_write: bool, _dfsc: usize) {
    log::error!("Permission fault (write = {caused_by_write})");
}
fn access_flag_fault(_faulted_addr: VirtAddr, caused_by_write: bool, _dfsc: usize) {
    log::error!("Access flag fault (write = {caused_by_write})");
}
fn unhandled_fault(_faulted_addr: VirtAddr, caused_by_write: bool, dfsc: usize) {
    log::error!("Unhandled fault (write = {caused_by_write})");
    log::error!("dfsc: {dfsc:#b}");

    let table = PageTable::current(TableKind::Kernel);
    log::error!("current table: {}", table.phys_addr());
}

fn handle_irq() {
    let mut chip = irq_chip();
    let irq = chip.ack();

    log::trace!("IRQ {irq} caught");
    chip.handle_irq(irq);
    chip.eoi(irq);
}
