use aarch64_cpu::registers::*;

use crate::{mem::units::VirtAddr, println};

core::arch::global_asm!(
    r###"
.section .text.vectors
.align 11                        // 2 KiB – required by the architecture
.global __exception_vectors
__exception_vectors:

/* ---------- Current EL with SP_EL0 ---------- */
    /* 0x000: Synchronous */
    b   __sync_current_el_sp0
    .org 0x080                       // next slot – 128 B stride
    /* 0x080: IRQ */
    b   __irq_current_el_sp0
    .org 0x100
    /* 0x100: FIQ */
    b   __fiq_current_el_sp0
    .org 0x180
    /* 0x180: SError */
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

    /* Pad the remainder (0x800 total) */
    .org 0x800
"###
);

unsafe extern "C" {
    unsafe static __exception_vectors: u8;
}

pub unsafe fn exception_vector_table() -> VirtAddr {
    unsafe { VirtAddr::new_unchecked(&__exception_vectors as *const _ as usize) }
}

pub unsafe fn init() {
    unsafe {
        let addr = exception_vector_table().value();

        core::arch::asm!("
        msr vbar_el1, {vec}
        isb
        ", vec = in(reg) addr, options(nomem, nostack, preserves_flags))
    }
}

#[derive(Default)]
#[repr(C, packed)]
pub struct IretRegs {
    pub sp_el0: usize,
    pub esr_el1: usize,
    pub spsr_el1: usize,
    pub elr_el1: usize,
}

impl IretRegs {
    pub fn dump(&self) {
        println!("ELR_EL1: {:>016X}", { self.elr_el1 });
        println!("SPSR_EL1: {:>016X}", { self.spsr_el1 });
        println!("ESR_EL1: {:>016X}", { self.esr_el1 });
        println!("SP_EL0: {:>016X}", { self.sp_el0 });
    }
}

#[derive(Default)]
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
    pub _padding: usize,
}

impl ScratchRegs {
    pub fn dump(&self) {
        println!("X0:    {:>016X}", { self.x0 });
        println!("X1:    {:>016X}", { self.x1 });
        println!("X2:    {:>016X}", { self.x2 });
        println!("X3:    {:>016X}", { self.x3 });
        println!("X4:    {:>016X}", { self.x4 });
        println!("X5:    {:>016X}", { self.x5 });
        println!("X6:    {:>016X}", { self.x6 });
        println!("X7:    {:>016X}", { self.x7 });
        println!("X8:    {:>016X}", { self.x8 });
        println!("X9:    {:>016X}", { self.x9 });
        println!("X10:   {:>016X}", { self.x10 });
        println!("X11:   {:>016X}", { self.x11 });
        println!("X12:   {:>016X}", { self.x12 });
        println!("X13:   {:>016X}", { self.x13 });
        println!("X14:   {:>016X}", { self.x14 });
        println!("X15:   {:>016X}", { self.x15 });
        println!("X16:   {:>016X}", { self.x16 });
        println!("X17:   {:>016X}", { self.x17 });
        println!("X18:   {:>016X}", { self.x18 });
    }
}

#[derive(Default)]
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
        println!("X19:   {:>016X}", { self.x19 });
        println!("X20:   {:>016X}", { self.x20 });
        println!("X21:   {:>016X}", { self.x21 });
        println!("X22:   {:>016X}", { self.x22 });
        println!("X23:   {:>016X}", { self.x23 });
        println!("X24:   {:>016X}", { self.x24 });
        println!("X25:   {:>016X}", { self.x25 });
        println!("X26:   {:>016X}", { self.x26 });
        println!("X27:   {:>016X}", { self.x27 });
        println!("X28:   {:>016X}", { self.x28 });
        println!("X29:   {:>016X}", { self.x29 });
        println!("X30:   {:>016X}", { self.x30 });
    }
}

#[derive(Default)]
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

pub fn exception_code(esr: usize) -> u8 {
    ((esr >> 26) & 0x3f) as u8
}

exception_stack!(__sync_current_el_sp0, |stack| {
    stack.dump();
    panic!("{}", stringify!(__sync_current_el_sp0))
});
exception_stack!(__irq_current_el_sp0, |stack| {
    stack.dump();
    panic!("{}", stringify!(__irq_current_el_sp0))
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
    println!("SYNCHRONOUS EXCEPTION (current EL, SPX)");
    let error_code = exception_code(stack.iret.esr_el1);
    println!("Code: {:#x}", error_code);
    if error_code == 0x25 {
        println!("Translation Fault");
        let faulted_addr = unsafe { VirtAddr::new_unchecked(FAR_EL1.get() as usize) };
        println!("Faulted addr: {}", faulted_addr);

        let iss = stack.iret.esr_el1 & 0x1ffffff;
        let wn_r = (iss >> 6) & 1 == 1;
        let dfsc = iss & 0x3f;

        match dfsc {
            0b000000..=0b000011 => page_not_present(faulted_addr, wn_r, dfsc),
            0b001101..=0b001111 => permission_fault(faulted_addr, wn_r, dfsc),
            0b001001..=0b001011 => access_flag_fault(faulted_addr, wn_r, dfsc),
            _ => unhandled_fault(faulted_addr, wn_r, dfsc),
        }
    }
    println!("-----------------");
    stack.dump();
    panic!("{}", stringify!(__sync_current_el_spx))
});
exception_stack!(__irq_current_el_spx, |stack| {
    stack.dump();
    panic!("{}", stringify!(__irq_current_el_spx))
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
    stack.dump();
    panic!("{}", stringify!(__sync_lower_el_a64))
});
exception_stack!(__irq_lower_el_a64, |stack| {
    stack.dump();
    panic!("{}", stringify!(__irq_lower_el_a64))
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
exception_stack!(__irq_lower_el_a32, |stack| {
    stack.dump();
    panic!("{}", stringify!(__irq_lower_el_a32))
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
    println!("Page not present (write = {})", caused_by_write);
}
fn permission_fault(_faulted_addr: VirtAddr, caused_by_write: bool, _dfsc: usize) {
    println!("Permission fault (write = {})", caused_by_write);
}
fn access_flag_fault(_faulted_addr: VirtAddr, caused_by_write: bool, _dfsc: usize) {
    println!("Access flag fault (write = {})", caused_by_write);
}
fn unhandled_fault(_faulted_addr: VirtAddr, caused_by_write: bool, dfsc: usize) {
    println!("Unhandled fault (write = {})", caused_by_write);
    println!("dfsc: {:#b}", dfsc);
}
