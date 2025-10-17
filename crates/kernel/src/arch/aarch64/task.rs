use core::mem::offset_of;

use crate::task::{context::Context, stack::Stack};

use super::vectors::{InterruptFrame, enter_usermode};

/// The architecture-specific context for a task.
#[derive(Debug, Clone, Default)]
#[allow(unused)]
pub struct ArchContext {
    elr_el1: usize,
    sp_el0: usize,
    spsr_el1: usize,
    esr_el1: usize,
    sp: usize,
    lr: usize,
    fp: usize,
    x28: usize,
    x27: usize,
    x26: usize,
    x25: usize,
    x24: usize,
    x23: usize,
    x22: usize,
    x21: usize,
    x20: usize,
    x19: usize,
}

impl ArchContext {
    /// Sets up the entry point for the task's context.
    ///
    /// If `user` is true, it prepares the context for user mode execution,
    /// otherwise it prepares for kernel mode execution.
    pub fn setup_initial_call(&mut self, stack: &Stack, entry_func: extern "C" fn(), user: bool) {
        let mut stack_top = stack.initial_top();

        if user {
            unsafe {
                stack_top = stack_top.sub(size_of::<InterruptFrame>());
                stack_top.write_bytes(0u8, size_of::<InterruptFrame>());
            }
            self.lr = enter_usermode as usize;
            self.x28 = entry_func as usize;
        } else {
            self.lr = entry_func as usize;
        }

        self.sp = stack_top as usize;
    }
}

/// Switches the current task's context to the next task's context.
///
/// # Panics
///
/// This function will panic if there is no current CPU-local block.
pub unsafe fn switch_to(prev: &mut Context, next: &mut Context) {
    unsafe {
        switch_to_inner(&mut prev.arch, &mut next.arch);
    }
}

#[unsafe(naked)]
unsafe extern "C" fn switch_to_inner(_prev: &mut ArchContext, _next: &mut ArchContext) {
    core::arch::naked_asm!(
        "
        str x19, [x0, #{off_x19}]
        ldr x19, [x1, #{off_x19}]

        str x20, [x0, #{off_x20}]
        ldr x20, [x1, #{off_x20}]

        str x21, [x0, #{off_x21}]
        ldr x21, [x1, #{off_x21}]

        str x22, [x0, #{off_x22}]
        ldr x22, [x1, #{off_x22}]

        str x23, [x0, #{off_x23}]
        ldr x23, [x1, #{off_x23}]

        str x24, [x0, #{off_x24}]
        ldr x24, [x1, #{off_x24}]

        str x25, [x0, #{off_x25}]
        ldr x25, [x1, #{off_x25}]

        str x26, [x0, #{off_x26}]
        ldr x26, [x1, #{off_x26}]

        str x27, [x0, #{off_x27}]
        ldr x27, [x1, #{off_x27}]

        str x28, [x0, #{off_x28}]
        ldr x28, [x1, #{off_x28}]

        str x29, [x0, #{off_x29}]
        ldr x29, [x1, #{off_x29}]

        str x30, [x0, #{off_x30}]
        ldr x30, [x1, #{off_x30}]

        mrs x2, elr_el1
        str x2, [x0, #{off_elr_el1}]
        ldr x2, [x1, #{off_elr_el1}]
        msr elr_el1, x2

        mrs x2, sp_el0
        str x2, [x0, #{off_sp_el0}]
        ldr x2, [x1, #{off_sp_el0}]
        msr sp_el0, x2

        mrs x2, spsr_el1
        str x2, [x0, #{off_spsr_el1}]
        ldr x2, [x1, #{off_spsr_el1}]
        msr spsr_el1, x2

        mrs x2, esr_el1
        str x2, [x0, #{off_esr_el1}]
        ldr x2, [x1, #{off_esr_el1}]
        msr esr_el1, x2

        mov x2, sp
        str x2, [x0, #{off_sp}]
        ldr x2, [x1, #{off_sp}]
        mov sp, x2

        b {switch_hook}
        ",
        off_x19 = const(offset_of!(ArchContext, x19)),
        off_x20 = const(offset_of!(ArchContext, x20)),
        off_x21 = const(offset_of!(ArchContext, x21)),
        off_x22 = const(offset_of!(ArchContext, x22)),
        off_x23 = const(offset_of!(ArchContext, x23)),
        off_x24 = const(offset_of!(ArchContext, x24)),
        off_x25 = const(offset_of!(ArchContext, x25)),
        off_x26 = const(offset_of!(ArchContext, x26)),
        off_x27 = const(offset_of!(ArchContext, x27)),
        off_x28 = const(offset_of!(ArchContext, x28)),
        off_x29 = const(offset_of!(ArchContext, fp)),
        off_x30 = const(offset_of!(ArchContext, lr)),
        off_elr_el1 = const(offset_of!(ArchContext, elr_el1)),
        off_sp_el0 = const(offset_of!(ArchContext, sp_el0)),
        off_spsr_el1 = const(offset_of!(ArchContext, spsr_el1)),
        off_esr_el1 = const(offset_of!(ArchContext, esr_el1)),
        off_sp = const(offset_of!(ArchContext, sp)),

        switch_hook = sym crate::task::switch::switch_finish_hook,
    );
}
